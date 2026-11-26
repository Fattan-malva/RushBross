#![windows_subsystem = "windows"]

mod console_host;
mod webview;

mod app_icon;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use rb_agent::{AgentRequest, AgentServer};
use console_host::{default_shell, ConsoleHost};
use rb_tabs::TabId;
use rb_ui::{Frame, Rect, RectShape};
use rb_ui_renderer::{RenderError, Renderer};
use rb_ui_widgets::{BrowserShell, ChromeAction, ChromeElement, CursorRole};
use serde_json::{json, Value};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{
    CursorIcon, Fullscreen, Icon, ResizeDirection, Window, WindowId, WindowLevel,
};

use webview::{UserEvent, WebViewHost, WebViewRuntime};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};

const VK_TAB: u32 = 0x09;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_A: u32 = 0x41;
const VK_D: u32 = 0x44;
const VK_L: u32 = 0x4c;
const VK_Q: u32 = 0x51;
const VK_R: u32 = 0x52;
const VK_T: u32 = 0x54;
const VK_W: u32 = 0x57;
const RESIZE_EDGE: f32 = 6.0;

struct OverlayState {
    window: Arc<Window>,
    renderer: Renderer,
    bounds: Rect,
    size: (u32, u32),
    visible: bool,
    cursor: rb_ui::Vec2,
    cursor_icon: CursorIcon,
}

fn json_rpc(id: Option<Value>, result: Result<Value, String>) -> String {
    match result {
        Ok(value) => rb_agent::rpc_result(id, value),
        Err(error) => rb_agent::rpc_error(id, rb_agent::ERR_INTERNAL, &error),
    }
}

fn tool_content(result: Result<Value, String>) -> Value {
    rb_agent::tool_result(result)
}

/// Nama tool yang diiklankan lewat `tools/list`. Pemanggilan nama lain dibalas error protokol.
fn is_known_tool(name: &str) -> bool {
    tool_definitions()
        .as_array()
        .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == name))
}

/// Token acak per sesi. Dibuat dari RandomState (seed acak OS), bukan waktu/PID saja.
fn new_session_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|_| {
            format!(
                "{:016x}",
                std::collections::hash_map::RandomState::new()
                    .build_hasher()
                    .finish()
            )
        })
        .collect()
}

fn tool_channel(reply: Sender<String>, id: Option<Value>) -> mpsc::Sender<Result<Value, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(result) = receiver.recv() {
            let _ = reply.send(json_rpc(id, Ok(tool_content(result))));
        }
    });
    sender
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "browser_state",
            "description": "Ambil URL, judul, dan viewport halaman aktif di RushBross.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_navigate",
            "description": "Buka URL di tab aktif RushBross.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"]
            }
        },
        {
            "name": "browser_snapshot",
            "description": "Ambil ringkasan DOM halaman aktif.",
            "inputSchema": {
                "type": "object",
                "properties": { "selector": { "type": "string" } }
            }
        },
        {
            "name": "browser_click",
            "description": "Klik elemen berdasarkan selector CSS atau koordinat viewport.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                }
            }
        },
        {
            "name": "browser_type",
            "description": "Ketik teks ke elemen yang dipilih, opsional tekan Enter setelahnya.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "text": { "type": "string" },
                    "submit": { "type": "boolean" }
                },
                "required": ["text"]
            }
        },
        {
            "name": "browser_press",
            "description": "Tekan satu tombol keyboard, misalnya Enter, Tab, atau Escape.",
            "inputSchema": {
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }
        },
        {
            "name": "browser_scroll",
            "description": "Scroll halaman aktif.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "required": ["y"]
            }
        },
        {
            "name": "browser_screenshot",
            "description": "Ambil screenshot halaman aktif sebagai data URL PNG.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_select",
            "description": "Aktifkan atau nonaktifkan mode pilih elemen.",
            "inputSchema": {
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } }
            }
        },
        {
            "name": "browser_get_selection",
            "description": "Ambil elemen terakhir yang dipilih lewat mode Select.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_tabs",
            "description": "Lihat daftar tab atau buat tab baru.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "new"] }
                }
            }
        }
    ])
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn wheel_lines(delta: winit::event::MouseScrollDelta) -> f32 {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
        winit::event::MouseScrollDelta::PixelDelta(position) => position.y as f32 / 16.0,
    }
}

fn upsert_json_config(
    path: &std::path::Path,
    keys: &[&str],
    server_key: &str,
    server: Value,
) -> Result<(), String> {
    let mut root = if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        serde_json::from_str(&text)
            .map_err(|_| format!("{} bukan JSON murni", path.display()))?
    } else {
        json!({})
    };
    let mut cursor = &mut root;
    for key in keys {
        let object = cursor
            .as_object_mut()
            .ok_or_else(|| format!("{} tidak berupa objek", path.display()))?;
        cursor = object
            .entry((*key).to_string())
            .or_insert_with(|| json!({}));
    }
    cursor
        .as_object_mut()
        .ok_or_else(|| format!("{} tidak berupa objek", path.display()))?
        .insert(server_key.to_string(), server);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())
}

fn remove_json_config(
    path: &std::path::Path,
    keys: &[&str],
    server_key: &str,
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|_| format!("{} bukan JSON murni", path.display()))?;
    let mut cursor = &mut root;
    for key in keys {
        let Some(next) = cursor.as_object_mut().and_then(|object| object.get_mut(*key)) else {
            return Ok(false);
        };
        cursor = next;
    }
    let removed = cursor
        .as_object_mut()
        .is_some_and(|object| object.remove(server_key).is_some());
    if removed {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(removed)
}

/// Lokasi binary bridge stdio. Config tidak memuat token: bridge membaca sesi aktif
/// dari `rushbross-agent.json` di folder temp, jadi config tetap valid setiap RushBross dibuka.
fn bridge_command() -> Result<String, String> {
    Ok(std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name("rushbross-mcp.exe")
        .to_string_lossy()
        .into_owned())
}

fn user_home() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Config JSON yang dikenali CLI agent. `keys` adalah jalur objek induknya.
fn json_targets(cwd: &std::path::Path, home: Option<&std::path::Path>) -> Vec<(std::path::PathBuf, Vec<&'static str>, bool)> {
    // Argumen ketiga: true bila formatnya `"command": [...]` (opencode/kilo), false bila `command` + `args`.
    let mut targets = vec![
        (cwd.join("opencode.json"), vec!["mcp"], true),
        (cwd.join(".kilo").join("kilo.json"), vec!["mcp"], true),
        (cwd.join(".mcp.json"), vec!["mcpServers"], false),
        (cwd.join(".agents").join("mcp_config.json"), vec!["mcpServers"], false),
    ];
    if let Some(home) = home {
        targets.push((home.join(".claude.json"), vec!["mcpServers"], false));
        targets.push((home.join(".gemini").join("settings.json"), vec!["mcpServers"], false));
        targets.push((home.join(".cursor").join("mcp.json"), vec!["mcpServers"], false));
    }
    targets
}

