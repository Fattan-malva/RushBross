//! Terminal RushAgent: shell sungguhan (ConPTY di Windows, PTY di platform lain) dengan
//! emulator VT dari `alacritty_terminal` (pure Rust, tanpa engine browser).
//!
//! Crate ini memegang semua logika terminal: input (tombol khusus, mode kursor aplikasi,
//! paste), output (emulasi penuh), warna, kursor, dan scrollback. UI hanya menggambar
//! `Snapshot` yang sudah dihitung per sel dan menerima input yang sudah diterjemahkan.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Baris riwayat yang disimpan emulator (scrollback).
const HISTORY_LINES: usize = 10_000;

/// Metrik sel untuk font monospace (Consolas: advance 1126/2048 em).
pub const FONT_SIZE: f32 = 13.0;
pub const CELL_WIDTH: f32 = FONT_SIZE * 0.5498;
pub const LINE_HEIGHT: f32 = 17.0;
pub const DEFAULT_FG: [u8; 3] = [0xd4, 0xd4, 0xd4];
pub const DEFAULT_BG: [u8; 3] = [0x0c, 0x0c, 0x0c];

/// Palet ANSI 16 warna (gaya Campbell, sama seperti terminal Windows modern).
const ANSI: [[u8; 3]; 16] = [
    [0x0c, 0x0c, 0x0c],
    [0xc5, 0x0f, 0x1f],
    [0x13, 0xa1, 0x0e],
    [0xc1, 0x9c, 0x00],
    [0x00, 0x37, 0xda],
    [0x88, 0x17, 0x98],
    [0x3a, 0x96, 0xdd],
    [0xcc, 0xcc, 0xcc],
    [0x76, 0x76, 0x76],
    [0xe7, 0x48, 0x56],
    [0x16, 0xc6, 0x0c],
    [0xf9, 0xf1, 0xa5],
    [0x3b, 0x78, 0xff],
    [0xb4, 0x00, 0x9e],
    [0x61, 0xd6, 0xd6],
    [0xf2, 0xf2, 0xf2],
];

/// Ukuran layar emulator. (`TermSize` bawaan hanya tersedia untuk test di crate upstream.)
#[derive(Clone, Copy)]
struct TermSize {
    columns: usize,
    screen_lines: usize,
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

/// Menerima jawaban emulator yang harus dikirim balik ke shell (posisi kursor, atribut).
#[derive(Clone)]
struct PtyReplies(Sender<String>);

impl EventListener for PtyReplies {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            let _ = self.0.send(text);
        }
    }
}

/// Tombol logis yang diterjemahkan UI ke urutan byte terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
    Char(char),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Potongan teks dengan satu gaya, dimulai di kolom `col`.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub col: usize,
    pub text: String,
    pub fg: [u8; 3],
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
}

/// Keadaan layar yang siap digambar. Baris 0 adalah baris paling atas layar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Vec<Run>>,
    /// (baris, kolom) kursor bila terlihat di layar sekarang.
    pub cursor: Option<(usize, usize)>,
    /// True bila layar sedang digulir ke riwayat, bukan posisi terbaru.
    pub scrolled: bool,
}

