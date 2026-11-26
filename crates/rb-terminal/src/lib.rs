//! Terminal RushAgent: proses shell sungguhan (ConPTY di Windows, PTY di platform lain)
//! dengan emulator VT dari `alacritty_terminal`. Output shell diparse oleh emulator
//! lengkap (OSC, CSI, DSR, charset, wide char), jadi tampilannya sama seperti terminal
//! sungguhan, sama seperti pendekatan Open-CLI yang memakai emulator penuh.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Baris riwayat yang disimpan emulator (scrollback).
const HISTORY_LINES: usize = 5_000;
/// Jumlah baris terakhir yang diserahkan ke renderer per pembaruan.
const MAX_RENDER_LINES: usize = 2_000;
const DEFAULT_COLS: u16 = 100;
const DEFAULT_ROWS: u16 = 30;

/// Ukuran layar emulator. (`TermSize` bawaan hanya tersedia untuk test di crate upstream.)
#[derive(Clone, Copy)]
struct TermSize {
    columns: usize,
    screen_lines: usize,
}

impl TermSize {
    fn new(columns: usize, screen_lines: usize) -> Self {
        Self {
            columns,
            screen_lines,
        }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Menerima jawaban emulator yang harus dikirim balik ke shell, misalnya laporan posisi
/// kursor untuk `ESC [ 6 n` dan respons atribut perangkat.
#[derive(Clone)]
struct PtyReplies(Sender<String>);

impl EventListener for PtyReplies {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            let _ = self.0.send(text);
        }
    }
}

pub struct PtyTerminal {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    replies: Receiver<String>,
    term: Term<PtyReplies>,
    parser: Processor,
    lines: Vec<String>,
    cols: u16,
    rows: u16,
}

impl PtyTerminal {
    /// Buka shell default di `folder`. `on_output` dipanggil dari thread pembaca setiap
    /// ada output baru, biasanya untuk meminta redraw.
    pub fn start(folder: &str, on_output: impl Fn() + Send + 'static) -> Result<Self, String> {
        let cwd = Path::new(folder);
        if !cwd.is_dir() {
            return Err(format!("Folder tidak ditemukan: {folder}"));
        }
        let (program, args) = default_shell();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Self::spawn(&program, &args, cwd, on_output)
    }

    pub fn spawn(
        program: &str,
        args: &[&str],
        cwd: &Path,
        on_output: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_ROWS,
                cols: DEFAULT_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())?;

        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        if cfg!(windows) {
            command.env("PROMPT", "$P$G");
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("gagal menjalankan {program}: {error}"))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;

        let (output_tx, output) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        // Semua byte diteruskan utuh (termasuk BEL dan ESC): emulator
                        // yang memutuskan arti setiap urutan.
                        if output_tx.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                        on_output();
                    }
                }
            }
        });

        let (reply_tx, replies) = mpsc::channel();
        let config = Config {
            scrolling_history: HISTORY_LINES,
            ..Config::default()
        };
        let size = TermSize::new(DEFAULT_COLS as usize, DEFAULT_ROWS as usize);
        let mut terminal = Self {
            master: pair.master,
            child,
            writer,
            output,
            replies,
            term: Term::new(config, &size, PtyReplies(reply_tx)),
            parser: Processor::new(),
            lines: Vec::new(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        };
        terminal.refresh_lines();
        Ok(terminal)
    }

    /// Kirim teks mentah ke shell (ketikan, tombol khusus, atau paste).
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
        self.term
            .resize(TermSize::new(cols as usize, rows as usize));
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.refresh_lines();
    }

    /// Terapkan semua output yang sudah datang ke emulator dan kirim balasan DSR.
    pub fn pump(&mut self) {
        let mut changed = false;
        while let Ok(chunk) = self.output.try_recv() {
            self.parser.advance(&mut self.term, &chunk);
            changed = true;
            self.flush_replies();
        }
        if changed {
            self.refresh_lines();
        }
    }

    /// Baris teks layar beserta riwayat terakhir, siap dirender.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)) | Err(_))
    }

    pub fn kill(&mut self) {
        #[cfg(windows)]
        if let Some(pid) = self.child.process_id() {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: jangan memunculkan konsol untuk taskkill.
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .output();
        }
        let _ = self.child.kill();
    }

    fn flush_replies(&mut self) {
        let mut wrote = false;
        while let Ok(reply) = self.replies.try_recv() {
            let _ = self.writer.write_all(reply.as_bytes());
            wrote = true;
        }
        if wrote {
            let _ = self.writer.flush();
        }
    }

    /// Ubah grid emulator menjadi baris teks: riwayat sampai baris kursor, dipotong
    /// ke `MAX_RENDER_LINES` baris terakhir.
    fn refresh_lines(&mut self) {
        let grid = self.term.grid();
        let cursor_line = grid.cursor.point.line.0;
        let top = grid
            .topmost_line()
            .0
            .max(cursor_line - MAX_RENDER_LINES as i32 + 1);
        let columns = grid.columns();
        let mut lines = Vec::new();
        for line in top..=cursor_line {
            let row = &grid[Line(line)];
            let mut text = String::with_capacity(columns);
            for column in 0..columns {
                let cell = &row[Column(column)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
                if let Some(extra) = cell.zerowidth() {
                    text.extend(extra.iter().copied());
                }
            }
            lines.push(text.trim_end().to_string());
        }
        self.lines = lines;
    }
}