fn codex_config(home: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    home.map(|home| home.join(".codex").join("config.toml"))
}

fn write_agent_configs() -> Result<Vec<String>, String> {
    let command = bridge_command()?;
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let home = user_home();

    let local_server = json!({
        "type": "local",
        "command": [command.clone()],
        "enabled": true,
        "timeout": 30000
    });
    let stdio_server = json!({
        "type": "stdio",
        "command": command.clone(),
        "args": []
    });

    let mut written = Vec::new();
    let mut failures = Vec::new();
    for (path, keys, is_local) in json_targets(&cwd, home.as_deref()) {
        let server = if is_local { local_server.clone() } else { stdio_server.clone() };
        match upsert_json_config(&path, &keys, "rushbross", server) {
            Ok(()) => written.push(path.display().to_string()),
            Err(error) => failures.push(error),
        }
    }
    if let Some(path) = codex_config(home.as_deref()) {
        match upsert_toml_server(&path, &command) {
            Ok(()) => written.push(path.display().to_string()),
            Err(error) => failures.push(error),
        }
    }
    if written.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(written)
}

fn remove_agent_configs() -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let home = user_home();
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for (path, keys, _) in json_targets(&cwd, home.as_deref()) {
        match remove_json_config(&path, &keys, "rushbross") {
            Ok(true) => removed.push(path.display().to_string()),
            Ok(false) => {}
            Err(error) => failures.push(error),
        }
    }
    if let Some(path) = codex_config(home.as_deref()) {
        match remove_toml_server(&path) {
            Ok(true) => removed.push(path.display().to_string()),
            Ok(false) => {}
            Err(error) => failures.push(error),
        }
    }
    if removed.is_empty() && !failures.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(removed)
}