pub struct PtyTerminal {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    replies: Receiver<String>,
    term: Term<PtyReplies>,
    parser: Processor,
    snapshot: Snapshot,
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
        let (cols, rows) = (100u16, 30u16);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
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
            let mut buffer = [0u8; 16 * 1024];
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
        let size = TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        };
        let mut terminal = Self {
            master: pair.master,
            child,
            writer,
            output,
            replies,
            term: Term::new(config, &size, PtyReplies(reply_tx)),
            parser: Processor::new(),
            snapshot: Snapshot::default(),
            cols,
            rows,
        };
        terminal.refresh();
        Ok(terminal)
    }

    /// Ketikan dari pengguna. Layar otomatis kembali ke posisi terbaru.
    pub fn send(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.term.grid().display_offset() != 0 {
            self.term.scroll_display(Scroll::Bottom);
            self.refresh();
        }
        self.write_raw(text.as_bytes());
    }

    /// Terjemahkan tombol logis menjadi urutan byte, mengikuti mode kursor aplikasi.
    pub fn key(&mut self, key: Key, mods: Mods) {
        let app_cursor = self.term.mode().contains(TermMode::APP_CURSOR);
        let bytes = encode_key(key, mods, app_cursor);
        if !bytes.is_empty() {
            if self.term.grid().display_offset() != 0 {
                self.term.scroll_display(Scroll::Bottom);
                self.refresh();
            }
            self.write_raw(&bytes);
        }
    }

    /// Paste teks. Memakai bracketed paste bila shell memintanya.
    pub fn paste(&mut self, text: &str) {
        let bracketed = self.term.mode().contains(TermMode::BRACKETED_PASTE);
        let bytes = paste_bytes(text, bracketed);
        self.write_raw(&bytes);
    }

    /// Gulir riwayat. `lines` positif = ke atas (riwayat), negatif = ke bawah.
    pub fn scroll(&mut self, lines: i32) {
        if lines != 0 {
            self.term.scroll_display(Scroll::Delta(lines));
            self.refresh();
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 || (cols == self.cols && rows == self.rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.resize(TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        });
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.refresh();
    }

    /// Terapkan semua output yang sudah datang ke emulator dan kirim balasan emulator.
    pub fn pump(&mut self) {
        let mut changed = false;
        while let Ok(chunk) = self.output.try_recv() {
            self.parser.advance(&mut self.term, &chunk);
            changed = true;
            self.flush_replies();
        }
        if changed {
            self.refresh();
            if std::env::var_os("RUSHBROSS_TERM_DEBUG").is_some() {
                self.log_snapshot();
            }
        }
    }

    /// Log diagnostik (aktif bila env `RUSHBROSS_TERM_DEBUG` diset): posisi kursor dan
    /// baris terakhir yang tidak kosong, untuk memeriksa kursor sejajar dengan prompt.
    fn log_snapshot(&self) {
        let last_text = self
            .snapshot
            .lines
            .iter()
            .rposition(|runs| runs.iter().any(|run| !run.text.trim().is_empty()));
        eprintln!(
            "[rb-terminal] cursor={:?} last_text_row={:?} rows={} cols={} scrolled={}",
            self.snapshot.cursor,
            last_text,
            self.snapshot.rows,
            self.snapshot.cols,
            self.snapshot.scrolled
        );
    }

    /// Keadaan layar terakhir yang sudah dihitung.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
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

    fn write_raw(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
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

    /// Hitung ulang `Snapshot` dari grid emulator, termasuk posisi gulir.
    fn refresh(&mut self) {
        let grid = self.term.grid();
        let offset = grid.display_offset() as i32;
        let columns = grid.columns();
        let rows = grid.screen_lines();
        let mut lines = Vec::with_capacity(rows);
        for row in 0..rows {
            let line = &grid[Line(row as i32 - offset)];
            lines.push(row_runs(line, columns));
        }
        let cursor_visible = self.term.mode().contains(TermMode::SHOW_CURSOR) && offset == 0;
        let cursor = if cursor_visible {
            let point = grid.cursor.point;
            usize::try_from(point.line.0)
                .ok()
                .map(|row| (row, point.column.0))
        } else {
            None
        };
        self.snapshot = Snapshot {
            cols: columns,
            rows,
            lines,
            cursor,
            scrolled: offset != 0,
        };
    }
}

/// Ubah satu baris grid menjadi potongan teks ber-gaya. Sel lebar-ganda dilewati
/// (spacer-nya), jadi kolom tetap sejajar dengan layar.
fn row_runs<R>(row: &R, columns: usize) -> Vec<Run>
where
    R: std::ops::Index<Column, Output = alacritty_terminal::term::cell::Cell>,
{
    let mut runs: Vec<Run> = Vec::new();
    let mut current: Option<Run> = None;
    for column in 0..columns {
        let cell = &row[Column(column)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let (fg, bg) = cell_colors(cell.fg, cell.bg, cell.flags.contains(Flags::INVERSE));
        let bold = cell.flags.contains(Flags::BOLD);
        let mut text = String::new();
        text.push(cell.c);
        if let Some(extra) = cell.zerowidth() {
            text.extend(extra.iter().copied());
        }
        match current.as_mut() {
            Some(run) if run.fg == fg && run.bg == bg && run.bold == bold => {
                run.text.push_str(&text);
            }
            _ => {
                if let Some(done) = current.take() {
                    runs.push(done);
                }
                current = Some(Run {
                    col: column,
                    text,
                    fg,
                    bg,
                    bold,
                });
            }
        }
    }
    if let Some(done) = current {
        runs.push(done);
    }
    // Buang spasi kosong di ujung baris yang tidak punya latar belakang.
    while let Some(last) = runs.last() {
        if last.bg.is_none() && last.text.trim().is_empty() {
            runs.pop();
        } else {
            break;
        }
    }
    runs
}

fn cell_colors(fg: Color, bg: Color, inverse: bool) -> ([u8; 3], Option<[u8; 3]>) {
    let fg_rgb = resolve_color(fg);
    let bg_rgb = resolve_color(bg);
    let (fg_rgb, bg_rgb) = if inverse {
        (bg_rgb, Some(fg_rgb))
    } else {
        (
            fg_rgb,
            if bg_rgb == DEFAULT_BG {
                None
            } else {
                Some(bg_rgb)
            },
        )
    };
    (fg_rgb, bg_rgb)
}

/// Warna terminal → RGB. Foreground/Background bawaan memakai `DEFAULT_FG`/`DEFAULT_BG`.
pub fn resolve_color(color: Color) -> [u8; 3] {
    match color {
        Color::Spec(Rgb { r, g, b }) => [r, g, b],
        Color::Indexed(index) => indexed_color(index),
        Color::Named(named) => match named {
            NamedColor::Foreground
            | NamedColor::BrightForeground
            | NamedColor::DimForeground
            | NamedColor::Cursor => DEFAULT_FG,
            NamedColor::Background => DEFAULT_BG,
            NamedColor::Black | NamedColor::DimBlack => ANSI[0],
            NamedColor::Red | NamedColor::DimRed => ANSI[1],
            NamedColor::Green | NamedColor::DimGreen => ANSI[2],
            NamedColor::Yellow | NamedColor::DimYellow => ANSI[3],
            NamedColor::Blue | NamedColor::DimBlue => ANSI[4],
            NamedColor::Magenta | NamedColor::DimMagenta => ANSI[5],
            NamedColor::Cyan | NamedColor::DimCyan => ANSI[6],
            NamedColor::White | NamedColor::DimWhite => ANSI[7],
            NamedColor::BrightBlack => ANSI[8],
            NamedColor::BrightRed => ANSI[9],
            NamedColor::BrightGreen => ANSI[10],
            NamedColor::BrightYellow => ANSI[11],
            NamedColor::BrightBlue => ANSI[12],
            NamedColor::BrightMagenta => ANSI[13],
            NamedColor::BrightCyan => ANSI[14],
            NamedColor::BrightWhite => ANSI[15],
        },
    }
}

/// Palet 256 warna xterm: 0-15 ANSI, 16-231 kubus 6x6x6, 232-255 abu-abu.
pub fn indexed_color(index: u8) -> [u8; 3] {
    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let n = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            [level(n / 36), level((n % 36) / 6), level(n % 6)]
        }
        232..=255 => {
            let v = 8 + 10 * (index - 232);
            [v, v, v]
        }
    }
}