/// Shell default per platform. Di Windows memakai cmd.exe seperti Open-CLI
/// (`/D /Q /K`), di platform lain memakai $SHELL.
pub fn default_shell() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        let program = std::env::var_os("COMSPEC")
            .filter(|path| Path::new(path).is_file())
            .or_else(|| {
                std::env::var_os("SystemRoot")
                    .map(std::path::PathBuf::from)
                    .map(|root| root.join("System32").join("cmd.exe"))
                    .filter(|path| path.is_file())
                    .map(std::path::PathBuf::into_os_string)
            })
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cmd.exe".to_string());
        (
            program,
            vec!["/D".to_string(), "/Q".to_string(), "/K".to_string()],
        )
    }
    #[cfg(not(windows))]
    {
        let program = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string());
        (program, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn pump_until(
        terminal: &mut PtyTerminal,
        timeout: Duration,
        done: impl Fn(&[String]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            terminal.pump();
            if done(terminal.lines()) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn runs_shell_in_folder_and_shows_output() {
        let dir = std::env::temp_dir();
        let mut terminal = PtyTerminal::spawn(
            "/bin/sh",
            &["-c", "pwd; echo rb-ready; sleep 2"],
            &dir,
            || {},
        )
        .expect("spawn");
        let expected = dir.canonicalize().unwrap().display().to_string();
        let ok = pump_until(&mut terminal, Duration::from_secs(5), |lines| {
            lines.iter().any(|line| line.contains("rb-ready"))
                && lines.iter().any(|line| line.contains(expected.as_str()))
        });
        assert!(ok, "output tidak tampil: {:?}", terminal.lines());
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn osc_title_does_not_swallow_output_and_cursor_query_is_answered() {
        // OSC judul diakhiri BEL; DSR `ESC [ 6 n` harus dijawab emulator, kalau tidak
        // `read` di bash menunggu sampai timeout dan tidak mencetak "got".
        // Memakai bash karena `read -t/-d` tidak ada di dash (/bin/sh di Debian/Ubuntu).
        let script = "printf '\\033]0;RushBross\\007'; printf '\\033[6n'; IFS= read -r -t 3 -d R reply; printf 'got:%s\\n' \"$reply\"; sleep 2";
        let mut terminal =
            PtyTerminal::spawn("/bin/bash", &["-c", script], &std::env::temp_dir(), || {})
                .expect("spawn");
        let ok = pump_until(&mut terminal, Duration::from_secs(6), |lines| {
            lines
                .iter()
                .any(|line| line.contains("got:") && line.contains("1;1"))
        });
        assert!(
            ok,
            "DSR tidak dijawab atau output tertelan: {:?}",
            terminal.lines()
        );
        terminal.kill();
    }
}