fn upsert_toml_server(path: &std::path::Path, command: &str) -> Result<(), String> {
    let text = if path.exists() {
        std::fs::read_to_string(path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let updated = rb_agent::toml_upsert_server(
        &text,
        "mcp_servers.rushbross",
        &[
            ("command", rb_agent::TomlValue::Str(command)),
            ("args", rb_agent::TomlValue::StrArray(&[])),
        ],
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, updated).map_err(|error| format!("{}: {error}", path.display()))
}

fn remove_toml_server(path: &std::path::Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    match rb_agent::toml_remove_server(&text, "mcp_servers.rushbross") {
        Some(updated) => {
            std::fs::write(path, updated).map_err(|error| format!("{}: {error}", path.display()))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Satu baris JSON tanpa CR/LF. Teks yang dikirim ke terminal tidak akan
/// tereksekusi baris demi baris (cmd) atau terkirim sebagai pesan terpisah ke CLI agent.
/// Newline di HTML/teks sudah di-escape JSON, dan tidak ada tombol Enter yang ikut dikirim.
fn element_message(selection: &Value) -> String {
    let mut payload = selection.clone();
    if let Some(object) = payload.as_object_mut() {
        object.remove("type");
    }
    format!("[RushBross selected element] {}", payload)
}

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    frame: Frame,
    overlay: Option<OverlayState>,
    shell: BrowserShell,
    proxy: EventLoopProxy<UserEvent>,
    webview_env: Option<WebViewRuntime>,
    /// Satu WebView per tab: tab background tetap hidup sehingga audio/video tidak berhenti.
    webviews: HashMap<TabId, WebViewHost>,
    ready: HashSet<TabId>,
    terminal: Option<ConsoleHost>,
    /// Semua pane yang sedang tampil (satu tab biasa, atau hingga tiga pane split).
    visible_webviews: Vec<TabId>,
    /// Bounds WebView terakhir per tab, supaya SetBounds tidak dipanggil tiap frame.
    last_webview_bounds: HashMap<TabId, (i32, i32, i32, i32)>,
    _agent_server: Option<AgentServer>,
    agent_selection: Option<Value>,
    select_mode: bool,
    window_shown: bool,
    cursor: rb_ui::Vec2,
    size: (f32, f32),
    modifiers: ModifiersState,
    cursor_icon: CursorIcon,
}

impl State {
    fn new(
        window: Arc<Window>,
        renderer: Renderer,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self, String> {
        let scale = window.scale_factor() as f32;
        let physical = window.inner_size();
        let size = (
            physical.width as f32 / scale,
            physical.height as f32 / scale,
        );
        let shell = BrowserShell::new();
        let agent_proxy = proxy.clone();
        let agent_server = AgentServer::start(new_session_token(), move |request| {
            let _ = agent_proxy.send_event(UserEvent::Agent(request));
        })
        .map_err(|error| format!("start agent server: {error}"))?;
        let session = json!({
            "addr": agent_server.endpoint(),
            "token": agent_server.token
        });
        let _ = std::fs::write(
            std::env::temp_dir().join("rushbross-agent.json"),
            session.to_string(),
        );
        Ok(Self {
            window,
            renderer,
            frame: Frame::default(),
            overlay: None,
            shell,
            proxy,
            webview_env: None,
            webviews: HashMap::new(),
            ready: HashSet::new(),
            terminal: None,
            visible_webviews: Vec::new(),
            last_webview_bounds: HashMap::new(),
            _agent_server: Some(agent_server),
            agent_selection: None,
            select_mode: false,
            window_shown: false,
            cursor: rb_ui::Vec2::new(-1000.0, -1000.0),
            size,
            modifiers: ModifiersState::empty(),
            cursor_icon: CursorIcon::Default,
        })
    }

    fn parent_hwnd(&self) -> Result<windows::Win32::Foundation::HWND, String> {
        match self.window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                RawWindowHandle::Win32(handle) => Ok(windows::Win32::Foundation::HWND(
                    handle.hwnd.get() as *mut c_void,
                )),
                _ => Err("window is not a Win32 window".to_string()),
            },
            Err(error) => Err(error.to_string()),
        }
    }

    fn init_overlay(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let owner = self.parent_hwnd()?;
        let attributes = Window::default_attributes()
            .with_title("")
            .with_inner_size(LogicalSize::new(1.0, 1.0))
            .with_decorations(false)
            .with_skip_taskbar(true)
            .with_active(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_owner_window(owner.0 as isize);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("create overlay window: {error}"))?,
        );
        let mut renderer = pollster::block_on(Renderer::new(window.clone()))?;
        renderer.set_scale_factor(self.window.scale_factor() as f32);
        self.overlay = Some(OverlayState {
            window,
            renderer,
            bounds: Rect::default(),
            size: (1, 1),
            visible: false,
            cursor: rb_ui::Vec2::new(0.0, 0.0),
            cursor_icon: CursorIcon::Default,
        });
        Ok(())
    }

    fn init_webview(&mut self) -> Result<(), String> {
        self.webview_env = Some(WebViewRuntime::new()?);
        self.pump_navigation();
        Ok(())
    }

    fn open_rush_agent(&mut self) {
        // Pane kosong langsung tampil; folder dipilih lewat tombol di dalamnya.
        if self.terminal.is_none() {
            self.shell.open_terminal_placeholder();
        }
        self.shell.open_folder_picker();
        self.redraw();
    }

    fn start_rush_agent(&mut self, folder: String) {
        self.stop_terminal();
        let parent = match self.parent_hwnd() {
            Ok(parent) => parent,
            Err(error) => {
                self.shell.show_toast(format!("RushAgent gagal: {error}"));
                self.redraw();
                return;
            }
        };
        let rect = self.terminal_physical_rect();
        let program = default_shell();
        match ConsoleHost::spawn(parent, &program, &[], std::path::Path::new(&folder), rect) {
            Ok(host) => {
                self.terminal = Some(host);
                self.shell.open_terminal(folder);
                self.shell.set_agent_actions_enabled(true);
                self.shell.show_toast("RushAgent siap");
            }
            Err(error) => self.shell.show_toast(format!("RushAgent gagal: {error}")),
        }
        self.redraw();
    }

    fn terminal_exited(&mut self) -> bool {
        self.terminal.as_mut().is_some_and(|terminal| terminal.exited())
    }

    fn stop_terminal(&mut self) {
        if let Some(mut terminal) = self.terminal.take() {
            terminal.kill();
        }
        self.shell.close_terminal();
        self.shell.set_agent_actions_enabled(false);
        self.shell.set_agent_connected(false);
        self.select_mode = false;
        self.shell.set_select_mode(false);
    }

    /// Posisi body pane RushAgent dalam piksel fisik, relatif ke client area jendela.
    fn terminal_physical_rect(&self) -> (i32, i32, i32, i32) {
        let scale = self.window.scale_factor() as f32;
        match self.shell.terminal_layout(self.inset_content_rect()) {
            Some(layout) => (
                (layout.body.x * scale).round() as i32,
                (layout.body.y * scale).round() as i32,
                (layout.body.width * scale).round() as i32,
                (layout.body.height * scale).round() as i32,
            ),
            None => (0, 0, 0, 0),
        }
    }

    /// Sinkronkan konsol dengan layout dan fokus shell. Dipanggil setiap frame.
    fn pump_terminal(&mut self) {
        if self.terminal.as_mut().is_some_and(|terminal| terminal.exited()) {
            self.stop_terminal();
            return;
        }
        if self.terminal.is_some() && !self.shell.terminal_is_open() {
            self.stop_terminal();
            return;
        }
        let rect = self.terminal_physical_rect();
        let wants_focus = self.shell.terminal_focused();
        let has_focus = self.terminal.as_ref().is_some_and(|terminal| terminal.has_focus());
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.set_rect(rect);
            if has_focus && !wants_focus {
                // Pengguna klik langsung di konsol: shell ikut menandai pane terminal fokus.
                self.shell.focus_terminal();
            } else if !has_focus && wants_focus {
                terminal.focus();
            }
        }
    }

    fn handle_agent_request(&mut self, request: AgentRequest) {
        let id = request.id.clone();
        let response = match request.method.as_str() {
            "initialize" => {
                let requested = request.params.get("protocolVersion").and_then(Value::as_str);
                json_rpc(
                    id,
                    Ok(json!({
                        "protocolVersion": rb_agent::negotiate_protocol_version(requested),
                        "capabilities": {
                            "tools": { "listChanged": false },
                            "resources": { "listChanged": false },
                            "prompts": { "listChanged": false }
                        },
                        "serverInfo": { "name": "rushbross", "version": env!("CARGO_PKG_VERSION") },
                        "instructions": "Gunakan tool browser_* untuk mengendalikan RushBross. Kursor hijau menandai aksi agent. browser_get_selection mengembalikan elemen yang dipilih pengguna lewat mode Select."
                    })),
                )
            }
            "notifications/initialized" | "notifications/cancelled" => return,
            "ping" => json_rpc(id, Ok(json!({}))),
            "tools/list" => json_rpc(id, Ok(json!({ "tools": tool_definitions() }))),
            // Server ini tidak punya resource atau prompt, tetapi CLI sering memanggil daftarnya.
            "resources/list" => json_rpc(id, Ok(json!({ "resources": [] }))),
            "resources/templates/list" => json_rpc(id, Ok(json!({ "resourceTemplates": [] }))),
            "prompts/list" => json_rpc(id, Ok(json!({ "prompts": [] }))),
            "tools/call" => {
                let name = request.params.get("name").and_then(Value::as_str).unwrap_or("");
                if !is_known_tool(name) {
                    rb_agent::rpc_error(
                        id,
                        rb_agent::ERR_INVALID_PARAMS,
                        &format!("tool tidak dikenal: {name}"),
                    )
                } else {
                    match self.start_tool(&request.params, request.reply.clone(), id.clone()) {
                        // Hasil tool sudah berbentuk `content` + `isError`.
                        Some(Ok(value)) => json_rpc(id, Ok(value)),
                        // Kegagalan tool dikirim sebagai hasil tool (`isError: true`), bukan error protokol.
                        Some(Err(error)) => json_rpc(id, Ok(tool_content(Err(error)))),
                        // Balasan akan dikirim belakangan lewat tool_channel.
                        None => return,
                    }
                }
            }
            other => rb_agent::rpc_error(
                id,
                rb_agent::ERR_METHOD_NOT_FOUND,
                &format!("method tidak dikenal: {other}"),
            ),
        };
        let _ = request.reply.send(response);
    }

    fn start_tool(
        &mut self,
        params: &Value,
        reply: Sender<String>,
        id: Option<Value>,
    ) -> Option<Result<Value, String>> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let ok = |value: Value| Some(Ok(tool_content(Ok(value))));
        match name {
            "browser_tabs" => {
                let action = args.get("action").and_then(Value::as_str).unwrap_or("list");
                if action == "new" {
                    self.shell.new_tab();
                    self.redraw();
                    return ok(json!({ "ok": true }));
                }
                let tabs: Vec<Value> = self
                    .shell
                    .tab_infos()
                    .into_iter()
                    .map(|(id, title, url)| {
                        json!({ "id": id.0, "title": title, "url": url })
                    })
                    .collect();
                ok(json!({ "tabs": tabs }))
            }
            "browser_get_selection" => match &self.agent_selection {
                Some(selection) => ok(selection.clone()),
                None => ok(json!({ "selected": false })),
            },
            "browser_select" => {
                let enabled = args
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(!self.select_mode);
                self.select_mode = enabled;
                self.shell.set_select_mode(enabled);
                for host in self.webviews.values() {
                    host.set_picker(enabled);
                }
                self.shell.show_toast(if enabled {
                    "Select element aktif"
                } else {
                    "Select element dimatikan"
                });
                self.redraw();
                ok(json!({ "ok": true, "enabled": enabled }))
            }
            "browser_navigate" => {
                let Some(url) = args.get("url").and_then(Value::as_str) else {
                    return Some(Err("url wajib diisi".to_string()));
                };
                self.shell.navigate_active(url.to_string());
                self.redraw();
                ok(json!({ "ok": true, "url": url }))
            }
            "browser_state" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                host.eval(
                    r#"({ url: location.href, title: document.title, readyState: document.readyState, viewport: [innerWidth, innerHeight], scroll: [scrollX, scrollY] })"#,
                    tool_channel(reply, id),
                );
                None
            }
            "browser_snapshot" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                let selector = args.get("selector").and_then(Value::as_str).unwrap_or("");
                let expression = format!(
                    "(function(){{var root={} ? document.querySelector({}) : document.body; if(!root)return null; return {{tag:root.tagName.toLowerCase(),html:root.outerHTML.slice(0,12000),text:(root.innerText||root.textContent||'').slice(0,12000)}};}})()",
                    if selector.is_empty() { "false" } else { "true" },
                    js_string(selector)
                );
                host.eval(&expression, tool_channel(reply, id));
                None
            }
            "browser_click" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                if let Some(selector) = args.get("selector").and_then(Value::as_str) {
                    let expression = format!(
                        "(function(){{var el=document.querySelector({}); if(!el)return false; var r=el.getBoundingClientRect(); window.__rbAgentCursor.move(r.x+r.width/2,r.y+r.height/2); window.__rbAgentCursor.click(r.x+r.width/2,r.y+r.height/2); el.click(); return true;}})()",
                        js_string(selector)
                    );
                    host.eval(&expression, tool_channel(reply, id));
                } else {
                    let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                    let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                    host.move_cursor(x, y);
                    host.click_at(x, y, tool_channel(reply, id));
                }
                None
            }
            "browser_type" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                let Some(text) = args.get("text").and_then(Value::as_str) else {
                    return Some(Err("text wajib diisi".to_string()));
                };
                let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
                let mut inserted = text.to_string();
                if submit {
                    inserted.push('\r');
                }
                if let Some(selector) = args.get("selector").and_then(Value::as_str) {
                    let expression = format!(
                        "(function(){{var el=document.querySelector({}); if(!el)return false; el.focus(); window.__rbAgentCursor.type(); document.execCommand('insertText',false,{}); return true;}})()",
                        js_string(selector),
                        js_string(&inserted)
                    );
                    host.eval(&expression, tool_channel(reply, id));
                } else {
                    host.insert_text(text, tool_channel(reply, id));
                }
                None
            }
            "browser_press" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                let Some(key) = args.get("key").and_then(Value::as_str) else {
                    return Some(Err("key wajib diisi".to_string()));
                };
                host.press_key(key, tool_channel(reply, id));
                None
            }
            "browser_scroll" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let expression = format!("window.scrollBy({x},{y}); true");
                host.eval(&expression, tool_channel(reply, id));
                None
            }
            "browser_screenshot" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("tidak ada tab aktif".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab belum siap".to_string()));
                };
                host.cdp(
                    "Page.captureScreenshot",
                    &json!({ "format": "png", "fromSurface": true }),
                    tool_channel(reply, id),
                );
                None
            }
            _ => Some(Err(format!("tool tidak dikenal: {name}"))),
        }
    }

    fn connect_agent(&mut self) {
        if self.shell.agent_connected() {
            match remove_agent_configs() {
                Ok(paths) => {
                    self.shell.set_agent_connected(false);
                    self.shell.show_toast(format!(
                        "Connect MCP dinonaktifkan ({} config dibersihkan)",
                        paths.len()
                    ));
                }
                Err(error) => self.shell.show_toast(format!("MCP gagal dinonaktifkan: {error}")),
            }
        } else {
            match write_agent_configs() {
                Ok(paths) => {
                    self.shell.set_agent_connected(true);
                    self.shell
                        .show_toast(format!("MCP tersambung ({} config)", paths.len()));
                }
                Err(error) => self.shell.show_toast(format!("MCP gagal: {error}")),
            }
        }
        self.redraw();
    }

    fn toggle_select(&mut self) {
        let enabled = !self.select_mode;
        self.select_mode = enabled;
        self.shell.set_select_mode(enabled);
        for host in self.webviews.values() {
            host.set_picker(enabled);
        }
        self.shell.show_toast(if enabled {
            "Select element aktif"
        } else {
            "Select element dimatikan"
        });
        self.redraw();
    }

    fn handle_element_message(&mut self, message: &str) {
        let Ok(value) = serde_json::from_str::<Value>(message) else {
            return;
        };
        if value.get("type").and_then(Value::as_str) != Some("rb-element-selected") {
            return;
        }
        self.agent_selection = Some(value.clone());
        self.select_mode = false;
        self.shell.set_select_mode(false);
        for host in self.webviews.values() {
            host.set_picker(false);
        }
        let text = element_message(&value);
        let sent = match self.terminal.as_ref() {
            Some(terminal) => terminal.send_text(&text),
            None => Err("terminal RushAgent belum dibuka".to_string()),
        };
        match sent {
            Ok(()) => self.shell.show_toast("Elemen dikirim ke agent"),
            Err(error) => self.shell.show_toast(format!("Elemen gagal dikirim: {error}")),
        }
        self.redraw();
    }

    fn draw(&mut self) {
        let (width, height) = self.size;
        if width <= 1.0 || height <= 1.0 {
            return;
        }
        let _ = self.shell.take_folder_cancel();
        if let Some(path) = self.shell.take_folder_selection() {
            self.start_rush_agent(path);
        }
        if self.terminal_exited() {
            self.stop_terminal();
        }
        let agent_ready = self.terminal.is_some();
        if self.shell.take_agent_connect() {
            if agent_ready {
                self.connect_agent();
            } else {
                self.shell.show_toast("Buka RushAgent dulu");
            }
        }
        if self.shell.take_select_element() {
            if agent_ready {
                self.toggle_select();
            } else {
                self.shell.show_toast("Buka RushAgent dulu");
            }
        }
        self.shell.set_maximized(self.window.is_maximized());
        // Drain PTY output before building the frame so terminal text is
        // visible in the redraw requested by the PTY reader thread.
        self.pump_terminal();
        let frame = self.shell.render(width, height);
        self.frame = frame.clone();
        match self.renderer.render(&frame) {
            Ok(_) => {}
            Err(RenderError::Outdated) => self.renderer.reconfigure(),
            Err(RenderError::Validation) => self.renderer.reconfigure(),
            Err(RenderError::Lost) => {
                let scale = self.renderer.scale_factor();
                match pollster::block_on(Renderer::new(self.window.clone())) {
                    Ok(mut renderer) => {
                        renderer.set_scale_factor(scale);
                        self.renderer = renderer;
                    }
                    Err(error) => eprintln!("recreate renderer: {error}"),
                }
            }
        }
        if !self.window_shown {
            self.window_shown = true;
            self.window.set_visible(true);
        }
        self.sync_active_tab();
    }

    fn hide_overlay(&mut self) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        overlay.bounds = Rect::default();
        if overlay.visible {
            overlay.window.set_visible(false);
            overlay.visible = false;
        }
    }

    fn draw_overlay(&mut self) {
        let Some(bounds) = self.shell.popup_bounds() else {
            self.hide_overlay();
            return;
        };
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            self.hide_overlay();
            return;
        }
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let scale = self.window.scale_factor() as f32;
        let width = (bounds.width * scale).ceil() as u32;
        let height = (bounds.height * scale).ceil() as u32;
        let width = width.max(1);
        let height = height.max(1);
        if overlay.size != (width, height) {
            let _ = overlay
                .window
                .request_inner_size(PhysicalSize::new(width, height));
            overlay.renderer.resize(width, height);
            overlay.size = (width, height);
        }
        if let Ok(origin) = self.window.inner_position() {
            overlay.window.set_outer_position(PhysicalPosition::new(
                origin.x + (bounds.x * scale).round() as i32,
                origin.y + (bounds.y * scale).round() as i32,
            ));
        }
        overlay.bounds = bounds;
        let mut frame = Frame::default();
        let logical = Rect::new(
            0.0,
            0.0,
            width as f32 / scale,
            height as f32 / scale,
        );
        frame.base.push_rect(RectShape::filled(
            logical,
            self.shell.popup_background(),
            0.0,
        ));
        frame.overlay = self.frame.popup.translated(bounds.x, bounds.y);
        match overlay.renderer.render(&frame) {
            Ok(_) => {}
            Err(RenderError::Outdated) => overlay.renderer.reconfigure(),
            Err(RenderError::Validation) => overlay.renderer.reconfigure(),
            Err(RenderError::Lost) => {
                let scale = overlay.renderer.scale_factor();
                match pollster::block_on(Renderer::new(overlay.window.clone())) {
                    Ok(mut renderer) => {
                        renderer.set_scale_factor(scale);
                        overlay.renderer = renderer;
                    }
                    Err(error) => eprintln!("recreate overlay renderer: {error}"),
                }
            }
        }
        if !overlay.visible {
            overlay.window.set_visible(true);
            overlay.visible = true;
        }
    }

    fn overlay_id(&self) -> Option<WindowId> {
        self.overlay.as_ref().map(|overlay| overlay.window.id())
    }

    fn overlay_point(&self, position: rb_ui::Vec2) -> rb_ui::Vec2 {
        let Some(overlay) = self.overlay.as_ref() else {
            return rb_ui::Vec2::new(-1000.0, -1000.0);
        };
        rb_ui::Vec2::new(
            overlay.bounds.x + position.x,
            overlay.bounds.y + position.y,
        )
    }

    fn update_overlay_cursor(&mut self, point: rb_ui::Vec2) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let cursor = match self.shell.cursor_role(point) {
            CursorRole::Text => CursorIcon::Text,
            CursorRole::Pointer => CursorIcon::Pointer,
            CursorRole::Default => CursorIcon::Default,
        };
        if cursor != overlay.cursor_icon {
            overlay.window.set_cursor(cursor);
            overlay.cursor_icon = cursor;
        }
    }

    fn overlay_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.hide_overlay(),
            WindowEvent::Resized(size) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.renderer.resize(size.width, size.height);
                    overlay.size = (size.width, size.height);
                    overlay.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.renderer.set_scale_factor(scale_factor as f32);
                    let size = overlay.window.inner_size();
                    overlay.renderer.resize(size.width, size.height);
                    overlay.size = (size.width, size.height);
                    overlay.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw_overlay(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.scale_factor() as f32;
                let local = rb_ui::Vec2::new(position.x as f32 / scale, position.y as f32 / scale);
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.cursor = local;
                }
                let point = self.overlay_point(local);
                self.update_overlay_cursor(point);
                let dirty = self.shell.pointer_moved(point);
                if dirty {
                    self.draw();
                    self.draw_overlay();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.shell.pointer_moved(rb_ui::Vec2::new(-1000.0, -1000.0));
                self.draw();
                self.draw_overlay();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.shell.folder_picker_scroll(wheel_lines(delta)) {
                    self.draw();
                    self.draw_overlay();
                }
            }
            WindowEvent::MouseInput {
                state: pressed,
                button,
                ..
            } => {
                let local = self
                    .overlay
                    .as_ref()
                    .map(|overlay| overlay.cursor)
                    .unwrap_or(rb_ui::Vec2::new(-1000.0, -1000.0));
                let point = self.overlay_point(local);
                self.shell.pointer_moved(point);
                match button {
                    MouseButton::Left => match pressed {
                        ElementState::Pressed => self.shell.left_click(point),
                        ElementState::Released => self.shell.left_released(point),
                    },
                    MouseButton::Right if pressed == ElementState::Pressed => {
                        self.shell.right_click(point)
                    }
                    _ => return,
                }
                if self.shell.take_rush_agent() {
                    self.open_rush_agent();
                    self.draw_overlay();
                    return;
                }
                if self.shell.take_terminal_close() {
                    self.stop_terminal();
                }
                self.window.focus_window();
                self.pump_navigation();
                self.draw();
                self.draw_overlay();
                if let Some(action) = self.shell.take_chrome_action() {
                    self.apply_chrome_action(action, event_loop);
                }
                if self.shell.take_fullscreen_exit() {
                    self.set_web_fullscreen(false);
                }
            }
            _ => {}
        }
    }

    fn sync_active_tab(&mut self) {
        let active = self.shell.focused_tab_id();
        if let (Some(env), Some(id)) = (self.webview_env.as_ref(), active) {
            env.set_active_tab(id);
        }
        self.sync_webview_visibility();
        self.sync_webview_bounds();
    }

    fn inset_content_rect(&self) -> Rect {
        let (width, height) = self.size;
        let logical = self.shell.content_rect(width, height);
        let inset = if self.shell.is_fullscreen() || self.window.is_maximized() {
            0.0
        } else {
            RESIZE_EDGE
        };
        Rect::new(
            logical.x + inset,
            logical.y,
            (logical.width - inset * 2.0).max(0.0),
            (logical.height - inset).max(0.0),
        )
    }

    /// Pane biasa memakai satu WebView; split view memakai hingga tiga WebView yang
    /// semuanya tampil. Tab non-pane tetap hidup di background.
    fn webview_layout(&self) -> Vec<(TabId, Rect)> {
        let scale = self.window.scale_factor() as f32;
        self.shell
            .webview_layout(self.inset_content_rect())
            .into_iter()
            .map(|(tab_id, rect)| {
                (
                    tab_id,
                    Rect::new(
                        rect.x * scale,
                        rect.y * scale,
                        rect.width * scale,
                        rect.height * scale,
                    ),
                )
            })
            .collect()
    }

    fn content_bounds(&self) -> Rect {
        self.webview_layout()
            .first()
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    fn sync_webview_visibility(&mut self) {
        let target: Vec<TabId> = self
            .webview_layout()
            .into_iter()
            .map(|(tab_id, _)| tab_id)
            .filter(|tab_id| self.ready.contains(tab_id))
            .collect();
        for previous in &self.visible_webviews {
            if !target.contains(previous) {
                if let Some(host) = self.webviews.get(previous) {
                    if let Err(error) = host.set_visible(false) {
                        eprintln!("webview hide: {error}");
                    }
                    // Tab yang disembunyikan turun ke memori rendah; tab audio tetap jalan.
                    if let Err(error) = host.set_background(true) {
                        eprintln!("webview background: {error}");
                    }
                }
            }
        }
        for id in &target {
            if !self.visible_webviews.contains(id) {
                if let Some(host) = self.webviews.get(id) {
                    if let Err(error) = host.set_background(false) {
                        eprintln!("webview foreground: {error}");
                    }
                    if let Err(error) = host.set_visible(true) {
                        eprintln!("webview show: {error}");
                    }
                }
            }
        }
        self.visible_webviews = target;
    }

    fn sync_webview_bounds(&mut self) {
        for (tab_id, physical) in self.webview_layout() {
            let key = (
                physical.x.round() as i32,
                physical.y.round() as i32,
                physical.width.round() as i32,
                physical.height.round() as i32,
            );
            if self.last_webview_bounds.get(&tab_id) == Some(&key) {
                continue;
            }
            self.last_webview_bounds.insert(tab_id, key);
            if let Some(host) = self.webviews.get(&tab_id) {
                if let Err(error) = host.set_bounds(physical) {
                    eprintln!("webview bounds: {error}");
                }
            }
        }
    }

    fn ensure_webview(&mut self, tab_id: TabId) -> Result<(), String> {
        if self.webviews.contains_key(&tab_id) {
            return Ok(());
        }
        let hwnd = self.parent_hwnd()?;
        let host = self
            .webview_env
            .as_ref()
            .ok_or_else(|| "WebView2 environment is not ready".to_string())?
            .create_host(hwnd, tab_id, self.proxy.clone())?;
        if let Err(error) = host.set_bounds(self.content_bounds()) {
            eprintln!("webview bounds: {error}");
        }
        self.webviews.insert(tab_id, host);
        Ok(())
    }

    fn pump_navigation(&mut self) {
        for tab_id in self.shell.take_closed_tabs() {
            self.webviews.remove(&tab_id);
            self.ready.remove(&tab_id);
            self.last_webview_bounds.remove(&tab_id);
            self.visible_webviews.retain(|visible| *visible != tab_id);
        }
        while let Some(request) = self.shell.take_navigation() {
            if let Err(error) = self.ensure_webview(request.tab_id) {
                eprintln!("webview create: {error}");
                continue;
            }
            let Some(host) = self.webviews.get(&request.tab_id) else {
                continue;
            };
            let result = match request.kind {
                rb_ui_widgets::NavigationKind::Load => host.load(&request.url),
                rb_ui_widgets::NavigationKind::Reload => host.reload(),
            };
            if let Err(error) = result {
                eprintln!("webview navigate {}: {error}", request.url);
            }
        }
        self.sync_active_tab();
    }

    fn redraw(&self) {
        self.window.request_redraw();
        if let Some(overlay) = self.overlay.as_ref() {
            if overlay.visible {
                overlay.window.request_redraw();
            }
        }
    }

    fn apply_chrome_action(&mut self, action: ChromeAction, event_loop: &ActiveEventLoop) {
        match action {
            ChromeAction::Minimize => {
                self.window.set_minimized(true);
                webview::trim_working_set();
            }
            ChromeAction::MaximizeToggle => {
                self.window.set_maximized(!self.window.is_maximized());
            }
            ChromeAction::Close => event_loop.exit(),
        }
    }

    fn set_web_fullscreen(&mut self, enabled: bool) {
        if enabled {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
        } else {
            self.window.set_fullscreen(None);
        }
        self.shell.set_fullscreen(enabled);
        self.redraw();
    }

    fn update_cursor(&mut self) {
        let direction = resize_direction(self.cursor, self.size);
        let cursor = if let Some(direction) = direction {
            resize_cursor(direction)
        } else {
            match self.shell.cursor_role(self.cursor) {
                CursorRole::Text => CursorIcon::Text,
                CursorRole::Pointer => CursorIcon::Pointer,
                CursorRole::Default => CursorIcon::Default,
            }
        };
        if cursor != self.cursor_icon {
            self.window.set_cursor(cursor);
            self.cursor_icon = cursor;
        }
    }

    fn start_drag(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(error) = self.window.drag_window() {
            eprintln!("drag window: {error}");
        }
    }

    fn handle_accelerator(
        &mut self,
        tab_id: TabId,
        key: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        event_loop: &ActiveEventLoop,
    ) {
        if self.shell.focused_tab_id() != Some(tab_id) {
            return;
        }
        let focus_shell = if ctrl && !alt {
            match key {
                VK_TAB => {
                    self.shell.cycle_tabs(if shift { -1 } else { 1 });
                    true
                }
                VK_T => {
                    self.shell.new_tab();
                    true
                }
                VK_W => {
                    self.shell.close_active_tab();
                    true
                }
                VK_L => {
                    self.shell.focus_address();
                    true
                }
                VK_D => {
                    self.shell.toggle_bookmark();
                    true
                }
                VK_A => {
                    self.shell.select_all();
                    true
                }
                VK_R => {
                    self.shell.reload();
                    false
                }
                VK_Q => {
                    event_loop.exit();
                    return;
                }
                _ => return,
            }
        } else if alt && !ctrl {
            match key {
                VK_LEFT => {
                    self.shell.go_back();
                    false
                }
                VK_RIGHT => {
                    self.shell.go_forward();
                    false
                }
                _ => return,
            }
        } else {
            return;
        };

        if focus_shell {
            self.window.focus_window();
        }
        self.pump_navigation();
        self.redraw();
    }

    fn insert_text(&mut self, text: &str) {
        self.shell.insert_text(text);
    }

    fn handle_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }

        let ctrl = self.modifiers.control_key();
        let alt = self.modifiers.alt_key();
        let shift = self.modifiers.shift_key();

        match &event.logical_key {
            Key::Named(NamedKey::Tab) if ctrl => {
                self.shell.cycle_tabs(if shift { -1 } else { 1 });
                self.pump_navigation();
                self.redraw();
                return;
            }
            Key::Named(NamedKey::ArrowLeft) if alt => {
                self.shell.go_back();
                self.pump_navigation();
                self.redraw();
                return;
            }
            Key::Named(NamedKey::ArrowRight) if alt => {
                self.shell.go_forward();
                self.pump_navigation();
                self.redraw();
                return;
            }
            Key::Named(NamedKey::Backspace) => self.shell.backspace(),
            Key::Named(NamedKey::Delete) => self.shell.delete_forward(),
            Key::Named(NamedKey::ArrowLeft) => self.shell.move_left(ctrl),
            Key::Named(NamedKey::ArrowRight) => self.shell.move_right(ctrl),
            Key::Named(NamedKey::Home) => self.shell.move_home(),
            Key::Named(NamedKey::End) => self.shell.move_end(),
            Key::Named(NamedKey::Enter) => self.shell.submit(),
            Key::Named(NamedKey::Escape) => {
                if self.shell.is_fullscreen() {
                    self.set_web_fullscreen(false);
                } else {
                    self.shell.escape();
                    if self.shell.take_terminal_close() {
                        self.stop_terminal();
                    }
                }
            }
            Key::Named(NamedKey::Space) if !ctrl && !alt => {
                if let Some(text) = &event.text {
                    if text.is_ascii() {
                        self.insert_text(text);
                    }
                }
                self.redraw();
                return;
            }
            _ => {}
        }

        // Tombol edit (backspace, delete, panah, enter) mengubah teks tanpa event teks,
        // jadi layar harus digambar ulang di sini.
        self.pump_navigation();
        self.redraw();

        if ctrl {
            let key = match &event.logical_key {
                Key::Character(ch) => ch.to_lowercase(),
                _ => String::new(),
            };
            match key.as_str() {
                "l" => self.shell.focus_address(),
                "t" => self.shell.new_tab(),
                "w" => self.shell.close_active_tab(),
                "d" => self.shell.toggle_bookmark(),
                "a" => self.shell.select_all(),
                "r" => self.shell.reload(),
                "q" => {
                    event_loop.exit();
                    return;
                }
                _ => {}
            }
            self.pump_navigation();
            self.redraw();
            return;
        }

        if alt {
            return;
        }

        if let Some(text) = &event.text {
            if text.is_ascii() && text.chars().any(|ch| !ch.is_control()) {
                self.shell.insert_text(text);
                self.redraw();
            }
        }
    }
}