/// Urutan byte untuk satu tombol. `app_cursor` = mode DECCKM aktif (`ESC O` untuk panah).
pub fn encode_key(key: Key, mods: Mods, app_cursor: bool) -> Vec<u8> {
    let modifier = 1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl);
    let csi_letter = |letter: char| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{letter}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        }
    };
    let csi_tilde = |code: u8| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[{code};{modifier}~").into_bytes()
        } else {
            format!("\x1b[{code}~").into_bytes()
        }
    };
    match key {
        Key::Enter => vec![b'\r'],
        Key::Backspace => vec![if mods.ctrl { 0x08 } else { 0x7f }],
        Key::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => csi_letter('A'),
        Key::Down => csi_letter('B'),
        Key::Right => csi_letter('C'),
        Key::Left => csi_letter('D'),
        Key::Home => csi_letter('H'),
        Key::End => csi_letter('F'),
        Key::Insert => csi_tilde(2),
        Key::Delete => csi_tilde(3),
        Key::PageUp => csi_tilde(5),
        Key::PageDown => csi_tilde(6),
        Key::F(number) => match number {
            1..=4 => {
                let letter = ['P', 'Q', 'R', 'S'][number as usize - 1];
                format!("\x1bO{letter}").into_bytes()
            }
            5 => csi_tilde(15),
            6 => csi_tilde(17),
            7 => csi_tilde(18),
            8 => csi_tilde(19),
            9 => csi_tilde(20),
            10 => csi_tilde(21),
            11 => csi_tilde(23),
            12 => csi_tilde(24),
            _ => Vec::new(),
        },
        Key::Char(ch) => {
            let mut bytes = Vec::new();
            if mods.ctrl {
                let control = match ch {
                    'a'..='z' => Some(ch as u8 - b'a' + 1),
                    'A'..='Z' => Some(ch as u8 - b'A' + 1),
                    ' ' | '@' => Some(0),
                    '[' => Some(0x1b),
                    '\\' => Some(0x1c),
                    ']' => Some(0x1d),
                    '^' => Some(0x1e),
                    '_' => Some(0x1f),
                    '?' => Some(0x7f),
                    _ => None,
                };
                match control {
                    Some(byte) => bytes.push(byte),
                    None => {
                        let mut buffer = [0u8; 4];
                        bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                    }
                }
            } else {
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
            }
            if mods.alt {
                bytes.insert(0, 0x1b);
            }
            bytes
        }
    }
}

