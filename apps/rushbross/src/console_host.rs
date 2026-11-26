//! Terminal RushAgent berupa konsol Windows asli (cmd.exe di jendela konsol sendiri),
//! yang jendela konsolnya ditempelkan sebagai child window ke jendela RushBross.
//!
//! Tampilan, kursor, warna, scrollback, dan perilaku keyboard sepenuhnya dari konsol
//! Windows. RushBross hanya mengatur posisi/ukuran child window dan fokus, serta
//! mengirim teks lewat input konsol (dipakai untuk "pilih elemen").

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HWND};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleWindow, WriteConsoleInputW, INPUT_RECORD, INPUT_RECORD_0,
    KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, IsChild, SetParent, SetWindowLongW,
    SetWindowPos, ShowWindow, GUITHREADINFO, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
};

/// Shell bawaan Windows: COMSPEC (biasanya cmd.exe), sama seperti terminal bawaan.
pub fn default_shell() -> String {
    std::env::var("COMSPEC")
        .ok()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| "cmd.exe".to_string())
}

/// Membuat konsol baru untuk proses ini (bukan mewarisi konsol RushBross, yang tidak punya).
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// Gaya jendela top-level yang dibuang saat konsol ditempel sebagai child.
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;

const VK_RETURN: u16 = 0x0D;

pub struct ConsoleHost {
    child: Child,
    pid: u32,
    hwnd: HWND,
    thread_id: u32,
    parent: HWND,
    rect: (i32, i32, i32, i32),
}

impl ConsoleHost {
    /// Jalankan `program` di konsol baru dan tempelkan jendela konsolnya ke `parent`
    /// pada posisi `rect` (x, y, lebar, tinggi dalam piksel fisik, relatif ke client area).
    pub fn spawn(
        parent: HWND,
        program: &str,
        args: &[&str],
        cwd: &Path,
        rect: (i32, i32, i32, i32),
    ) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|error| format!("gagal menjalankan {program}: {error}"))?;
        let pid = child.id();
        let hwnd = match find_console_window(pid) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                let _ = child.kill();
                return Err(error);
            }
        };
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
        let mut host = Self {
            child,
            pid,
            hwnd,
            thread_id,
            parent,
            rect,
        };
        embed(hwnd, parent, rect);
        // Pastikan ukuran dan posisi sesuai pane, lalu tampilkan.
        host.set_rect(rect);
        Ok(host)
    }

    /// Ubah posisi/ukuran child window bila berubah (piksel fisik).
    pub fn set_rect(&mut self, rect: (i32, i32, i32, i32)) {
        if rect == self.rect {
            return;
        }
        self.rect = rect;
        let (x, y, width, height) = rect;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                x,
                y,
                width.max(1),
                height.max(1),
                SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }
    }

    /// Beri fokus keyboard ke konsol, sehingga ketikan langsung masuk ke shell.
    pub fn focus(&self) {
        focus_window(self.hwnd);
    }

    /// Kembalikan fokus keyboard ke jendela RushBross (mis. saat pengguna klik toolbar).
    pub fn release_focus(&self) {
        focus_window(self.parent);
    }

    /// True bila fokus keyboard sedang berada di konsol ini. Dibaca dari antrean input
    /// thread konsol, karena GetFocus dari thread kita tidak melihat fokus di thread lain.
    pub fn has_focus(&self) -> bool {
        unsafe {
            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(self.thread_id, &mut info).is_err() {
                return false;
            }
            let focused = info.hwndFocus;
            focused == self.hwnd || IsChild(self.hwnd, focused).as_bool()
        }
    }

    /// Kirim teks ke input konsol seolah diketik. Baris baru dikirim sebagai Enter.
    pub fn send_text(&self, text: &str) -> Result<(), String> {
        let records: Vec<INPUT_RECORD> = text
            .encode_utf16()
            .flat_map(|unit| {
                let virtual_key = if unit == u16::from(b'\r') || unit == u16::from(b'\n') {
                    VK_RETURN
                } else {
                    0
                };
                let unit = if unit == u16::from(b'\n') {
                    u16::from(b'\r')
                } else {
                    unit
                };
                [true, false].map(|down| key_record(down, virtual_key, unit))
            })
            .collect();
        if records.is_empty() {
            return Ok(());
        }
        with_console(self.pid, || write_input(&records))
    }

    /// True bila proses shell sudah berhenti.
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)) | Err(_))
    }

    /// Hentikan shell beserta proses turunannya.
    pub fn kill(&mut self) {
        let _ = Command::new("taskkill")
            .args(["/PID", &self.pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ConsoleHost {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Cari jendela konsol proses `pid`. Konsol dibuat sedikit setelah proses dimulai, jadi
/// kita coba beberapa kali. Memakai AttachConsole sebentar, lalu FreeConsole, agar
/// RushBross sendiri tetap tanpa konsol.
fn find_console_window(pid: u32) -> Result<HWND, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let found = with_console(pid, || Ok(unsafe { GetConsoleWindow() }));
        if let Ok(hwnd) = found {
            if !hwnd.0.is_null() {
                return Ok(hwnd);
            }
        }
        if Instant::now() > deadline {
            return Err("jendela konsol tidak ditemukan".to_string());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Lampirkan ke konsol `pid`, jalankan `action`, lalu lepas. Hanya dipakai di thread UI.
fn with_console<T>(pid: u32, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    unsafe {
        FreeConsole().ok();
        AttachConsole(pid).map_err(|error| format!("AttachConsole gagal: {error}"))?;
    }
    let result = action();
    unsafe {
        let _ = FreeConsole();
    }
    result
}

fn write_input(records: &[INPUT_RECORD]) -> Result<(), String> {
    let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
    unsafe {
        let handle = CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
        .map_err(|error| format!("CONIN$ tidak bisa dibuka: {error}"))?;
        let mut written = 0u32;
        let result = WriteConsoleInputW(handle, records, &mut written)
            .map_err(|error| format!("WriteConsoleInputW gagal: {error}"));
        let _ = CloseHandle(handle);
        result
    }
}

fn key_record(key_down: bool, virtual_key: u16, unit: u16) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: BOOL::from(key_down),
                wRepeatCount: 1,
                wVirtualKeyCode: virtual_key,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                dwControlKeyState: 0,
            },
        },
    }
}

/// Ubah jendela konsol menjadi child dari `parent`: buang caption/border, set posisi.
fn embed(hwnd: HWND, parent: HWND, rect: (i32, i32, i32, i32)) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let removed =
            WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_POPUP;
        let style = (style & !removed) | WS_CHILD | WS_VISIBLE;
        SetWindowLongW(hwnd, GWL_STYLE, style as i32);
        let _ = SetParent(hwnd, Some(parent));
        let (x, y, width, height) = rect;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            x,
            y,
            width.max(1),
            height.max(1),
            SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
    }
}

/// Pindahkan fokus input ke `target`. SetFocus hanya berhasil lintas thread bila input
/// thread-nya dihubungkan lebih dulu (AttachThreadInput).
fn focus_window(target: HWND) {
    unsafe {
        let mut pid = 0u32;
        let target_thread = GetWindowThreadProcessId(target, Some(&mut pid));
        let current_thread = GetCurrentThreadId();
        if target_thread != current_thread {
            let _ = AttachThreadInput(current_thread, target_thread, true);
            let _ = SetFocus(Some(target));
            let _ = AttachThreadInput(current_thread, target_thread, false);
        } else {
            let _ = SetFocus(Some(target));
        }
    }
}