#[derive(Default)]
struct App {
    state: Option<State>,
    proxy: Option<EventLoopProxy<UserEvent>>,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let mut width = 1280.0f64;
        let mut height = 800.0f64;
        if let Some(monitor) = event_loop.primary_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let available_w = size.width as f64 / scale - 48.0;
            let available_h = size.height as f64 / scale - 96.0;
            width = width.min(available_w.max(720.0));
            height = height.min(available_h.max(480.0));
        }

        let mut attributes = Window::default_attributes()
            .with_title("RushBross")
            .with_inner_size(LogicalSize::new(width, height))
            .with_min_inner_size(LogicalSize::new(720.0, 480.0))
            .with_decorations(false)
            .with_undecorated_shadow(true)
            .with_visible(false);
        if let Some(icon) = window_icon() {
            attributes = attributes.with_window_icon(Some(icon));
        }

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("create window: {error}");
                event_loop.exit();
                return;
            }
        };
        window.set_ime_allowed(true);
        round_window_corners(&window);

        let renderer = match pollster::block_on(Renderer::new(window.clone())) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("create renderer: {error}");
                event_loop.exit();
                return;
            }
        };

        let proxy = match self.proxy.clone() {
            Some(proxy) => proxy,
            None => {
                eprintln!("missing event loop proxy");
                event_loop.exit();
                return;
            }
        };
        let mut state = match State::new(window, renderer, proxy.clone()) {
            Ok(state) => state,
            Err(error) => {
                eprintln!("create state: {error}");
                event_loop.exit();
                return;
            }
        };
        if let Err(error) = state.init_overlay(event_loop) {
            eprintln!("create overlay: {error}");
            event_loop.exit();
            return;
        }
        state.draw();
        state.draw_overlay();
        self.state = Some(state);
        if let Some(state) = self.state.as_mut() {
            if let Err(error) = state.init_webview() {
                eprintln!("create webview: {error}");
            }
            state.redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let is_main = window_id == state.window.id();
        let is_overlay = state.overlay_id() == Some(window_id);
        if !is_main && !is_overlay {
            return;
        }
        if is_overlay {
            state.overlay_event(event_loop, event);
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                state.stop_terminal();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                let scale = state.window.scale_factor() as f32;
                state.size = (size.width as f32 / scale, size.height as f32 / scale);
                state.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state.renderer.set_scale_factor(scale_factor as f32);
                if let Some(overlay) = state.overlay.as_mut() {
                    overlay.renderer.set_scale_factor(scale_factor as f32);
                }
                let physical = state.window.inner_size();
                state.renderer.resize(physical.width, physical.height);
                state.size = (
                    physical.width as f32 / scale_factor as f32,
                    physical.height as f32 / scale_factor as f32,
                );
                state.redraw();
            }
            WindowEvent::Moved(_) => state.draw_overlay(),
            WindowEvent::RedrawRequested => {
                state.draw();
                state.draw_overlay();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = state.window.scale_factor() as f32;
                let point = rb_ui::Vec2::new(position.x as f32 / scale, position.y as f32 / scale);
                state.cursor = point;
                let dirty = state.shell.pointer_moved(point);
                state.update_cursor();
                if dirty {
                    state.redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                state.cursor = rb_ui::Vec2::new(-1000.0, -1000.0);
                state.shell.pointer_moved(state.cursor);
                state.update_cursor();
                state.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.shell.folder_picker_scroll(wheel_lines(delta)) {
                    state.redraw();
                }
            }
            WindowEvent::MouseInput {
                state: pressed,
                button,
                ..
            } => {
                match button {
                    MouseButton::Left => match pressed {
                        ElementState::Pressed => {
                            if !state.shell.is_fullscreen() && !state.window.is_maximized() {
                                if let Some(direction) = resize_direction(state.cursor, state.size)
                                {
                                    if let Err(error) = state.window.drag_resize_window(direction) {
                                        eprintln!("resize window: {error}");
                                    }
                                    return;
                                }
                                if state.shell.hit_chrome(state.cursor)
                                    == Some(ChromeElement::DragStrip)
                                {
                                    state.start_drag(event_loop);
                                    return;
                                }
                            }
                            state.shell.left_click(state.cursor);
                            if state.shell.take_fullscreen_exit() {
                                state.set_web_fullscreen(false);
                                return;
                            }
                            if let Some(action) = state.shell.take_chrome_action() {
                                state.apply_chrome_action(action, event_loop);
                                return;
                            }
                        }
                        ElementState::Released => state.shell.left_released(state.cursor),
                    },
                    MouseButton::Right if pressed == ElementState::Pressed => {
                        if !state.shell.is_fullscreen()
                            && state.shell.hit_chrome(state.cursor)
                                == Some(ChromeElement::DragStrip)
                        {
                            let scale = state.window.scale_factor();
                            let point = PhysicalPosition::new(
                                state.cursor.x as f64 * scale,
                                state.cursor.y as f64 * scale,
                            );
                            state.window.show_window_menu(point);
                            return;
                        }
                        state.shell.right_click(state.cursor);
                    }
                    _ => {}
                }
                if state.shell.take_rush_agent() {
                    state.open_rush_agent();
                    return;
                }
                if state.shell.take_terminal_close() {
                    state.stop_terminal();
                }
                state.pump_navigation();
                state.redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => state.handle_key(&event, event_loop),
            WindowEvent::Ime(Ime::Commit(text)) => {
                state.insert_text(&text);
                state.redraw();
            }
            _ => {}
        }
        state.pump_navigation();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            UserEvent::WebView(webview::WebViewEvent::Loaded(tab_id)) => {
                state.ready.insert(tab_id);
                let select_mode = state.select_mode;
                if let Some(host) = state.webviews.get(&tab_id) {
                    host.install_agent_helpers();
                    if select_mode {
                        host.set_picker(true);
                    }
                }
                state.shell.webview_loaded(tab_id);
                state.sync_webview_visibility();
            }
            UserEvent::WebView(webview::WebViewEvent::Favicon(tab_id, favicon)) => {
                state.shell.webview_favicon(tab_id, favicon)
            }
            UserEvent::WebView(webview::WebViewEvent::TitleChanged(tab_id, title)) => {
                state.shell.webview_title_changed(tab_id, &title)
            }
            UserEvent::WebView(webview::WebViewEvent::UrlChanged(tab_id, url)) => {
                state.shell.webview_url_changed(tab_id, &url)
            }
            UserEvent::WebView(webview::WebViewEvent::PlayingAudio(tab_id, playing)) => {
                state.shell.webview_playing_audio(tab_id, playing)
            }
            UserEvent::WebView(webview::WebViewEvent::NewWindowRequested(url)) => {
                state.shell.webview_new_window(&url)
            }
            UserEvent::WebView(webview::WebViewEvent::FullScreenChanged(tab_id, enabled)) => {
                if state.shell.focused_tab_id() == Some(tab_id) {
                    state.set_web_fullscreen(enabled);
                }
                return;
            }
            UserEvent::WebView(webview::WebViewEvent::Accelerator {
                tab_id,
                key,
                ctrl,
                alt,
                shift,
            }) => {
                state.handle_accelerator(tab_id, key, ctrl, alt, shift, event_loop);
                return;
            }
            UserEvent::WebView(webview::WebViewEvent::WebMessage(message)) => {
                state.handle_element_message(&message);
                return;
            }
            UserEvent::Agent(request) => {
                state.handle_agent_request(request);
                return;
            }
        }
        state.pump_navigation();
        state.redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            if state.terminal_exited() {
                state.stop_terminal();
                state.redraw();
            }
            if state.shell.has_animation() {
                state.redraw();
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App {
        state: None,
        proxy: Some(event_loop.create_proxy()),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Membulatkan sudut jendela tanpa dekorasi di Windows 11. Di Windows 10 atau
/// saat DWM menolak atribut ini, jendela tetap persegi tanpa error.
fn round_window_corners(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut c_void);
    let preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

fn resize_direction(point: rb_ui::Vec2, size: (f32, f32)) -> Option<ResizeDirection> {
    let (width, height) = size;
    let left = point.x <= RESIZE_EDGE;
    let right = point.x >= width - RESIZE_EDGE;
    let top = point.y <= RESIZE_EDGE;
    let bottom = point.y >= height - RESIZE_EDGE;
    match (top, bottom, left, right) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (true, _, _, true) => Some(ResizeDirection::NorthEast),
        (_, true, true, _) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::North),
        (_, true, _, _) => Some(ResizeDirection::South),
        (_, _, true, _) => Some(ResizeDirection::West),
        (_, _, _, true) => Some(ResizeDirection::East),
        _ => None,
    }
}

fn resize_cursor(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::NResize,
        ResizeDirection::South => CursorIcon::SResize,
        ResizeDirection::East => CursorIcon::EResize,
        ResizeDirection::West => CursorIcon::WResize,
        ResizeDirection::NorthEast => CursorIcon::NeResize,
        ResizeDirection::NorthWest => CursorIcon::NwResize,
        ResizeDirection::SouthEast => CursorIcon::SeResize,
        ResizeDirection::SouthWest => CursorIcon::SwResize,
    }
}

fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../../../assets/icons.png"
    )));
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];
    let width = info.width;
    let height = info.height;
    let mut rgba = match info.color_type {
        png::ColorType::Rgb => bytes
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Grayscale => bytes
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    app_icon::round_and_clear_background(&mut rgba, width, height);
    let size = 256;
    Some((app_icon::downscale(&rgba, width, height, size), size, size))
}

fn window_icon() -> Option<Icon> {
    let (rgba, width, height) = icon_rgba()?;
    Icon::from_rgba(rgba, width, height).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_direction_detects_edges_and_corners() {
        let size = (800.0, 600.0);
        assert_eq!(
            resize_direction(rb_ui::Vec2::new(2.0, 2.0), size),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            resize_direction(rb_ui::Vec2::new(798.0, 300.0), size),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            resize_direction(rb_ui::Vec2::new(400.0, 598.0), size),
            Some(ResizeDirection::South)
        );
        assert_eq!(resize_direction(rb_ui::Vec2::new(400.0, 300.0), size), None);
    }

    #[test]
    fn icon_rgba_has_transparent_background() {
        let (rgba, width, height) = icon_rgba().expect("icon should decode");
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 128));
    }
}