/// Teks paste siap kirim: baris baru dinormalkan ke CR, dan dibungkus bila bracketed paste.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{normalized}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}

/// Shell default per platform. Di Windows memakai cmd.exe (`/D /Q /K`, sama seperti
/// Open-CLI), di platform lain memakai $SHELL.
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

    #[test]
    fn arrow_keys_follow_cursor_mode_and_modifiers() {
        assert_eq!(encode_key(Key::Up, Mods::default(), false), b"\x1b[A");
        assert_eq!(encode_key(Key::Up, Mods::default(), true), b"\x1bOA");
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert_eq!(encode_key(Key::Left, ctrl, false), b"\x1b[1;5D");
    }

    #[test]
    fn control_and_alt_characters() {
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert_eq!(encode_key(Key::Char('c'), ctrl, false), vec![0x03]);
        assert_eq!(encode_key(Key::Char('D'), ctrl, false), vec![0x04]);
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        assert_eq!(encode_key(Key::Char('x'), alt, false), b"\x1bx");
        assert_eq!(
            encode_key(Key::Char('é'), Mods::default(), false),
            "é".as_bytes()
        );
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(encode_key(Key::Enter, Mods::default(), false), b"\r");
        assert_eq!(
            encode_key(Key::Backspace, Mods::default(), false),
            vec![0x7f]
        );
        assert_eq!(
            encode_key(
                Key::Tab,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
                false
            ),
            b"\x1b[Z"
        );
        assert_eq!(encode_key(Key::Delete, Mods::default(), false), b"\x1b[3~");
        assert_eq!(encode_key(Key::F(1), Mods::default(), false), b"\x1bOP");
        assert_eq!(encode_key(Key::F(12), Mods::default(), false), b"\x1b[24~");
    }

    #[test]
    fn paste_normalizes_newlines_and_uses_bracketed_mode() {
        assert_eq!(paste_bytes("a\nb\r\nc", false), b"a\rb\rc");
        assert_eq!(paste_bytes("x", true), b"\x1b[200~x\x1b[201~");
    }

    #[test]
    fn indexed_palette_matches_xterm_cube() {
        assert_eq!(indexed_color(196), [0xff, 0x00, 0x00]);
        assert_eq!(indexed_color(232), [8, 8, 8]);
        assert_eq!(indexed_color(1), ANSI[1]);
    }

    fn pump_until(
        terminal: &mut PtyTerminal,
        timeout: Duration,
        done: impl Fn(&Snapshot) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            terminal.pump();
            if done(terminal.snapshot()) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn text_of(snapshot: &Snapshot) -> Vec<String> {
        snapshot
            .lines
            .iter()
            .map(|runs| {
                runs.iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
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
        let ok = pump_until(&mut terminal, Duration::from_secs(5), |snapshot| {
            let text = text_of(snapshot);
            text.iter().any(|line| line.contains("rb-ready"))
                && text.iter().any(|line| line.contains(expected.as_str()))
        });
        assert!(
            ok,
            "output tidak tampil: {:?}",
            text_of(terminal.snapshot())
        );
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn colors_and_cursor_are_part_of_the_snapshot() {
        let script = "printf '\\033[31mred\\033[0m plain'; sleep 2";
        let mut terminal =
            PtyTerminal::spawn("/bin/sh", &["-c", script], &std::env::temp_dir(), || {})
                .expect("spawn");
        let ok = pump_until(&mut terminal, Duration::from_secs(5), |snapshot| {
            snapshot
                .lines
                .iter()
                .flatten()
                .any(|run| run.text.contains("red") && run.fg == ANSI[1])
        });
        assert!(ok, "warna merah tidak terbawa: {:?}", terminal.snapshot());
        let snapshot = terminal.snapshot();
        assert!(snapshot.cursor.is_some(), "kursor harus terlihat");
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
        let ok = pump_until(&mut terminal, Duration::from_secs(6), |snapshot| {
            text_of(snapshot)
                .iter()
                .any(|line| line.contains("got:") && line.contains("1;1"))
        });
        assert!(
            ok,
            "DSR tidak dijawab atau output tertelan: {:?}",
            text_of(terminal.snapshot())
        );
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn typed_keys_reach_the_shell() {
        let mut terminal = PtyTerminal::spawn(
            "/bin/bash",
            &["--norc", "--noprofile", "-i"],
            &std::env::temp_dir(),
            || {},
        )
        .expect("spawn");
        thread::sleep(Duration::from_millis(300));
        terminal.pump();
        terminal.send("echo rb-typed");
        terminal.key(Key::Enter, Mods::default());
        let ok = pump_until(&mut terminal, Duration::from_secs(5), |snapshot| {
            text_of(snapshot)
                .iter()
                .any(|line| line.trim() == "rb-typed")
        });
        assert!(
            ok,
            "ketikan tidak sampai ke shell: {:?}",
            text_of(terminal.snapshot())
        );
        terminal.kill();
    }
}

#[cfg(all(test, unix))]
mod cursor_alignment_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn cursor_sits_on_prompt_after_resize() {
        let mut terminal = PtyTerminal::spawn(
            "/bin/sh",
            &["-c", "sleep 0.3; printf 'C:\\\\FATTAN>'; sleep 3"],
            &std::env::temp_dir(),
            || {},
        )
        .expect("spawn");
        terminal.resize(60, 40);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            terminal.pump();
            let found = terminal
                .snapshot()
                .lines
                .iter()
                .position(|runs| runs.iter().any(|run| run.text.contains("FATTAN")));
            if let Some(row) = found {
                let snap = terminal.snapshot();
                let (cur_row, cur_col) = snap.cursor.expect("cursor");
                eprintln!(
                    "prompt row={row} cursor=({cur_row},{cur_col}) rows={} cols={}",
                    snap.rows, snap.cols
                );
                assert_eq!(cur_row, row, "kursor harus di baris prompt");
                break;
            }
            assert!(Instant::now() < deadline, "prompt tidak muncul");
            thread::sleep(Duration::from_millis(20));
        }
        terminal.kill();
    }
}
