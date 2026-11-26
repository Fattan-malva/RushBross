use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::sync::mpsc::{self, Receiver};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

pub struct PtyTerminal {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    buffer: TerminalBuffer,
    cols: u16,
    rows: u16,
}

impl PtyTerminal {
    pub fn start<F>(folder: &str, on_output: F) -> Result<Self, String>
    where
        F: Fn() + Send + Sync + 'static,
    {
        if !std::path::Path::new(folder).is_dir() {
            return Err(format!("Folder tidak ditemukan: {folder}"));
        }
        let cols = 100u16;
        let rows = 30u16;
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())?;
        let shell_path = std::env::var_os("COMSPEC")
            .filter(|path| std::path::Path::new(path).is_file())
            .or_else(|| {
                std::env::var_os("SystemRoot")
                    .map(std::path::PathBuf::from)
                    .map(|root| root.join("System32").join("cmd.exe"))
                    .filter(|path| path.is_file())
                    .map(std::path::PathBuf::into_os_string)
            })
            .unwrap_or_else(|| "cmd.exe".into());
        let shell_path = shell_path.to_string_lossy().into_owned();
        let mut command = CommandBuilder::new(&shell_path);
        command.args(["/D", "/Q", "/K"]);
        command.env("PROMPT", "$P$G");
        command.env("TERM", "xterm-256color");
        command.cwd(folder);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let (tx, rx) = mpsc::channel();
        let on_output = std::sync::Arc::new(on_output);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        // BEL (0x07) tetap dikirim: ia penutup urutan OSC (judul jendela)
                        // dan diabaikan oleh parser. Membuangnya membuat parser macet di OSC.
                        let data: Vec<u8> = chunk[..count].to_vec();
                        if !data.is_empty() {
                            if tx.send(data).is_err() {
                                break;
                            }
                            on_output();
                        }
                    }
                }
            }
        });
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;

        Ok(Self {
            master: pair.master,
            child,
            writer,
            output: rx,
            buffer: TerminalBuffer::new(cols as usize, rows as usize),
            cols,
            rows,
        })
    }

    pub fn send(&mut self, text: &str) {
        let _ = self.writer.write_all(text.as_bytes());
        let _ = self.writer.flush();
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 || (cols == self.cols && rows == self.rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.buffer.resize(cols as usize, rows as usize);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn pump(&mut self) {
        while let Ok(chunk) = self.output.try_recv() {
            self.buffer.feed(&chunk);
        }
    }

    pub fn lines(&self) -> &[String] {
        self.buffer.lines()
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)) | Err(_))
    }

    pub fn kill(&mut self) {
        if let Some(pid) = self.child.process_id() {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .output();
        }
        let _ = self.child.kill();
    }
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Normal,
    Escape,
    Csi,
    Osc,
    /// Baru saja melihat ESC di dalam OSC; karakter berikutnya `\` menutup OSC (ST).
    OscEscape,
    /// Setelah `ESC (`, `ESC )`, dst: satu karakter pemilih charset dilewati.
    Charset,
}

struct TerminalBuffer {
    lines: Vec<String>,
    x: usize,
    y: usize,
    cols: usize,
    rows: usize,
    mode: Mode,
    csi: String,
}

impl TerminalBuffer {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            lines: vec![String::new()],
            x: 0,
            y: 0,
            cols,
            rows,
            mode: Mode::Normal,
            csi: String::new(),
        }
    }

    fn lines(&self) -> &[String] {
        &self.lines
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.x = self.x.min(cols.saturating_sub(1));
        self.y = self.y.min(rows.saturating_sub(1));
        if self.lines.len() > rows {
            self.lines.drain(0..self.lines.len() - rows);
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for ch in text.chars() {
            match self.mode {
                Mode::Osc => match ch {
                    '\x07' => self.mode = Mode::Normal,
                    '\x1b' => self.mode = Mode::OscEscape,
                    _ => {}
                },
                Mode::OscEscape => self.mode = Mode::Normal,
                Mode::Charset => self.mode = Mode::Normal,
                Mode::Escape => match ch {
                    '[' => {
                        self.csi.clear();
                        self.mode = Mode::Csi;
                    }
                    ']' => self.mode = Mode::Osc,
                    '(' | ')' | '*' | '+' | '#' | '%' => self.mode = Mode::Charset,
                    _ => self.mode = Mode::Normal,
                },
                Mode::Csi => {
                    if ('@'..='~').contains(&ch) {
                        self.handle_csi(ch);
                        self.mode = Mode::Normal;
                    } else {
                        self.csi.push(ch);
                    }
                }
                Mode::Normal => match ch {
                    '\x1b' => self.mode = Mode::Escape,
                    '\r' => self.x = 0,
                    '\n' => self.new_line(),
                    '\x08' => self.x = self.x.saturating_sub(1),
                    '\t' => self.x = ((self.x / 8) + 1) * 8,
                    ch if !ch.is_control() => self.put(ch),
                    _ => {}
                },
            }
        }
    }

    fn handle_csi(&mut self, final_byte: char) {
        let params: Vec<usize> = self
            .csi
            .split(';')
            .filter_map(|part| part.parse::<usize>().ok())
            .collect();
        let first = params.first().copied().unwrap_or(1).max(1);
        match final_byte {
            'A' => self.y = self.y.saturating_sub(first),
            'B' => self.y = (self.y + first).min(self.rows - 1),
            'C' => self.x = (self.x + first).min(self.cols - 1),
            'D' => self.x = self.x.saturating_sub(first),
            'G' => self.x = first.saturating_sub(1).min(self.cols - 1),
            'H' | 'f' => {
                self.y = first.saturating_sub(1).min(self.rows - 1);
                self.x = params
                    .get(1)
                    .copied()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(self.cols - 1);
            }
            'K' => self.erase_line(),
            'J' => {
                self.lines.clear();
                self.lines.push(String::new());
                self.x = 0;
                self.y = 0;
            }
            _ => {}
        }
    }

    fn put(&mut self, ch: char) {
        while self.lines.len() <= self.y {
            self.lines.push(String::new());
        }
        if self.x >= self.cols {
            self.new_line();
        }
        while self.lines.len() <= self.y {
            self.lines.push(String::new());
        }
        let mut chars: Vec<char> = self.lines[self.y].chars().collect();
        if chars.len() <= self.x {
            chars.resize(self.x + 1, ' ');
        }
        chars[self.x] = ch;
        self.lines[self.y] = chars.into_iter().collect();
        self.x += 1;
    }

    fn erase_line(&mut self) {
        if let Some(line) = self.lines.get_mut(self.y) {
            let mut chars: Vec<char> = line.chars().collect();
            for cell in chars.iter_mut().skip(self.x) {
                *cell = ' ';
            }
            *line = chars.into_iter().collect();
        }
    }

    fn new_line(&mut self) {
        self.y += 1;
        self.x = 0;
        if self.lines.len() <= self.y {
            self.lines.push(String::new());
        }
        if self.lines.len() > self.rows {
            self.lines.remove(0);
            self.y = self.y.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn buffer_keeps_output_after_osc_title_terminated_by_bel_or_st() {
        let mut buffer = TerminalBuffer::new(100, 30);
        buffer.feed(b"\x1b]0;C:\\RushBross\x07C:\\RushBross>echo hi\r\nhi\r\n");
        buffer.feed(b"\x1b]0;title\x1b\\prompt> ");
        assert_eq!(buffer.lines()[0], "C:\\RushBross>echo hi");
        assert_eq!(buffer.lines()[1], "hi");
        assert_eq!(buffer.lines()[2], "prompt> ");
    }

    #[test]
    fn buffer_skips_charset_selection_and_erase_sequences() {
        let mut buffer = TerminalBuffer::new(100, 30);
        buffer.feed(b"\x1b(Bok\x1b[Kx");
        assert_eq!(buffer.lines()[0], "okx");
    }

    #[test]
    fn pty_starts_and_captures_output() {
        let mut terminal =
            PtyTerminal::start(env!("CARGO_MANIFEST_DIR"), || {}).expect("pty");
        sleep(Duration::from_millis(400));
        terminal.pump();
        assert!(
            terminal.lines().iter().any(|line| !line.trim().is_empty()),
            "shell started but did not emit a visible prompt"
        );
        terminal.kill();
    }
}
