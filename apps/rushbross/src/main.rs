#![windows_subsystem = "windows"]

mod webview;

mod app_icon;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use rb_agent::{AgentRequest, AgentServer};
use rb_tabs::TabId;
use rb_terminal::{Key as TermKey, Mods, PtyTerminal, CELL_WIDTH, LINE_HEIGHT};
use rb_ui::{Frame, Rect, RectShape};
use rb_ui_renderer::{RenderError, Renderer};
use rb_ui_widgets::{BrowserShell, ChromeAction, ChromeElement, CursorRole};
use rb_ui_widgets::{TERMINAL_PADDING_X, TERMINAL_PADDING_Y};
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

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    unsafe {
        OpenClipboard(Some(HWND::default())).map_err(|error| error.to_string())?;
        let result = (|| {
            EmptyClipboard().map_err(|error| error.to_string())?;
            let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            let handle =
                GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|error| error.to_string())?;
            let pointer = GlobalLock(handle) as *mut u16;
            if pointer.is_null() {
                let _ = GlobalFree(Some(handle));
                return Err("clipboard lock failed".to_string());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
            let _ = GlobalUnlock(handle);
            if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0))) {
                let _ = GlobalFree(Some(handle));
                return Err(error.to_string());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
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
            "description": "Get the URL, title, and viewport of the active RushBross page.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_navigate",
            "description": "Open a URL in the active RushBross tab.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"]
            }
        },
        {
            "name": "browser_snapshot",
            "description": "Get a DOM summary of the active page.",
            "inputSchema": {
                "type": "object",
                "properties": { "selector": { "type": "string" } }
            }
        },
        {
            "name": "browser_click",
            "description": "Click an element by CSS selector or viewport coordinates.",
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
            "description": "Type text into the selected element, optionally pressing Enter afterwards.",
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
            "description": "Press a single keyboard key, such as Enter, Tab, or Escape.",
            "inputSchema": {
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }
        },
        {
            "name": "browser_scroll",
            "description": "Scroll the active page.",
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
            "description": "Capture a screenshot of the active page as a PNG data URL.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_select",
            "description": "Enable or disable element picker mode.",
            "inputSchema": {
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } }
            }
        },
        {
            "name": "browser_get_selection",
            "description": "Get the last element selected in Select mode.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "browser_tabs",
            "description": "List tabs, create a new tab, activate a tab, or close the active tab.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "new", "activate", "close"] },
                    "id": { "type": "integer" }
                }
            }
        },
        {
            "name": "browser_hover",
            "description": "Move the green cursor to an element or coordinates without clicking.",
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
            "name": "browser_eval",
            "description": "Run a JavaScript expression on the active page and return the result.",
            "inputSchema": {
                "type": "object",
                "properties": { "expression": { "type": "string" } },
                "required": ["expression"]
            }
        }
    ])
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Terjemahkan tombol navigasi dan fungsi winit ke tombol terminal.
fn named_to_terminal_key(named: NamedKey) -> Option<TermKey> {
    Some(match named {
        NamedKey::Enter => TermKey::Enter,
        NamedKey::Backspace => TermKey::Backspace,
        NamedKey::Tab => TermKey::Tab,
        NamedKey::Escape => TermKey::Escape,
        NamedKey::ArrowUp => TermKey::Up,
        NamedKey::ArrowDown => TermKey::Down,
        NamedKey::ArrowLeft => TermKey::Left,
        NamedKey::ArrowRight => TermKey::Right,
        NamedKey::Home => TermKey::Home,
        NamedKey::End => TermKey::End,
        NamedKey::PageUp => TermKey::PageUp,
        NamedKey::PageDown => TermKey::PageDown,
        NamedKey::Insert => TermKey::Insert,
        NamedKey::Delete => TermKey::Delete,
        NamedKey::F1 => TermKey::F(1),
        NamedKey::F2 => TermKey::F(2),
        NamedKey::F3 => TermKey::F(3),
        NamedKey::F4 => TermKey::F(4),
        NamedKey::F5 => TermKey::F(5),
        NamedKey::F6 => TermKey::F(6),
        NamedKey::F7 => TermKey::F(7),
        NamedKey::F8 => TermKey::F(8),
        NamedKey::F9 => TermKey::F(9),
        NamedKey::F10 => TermKey::F(10),
        NamedKey::F11 => TermKey::F(11),
        NamedKey::F12 => TermKey::F(12),
        _ => return None,
    })
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
            .map_err(|_| format!("{} is not valid JSON", path.display()))?
    } else {
        json!({})
    };
    let mut cursor = &mut root;
    for key in keys {
        let object = cursor
            .as_object_mut()
            .ok_or_else(|| format!("{} is not an object", path.display()))?;
        cursor = object
            .entry((*key).to_string())
            .or_insert_with(|| json!({}));
    }
    cursor
        .as_object_mut()
        .ok_or_else(|| format!("{} is not an object", path.display()))?
        .insert(server_key.to_string(), server);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())
}

fn prune_empty_objects(value: &mut Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let keys: Vec<String> = object.keys().cloned().collect();
    for key in keys {
        if let Some(child) = object.get_mut(&key) {
            if prune_empty_objects(child) {
                object.remove(&key);
            }
        }
    }
    object.is_empty()
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
        .map_err(|_| format!("{} is not valid JSON", path.display()))?;
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
        if prune_empty_objects(&mut root) {
            std::fs::remove_file(path).map_err(|error| error.to_string())?;
        } else {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(removed)
}

fn bridge_command() -> Result<(String, Vec<String>), String> {
    Ok((
        std::env::current_exe()
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned(),
        vec!["--mcp".to_string()],
    ))
}

fn user_home() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Config JSON yang dikenali CLI agent. `keys` adalah jalur objek induknya.
fn json_targets(
    folder: &std::path::Path,
    home: Option<&std::path::Path>,
) -> Vec<(std::path::PathBuf, Vec<&'static str>, bool)> {
    let mut targets = vec![
        (folder.join("opencode.json"), vec!["mcp"], true),
        (folder.join(".kilo").join("kilo.json"), vec!["mcp"], true),
        (folder.join(".mcp.json"), vec!["mcpServers"], false),
        (
            folder.join(".agents").join("mcp_config.json"),
            vec!["mcpServers"],
            false,
        ),
        (folder.join(".cursor").join("mcp.json"), vec!["mcpServers"], false),
        (folder.join(".vscode").join("mcp.json"), vec!["servers"], false),
        (folder.join(".gemini").join("settings.json"), vec!["mcpServers"], false),
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

const MCP_START: &str = "<!-- rushbross:mcp:start -->";
const MCP_END: &str = "<!-- rushbross:mcp:end -->";

const MCP_INSTRUCTIONS: &str = r#"Use the `rushbross` MCP server to control the RushBross browser in the same window as this terminal.

Available tools:
- `browser_state`: URL, title, viewport, and scroll position of the active page.
- `browser_navigate`: open a URL in the active tab.
- `browser_snapshot`: read the page DOM/text; use `selector` to limit the result.
- `browser_click`: click an element by CSS selector or `x`/`y` coordinates.
- `browser_type`: type text into a field; `submit: true` presses Enter afterwards.
- `browser_press`: press Enter, Tab, Escape, or an arrow key.
- `browser_scroll`: scroll the page relatively using `x`/`y`.
- `browser_screenshot`: capture the page as a PNG data URL.
- `browser_tabs`: `list`, `new`, `activate` (requires `id`), or `close`.
- `browser_hover`: move the green cursor to a selector or coordinates without clicking.
- `browser_eval`: run a JavaScript expression on the active page when other tools are insufficient.
- `browser_select`: ask the user to pick an element with the green cursor.
- `browser_get_selection`: get details of the last element selected by the user.

Suggested flow: call `browser_state`, then `browser_snapshot`, perform the action, and verify with `browser_snapshot` or `browser_screenshot`. The green cursor marks agent actions. When the user selects an element in Select mode, a summary is also sent to this terminal automatically, but `browser_get_selection` returns full details. For ordinary web exploration, use these tools as freely as a person would use a browser; ask for confirmation only for destructive or paid actions."#;

struct ManagedFile {
    path: std::path::PathBuf,
    body: String,
    standalone: bool,
}

fn managed_section(body: &str) -> String {
    format!("{MCP_START}\n{}\n{MCP_END}\n", body.trim())
}

fn managed_files(folder: &std::path::Path) -> Vec<ManagedFile> {
    let section = |path: std::path::PathBuf| ManagedFile {
        path,
        body: MCP_INSTRUCTIONS.to_string(),
        standalone: false,
    };
    let standalone = |path: std::path::PathBuf, frontmatter: &str| ManagedFile {
        path,
        body: format!("{}\n\n{}", frontmatter, managed_section(MCP_INSTRUCTIONS)),
        standalone: true,
    };
    vec![
        section(folder.join("AGENTS.md")),
        section(folder.join("GEMINI.md")),
        section(folder.join(".github").join("copilot-instructions.md")),
        standalone(
            folder.join(".cursor").join("rules").join("rushbross.mdc"),
            "---\ndescription: Control the RushBross browser through MCP\nglobs: **\nalwaysApply: true\n---",
        ),
        standalone(
            folder.join(".kilocode").join("rules").join("rushbross.mdc"),
            "---\ndescription: Control the RushBross browser through MCP\nglobs: **\nalwaysApply: true\n---",
        ),
        standalone(
            folder.join(".windsurf").join("rules").join("rushbross.md"),
            "# RushBross",
        ),
        standalone(
            folder
                .join(".claude")
                .join("skills")
                .join("rushbross")
                .join("SKILL.md"),
            "---\nname: rushbross\ndescription: Control the RushBross browser through MCP while navigating, clicking, typing, or taking screenshots.\n---",
        ),
        standalone(
            folder
                .join(".agents")
                .join("skills")
                .join("rushbross")
                .join("SKILL.md"),
            "---\nname: rushbross\ndescription: Control the RushBross browser through MCP while navigating, clicking, typing, or taking screenshots.\n---",
        ),
    ]
}

fn upsert_managed_file(file: &ManagedFile) -> Result<bool, String> {
    let existing = if file.path.exists() {
        std::fs::read_to_string(&file.path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let block = if file.standalone {
        file.body.clone()
    } else {
        managed_section(&file.body)
    };
    let updated = if let Some(begin) = existing.find(MCP_START) {
        let end = existing[begin..]
            .find(MCP_END)
            .map(|offset| begin + offset + MCP_END.len())
            .unwrap_or(existing.len());
        format!(
            "{}{}{}",
            &existing[..begin],
            block,
            &existing[end..]
        )
    } else if existing.trim().is_empty() {
        block
    } else {
        format!("{}\n\n{}", existing.trim_end(), block)
    };
    if updated == existing {
        return Ok(false);
    }
    if let Some(parent) = file.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&file.path, updated).map_err(|error| error.to_string())?;
    Ok(true)
}

fn remove_managed_file(file: &ManagedFile) -> Result<bool, String> {
    if !file.path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&file.path).map_err(|error| error.to_string())?;
    if file.standalone && text.contains(MCP_START) {
        std::fs::remove_file(&file.path).map_err(|error| error.to_string())?;
        return Ok(true);
    }
    let Some(begin) = text.find(MCP_START) else {
        return Ok(false);
    };
    let end = text[begin..]
        .find(MCP_END)
        .map(|offset| begin + offset + MCP_END.len())
        .unwrap_or(text.len());
    let trimmed = format!("{}{}", &text[..begin], &text[end..]);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        std::fs::remove_file(&file.path).map_err(|error| error.to_string())?;
    } else {
        std::fs::write(&file.path, format!("{trimmed}\n")).map_err(|error| error.to_string())?;
    }
    Ok(true)
}

fn mcp_manifest_path(folder: &std::path::Path) -> std::path::PathBuf {
    folder.join(".rushbross").join("mcp-manifest.json")
}

fn mcp_target_paths(
    folder: &std::path::Path,
    home: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<std::path::PathBuf> = json_targets(folder, home)
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    if let Some(path) = codex_config(home) {
        paths.push(path);
    }
    paths.extend(managed_files(folder).into_iter().map(|file| file.path));
    paths
}

fn snapshot_mcp_files(paths: &[std::path::PathBuf]) -> Result<Vec<Value>, String> {
    paths
        .iter()
        .map(|path| {
            let content = if path.exists() {
                Some(std::fs::read_to_string(path).map_err(|error| error.to_string())?)
            } else {
                None
            };
            Ok(json!({
                "path": path.to_string_lossy(),
                "existed": content.is_some(),
                "content": content,
            }))
        })
        .collect()
}

fn write_mcp_manifest(folder: &std::path::Path, files: &[Value]) -> Result<(), String> {
    let path = mcp_manifest_path(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let manifest = json!({ "version": 1, "files": files });
    std::fs::write(
        path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn prune_empty_dirs(folder: &std::path::Path, paths: &[std::path::PathBuf]) {
    let mut dirs: Vec<std::path::PathBuf> = paths
        .iter()
        .filter_map(|path| path.parent().map(std::path::Path::to_path_buf))
        .collect();
    dirs.push(mcp_manifest_path(folder).parent().unwrap().to_path_buf());
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in dirs {
        let mut current = dir;
        loop {
            if current == folder || !current.starts_with(folder) {
                break;
            }
            if std::fs::remove_dir(&current).is_err() {
                break;
            }
            let Some(parent) = current.parent() else {
                break;
            };
            current = parent.to_path_buf();
        }
    }
}

fn restore_mcp_manifest(
    folder: &std::path::Path,
    removed: &mut Vec<String>,
    failures: &mut Vec<String>,
) -> bool {
    let manifest_path = mcp_manifest_path(folder);
    if !manifest_path.exists() {
        return false;
    }
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => {
            failures.push(error.to_string());
            return false;
        }
    };
    let manifest: Value = match serde_json::from_str(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            failures.push(error.to_string());
            return false;
        }
    };
    let Some(entries) = manifest.get("files").and_then(Value::as_array) else {
        return false;
    };
    let mut paths = Vec::new();
    for entry in entries {
        let Some(value) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let path = std::path::PathBuf::from(value);
        paths.push(path.clone());
        let existed = entry.get("existed").and_then(Value::as_bool).unwrap_or(false);
        let result = if !existed {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|error| error.to_string())
            } else {
                Ok(())
            }
        } else {
            if let Some(parent) = path.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    failures.push(error.to_string());
                    continue;
                }
            }
            let content = entry
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            std::fs::write(&path, content).map_err(|error| error.to_string())
        };
        match result {
            Ok(()) => removed.push(path.display().to_string()),
            Err(error) => failures.push(error),
        }
    }
    let _ = std::fs::remove_file(&manifest_path);
    paths.push(manifest_path);
    prune_empty_dirs(folder, &paths);
    true
}

fn write_agent_configs(folder: &std::path::Path) -> Result<Vec<String>, String> {
    let (command, args) = bridge_command()?;
    let home = user_home();
    let command_with_args: Vec<String> = std::iter::once(command.clone())
        .chain(args.iter().cloned())
        .collect();

    let local_server = json!({
        "type": "local",
        "command": command_with_args,
        "enabled": true,
        "timeout": 30000
    });
    let stdio_server = json!({
        "type": "stdio",
        "command": command.clone(),
        "args": args.clone()
    });

    let all_paths = mcp_target_paths(folder, home.as_deref());
    let snapshot = snapshot_mcp_files(&all_paths)?;

    let mut written = Vec::new();
    let mut failures = Vec::new();
    for (path, keys, is_local) in json_targets(folder, home.as_deref()) {
        let server = if is_local {
            local_server.clone()
        } else {
            stdio_server.clone()
        };
        match upsert_json_config(&path, &keys, "rushbross", server) {
            Ok(()) => written.push(path.display().to_string()),
            Err(error) => failures.push(error),
        }
    }
    if let Some(path) = codex_config(home.as_deref()) {
        match upsert_toml_server(&path, &command, &args) {
            Ok(()) => written.push(path.display().to_string()),
            Err(error) => failures.push(error),
        }
    }
    for file in managed_files(folder) {
        match upsert_managed_file(&file) {
            Ok(true) => written.push(file.path.display().to_string()),
            Ok(false) => {}
            Err(error) => failures.push(error),
        }
    }
    if written.is_empty() {
        return Err(failures.join("; "));
    }
    if let Err(error) = write_mcp_manifest(folder, &snapshot) {
        failures.push(error);
    }
    if !failures.is_empty() && written.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(written)
}

fn remove_agent_configs(folder: &std::path::Path) -> Result<Vec<String>, String> {
    let home = user_home();
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    if restore_mcp_manifest(folder, &mut removed, &mut failures) {
        if removed.is_empty() && !failures.is_empty() {
            return Err(failures.join("; "));
        }
        return Ok(removed);
    }
    for (path, keys, _) in json_targets(folder, home.as_deref()) {
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
    for file in managed_files(folder) {
        match remove_managed_file(&file) {
            Ok(true) => removed.push(file.path.display().to_string()),
            Ok(false) => {}
            Err(error) => failures.push(error),
        }
    }
    prune_empty_dirs(folder, &mcp_target_paths(folder, home.as_deref()));
    if removed.is_empty() && !failures.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(removed)
}

fn upsert_toml_server(
    path: &std::path::Path,
    command: &str,
    args: &[String],
) -> Result<(), String> {
    let text = if path.exists() {
        std::fs::read_to_string(path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let updated = rb_agent::toml_upsert_server(
        &text,
        "mcp_servers.rushbross",
        &[
            ("command", rb_agent::TomlValue::Str(command)),
            ("args", rb_agent::TomlValue::StrArray(&args)),
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
            if updated.trim().is_empty() {
                std::fs::remove_file(path).map_err(|error| format!("{}: {error}", path.display()))?;
            } else {
                std::fs::write(path, updated)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
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
    terminal: Option<PtyTerminal>,
    /// Semua pane yang sedang tampil (satu tab biasa, atau hingga tiga pane split).
    visible_webviews: Vec<TabId>,
    /// Bounds WebView terakhir per tab, supaya SetBounds tidak dipanggil tiap frame.
    last_webview_bounds: HashMap<TabId, (i32, i32, i32, i32)>,
    _agent_server: Option<AgentServer>,
    mcp_folder: Option<std::path::PathBuf>,
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
            mcp_folder: None,
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
        let proxy = self.proxy.clone();
        match PtyTerminal::start(&folder, move || {
            let _ = proxy.send_event(UserEvent::TerminalOutput);
        }) {
            Ok(terminal) => {
                self.terminal = Some(terminal);
                self.shell.open_terminal(folder);
                self.shell.set_agent_actions_enabled(true);
                self.shell.show_toast("RushAgent is ready");
                self.window.focus_window();
            }
            Err(error) => self.shell.show_toast(format!("RushAgent failed: {error}")),
        }
        self.redraw();
    }

    fn terminal_exited(&mut self) -> bool {
        self.terminal.as_mut().is_some_and(|terminal| terminal.exited())
    }

    fn disconnect_mcp(&mut self, quiet: bool) {
        let folder = self.mcp_folder.take();
        self.shell.set_agent_connected(false);
        let Some(folder) = folder else {
            return;
        };
        match remove_agent_configs(&folder) {
            Ok(paths) if !quiet => self.shell.show_toast(format!(
                "MCP disconnected ({} configuration files cleaned up)",
                paths.len()
            )),
            Err(error) if !quiet => {
                self.shell
                    .show_toast(format!("Failed to disconnect MCP: {error}"))
            }
            _ => {}
        }
    }

    fn stop_terminal(&mut self) {
        self.disconnect_mcp(true);
        if let Some(mut terminal) = self.terminal.take() {
            terminal.kill();
        }
        self.shell.close_terminal();
        self.shell.set_agent_actions_enabled(false);
        self.select_mode = false;
        self.shell.set_select_mode(false);
    }

    /// Sinkronkan PTY dengan layout pane dan salin layar terbaru ke shell. Dipanggil
    /// setiap frame sebelum frame dibangun.
    fn pump_terminal(&mut self) {
        if self.terminal.as_mut().is_some_and(|terminal| terminal.exited()) {
            self.stop_terminal();
            return;
        }
        if self.terminal.is_some() && !self.shell.terminal_is_open() {
            self.stop_terminal();
            return;
        }
        let Some(layout) = self.shell.terminal_layout(self.inset_content_rect()) else {
            return;
        };
        // Ukuran grid dihitung dari metrik sel yang sama dengan penggambaran (logical px).
        let usable_width = (layout.body.width - TERMINAL_PADDING_X * 2.0).max(0.0);
        let usable_height = (layout.body.height - TERMINAL_PADDING_Y * 2.0).max(0.0);
        let cols = (usable_width / CELL_WIDTH).floor().max(20.0) as u16;
        let rows = (usable_height / LINE_HEIGHT).floor().max(4.0) as u16;
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.resize(cols, rows);
            terminal.pump();
            self.shell.set_terminal_snapshot(terminal.snapshot().clone());
        }
    }

    /// Kirim satu ketikan ke terminal. Mengembalikan false bila event ini bukan input
    /// (misalnya hanya tombol modifier), agar pemanggil bisa meneruskannya ke jalur lain.
    fn send_terminal_key(&mut self, event: &winit::event::KeyEvent, mods: Mods) -> bool {
        let key = match &event.logical_key {
            Key::Named(NamedKey::Space) if mods.ctrl || mods.alt => Some(TermKey::Char(' ')),
            Key::Named(named) => named_to_terminal_key(*named),
            Key::Character(value) if mods.ctrl != mods.alt => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => Some(TermKey::Char(ch)),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(terminal) = self.terminal.as_mut() else {
            return false;
        };
        if let Some(key) = key {
            terminal.key(key, mods);
            return true;
        }
        // Teks biasa (termasuk huruf non-ASCII dan IME) dikirim apa adanya.
        let Some(text) = event.text.as_deref() else {
            return false;
        };
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        terminal.send(text);
        true
    }

    /// Gulir riwayat terminal saat pointer di atas pane. Mengembalikan false bila tidak ada
    /// yang digulir, sehingga roda tetap dipakai oleh halaman atau daftar folder.
    fn terminal_body(&self) -> Option<Rect> {
        self.shell
            .terminal_layout(self.inset_content_rect())
            .map(|layout| layout.body)
    }

    fn copy_terminal_selection(&mut self) {
        let text = self.shell.terminal_selected_text();
        if text.trim().is_empty() {
            return;
        }
        match copy_to_clipboard(&text) {
            Ok(()) => self.shell.show_toast("Selected text copied"),
            Err(error) => self.shell.show_toast(format!("Copy failed: {error}")),
        }
    }

    fn scroll_terminal(&mut self, delta: winit::event::MouseScrollDelta, point: rb_ui::Vec2) -> bool {
        if self.terminal.is_none() || !self.shell.terminal_pane_at(point) {
            return false;
        }
        let lines = (wheel_lines(delta) * 3.0).round() as i32;
        if lines == 0 {
            return false;
        }
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.scroll(lines);
        }
        self.pump_terminal();
        true
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
                        "instructions": "Use the browser_* tools to control RushBross like a real browser: state, navigate, snapshot, click, type, press, scroll, screenshot, tabs, hover, eval, select, and get_selection. The green cursor marks agent actions. browser_get_selection returns the element selected by the user in Select mode."
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
                        &format!("unknown tool: {name}"),
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
                &format!("unknown method: {other}"),
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
                match action {
                    "new" => {
                        self.shell.new_tab();
                        self.redraw();
                        ok(json!({ "ok": true }))
                    }
                    "activate" => {
                        let Some(id) = args.get("id").and_then(Value::as_u64) else {
                            return Some(Err("id is required".to_string()));
                        };
                        if self.shell.activate_tab_by_id(TabId(id)) {
                            self.redraw();
                            ok(json!({ "ok": true }))
                        } else {
                            Some(Err(format!("tab {id} not found")))
                        }
                    }
                    "close" => {
                        self.shell.close_active_tab();
                        self.redraw();
                        ok(json!({ "ok": true }))
                    }
                    _ => {
                        let tabs: Vec<Value> = self
                            .shell
                            .tab_infos()
                            .into_iter()
                            .map(|(id, title, url)| json!({ "id": id.0, "title": title, "url": url }))
                            .collect();
                        ok(json!({ "tabs": tabs }))
                    }
                }
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
                    "Select element on"
                } else {
                    "Select element off"
                });
                self.redraw();
                ok(json!({ "ok": true, "enabled": enabled }))
            }
            "browser_navigate" => {
                let Some(url) = args.get("url").and_then(Value::as_str) else {
                    return Some(Err("url is required".to_string()));
                };
                self.shell.navigate_active(url.to_string());
                self.redraw();
                ok(json!({ "ok": true, "url": url }))
            }
            "browser_state" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                host.eval(
                    r#"({ url: location.href, title: document.title, readyState: document.readyState, viewport: [innerWidth, innerHeight], scroll: [scrollX, scrollY] })"#,
                    tool_channel(reply, id),
                );
                None
            }
            "browser_snapshot" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
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
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
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
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                let Some(text) = args.get("text").and_then(Value::as_str) else {
                    return Some(Err("text is required".to_string()));
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
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                let Some(key) = args.get("key").and_then(Value::as_str) else {
                    return Some(Err("key is required".to_string()));
                };
                host.press_key(key, tool_channel(reply, id));
                None
            }
            "browser_scroll" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let expression = format!("window.scrollBy({x},{y}); true");
                host.eval(&expression, tool_channel(reply, id));
                None
            }
            "browser_screenshot" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                host.cdp(
                    "Page.captureScreenshot",
                    &json!({ "format": "png", "fromSurface": true }),
                    tool_channel(reply, id),
                );
                None
            }
            "browser_hover" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                if let Some(selector) = args.get("selector").and_then(Value::as_str) {
                    let expression = format!(
                        "(function(){{var el=document.querySelector({}); if(!el)return false; var r=el.getBoundingClientRect(); window.__rbAgentCursor.move(r.x+r.width/2,r.y+r.height/2); return true;}})()",
                        js_string(selector)
                    );
                    host.eval(&expression, tool_channel(reply, id));
                } else {
                    let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                    let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                    host.move_cursor(x, y);
                    let _ = tool_channel(reply, id).send(Ok(json!({ "ok": true })));
                }
                None
            }
            "browser_eval" => {
                let Some(tab_id) = self.shell.focused_tab_id() else {
                    return Some(Err("no active tab".to_string()));
                };
                let Some(host) = self.webviews.get(&tab_id) else {
                    return Some(Err("WebView tab is not ready".to_string()));
                };
                let Some(expression) = args.get("expression").and_then(Value::as_str) else {
                    return Some(Err("expression is required".to_string()));
                };
                host.eval(expression, tool_channel(reply, id));
                None
            }
            _ => Some(Err(format!("unknown tool: {name}"))),
        }
    }

    fn connect_agent(&mut self) {
        if self.shell.agent_connected() {
            self.disconnect_mcp(false);
        } else {
            let Some(folder) = self.shell.terminal_folder().map(std::path::PathBuf::from) else {
                self.shell.show_toast("Choose a RushAgent folder first");
                self.redraw();
                return;
            };
            match write_agent_configs(&folder) {
                Ok(paths) => {
                    self.mcp_folder = Some(folder);
                    self.shell.set_agent_connected(true);
                    self.shell.show_toast(format!(
                        "MCP connected in this folder ({} config & skill files)",
                        paths.len()
                    ));
                }
                Err(error) => self.shell.show_toast(format!("MCP failed: {error}")),
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
            "Select element on"
        } else {
            "Select element off"
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
        let sent = match self.terminal.as_mut() {
            Some(terminal) => {
                terminal.send(&text);
                Ok(())
            }
            None => Err("RushAgent terminal is not open".to_string()),
        };
        match sent {
            Ok(()) => self.shell.show_toast("Element sent to agent"),
            Err(error) => self.shell.show_toast(format!("Failed to send element: {error}")),
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
        let agent_ready = self.shell.terminal_folder().is_some();
        if self.shell.take_agent_connect() {
            if agent_ready {
                self.connect_agent();
            } else {
                self.shell.show_toast("Open RushAgent first");
            }
        }
        if self.shell.take_select_element() {
            if agent_ready {
                self.toggle_select();
            } else {
                self.shell.show_toast("Open RushAgent first");
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
        if self.shell.terminal_focused() {
            if let Some(terminal) = self.terminal.as_mut() {
                terminal.send(text);
                self.pump_terminal();
                self.redraw();
                return;
            }
        }
        self.shell.insert_text(text);
    }

    fn handle_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }

        let ctrl = self.modifiers.control_key();
        let alt = self.modifiers.alt_key();
        let shift = self.modifiers.shift_key();

        let copy_shortcut = ctrl
            && shift
            && matches!(&event.logical_key, Key::Character(value) if value.eq_ignore_ascii_case("c"));
        if copy_shortcut && self.shell.terminal_has_selection() {
            self.copy_terminal_selection();
            self.redraw();
            return;
        }

        // Saat terminal fokus, semua ketikan (termasuk Ctrl+huruf dan tombol panah) menjadi
        // input shell. Ctrl+Tab tetap milik aplikasi untuk pindah tab.
        if self.shell.terminal_focused() && self.terminal.is_some() {
            let app_shortcut = ctrl && matches!(&event.logical_key, Key::Named(NamedKey::Tab));
            if !app_shortcut {
                self.shell.clear_terminal_selection();
                let mods = Mods { ctrl, alt, shift };
                if self.send_terminal_key(event, mods) {
                    self.pump_terminal();
                    self.redraw();
                    return;
                }
            }
        }

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
                if state.shell.terminal_is_selecting() {
                    if let Some(body) = state.terminal_body() {
                        state.shell.extend_terminal_selection(body, point);
                    }
                    state.redraw();
                } else if dirty {
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
                if state.scroll_terminal(delta, state.cursor)
                    || state.shell.folder_picker_scroll(wheel_lines(delta))
                {
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
                            if state.terminal.is_some() && state.shell.terminal_pane_at(state.cursor) {
                                if let Some(body) = state.terminal_body() {
                                    state.shell.begin_terminal_selection(body, state.cursor);
                                    state.shell.focus_terminal();
                                    state.window.focus_window();
                                    state.redraw();
                                    return;
                                }
                            }
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
                        ElementState::Released => {
                            if state.shell.terminal_is_selecting() {
                                state.shell.finish_terminal_selection();
                                state.copy_terminal_selection();
                                state.redraw();
                                return;
                            }
                            state.shell.left_released(state.cursor)
                        }
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
                // Terminal digambar di jendela utama; pastikan ketikan masuk ke sana,
                // bukan tertinggal di child window WebView.
                if state.shell.terminal_focused() {
                    state.window.focus_window();
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
            UserEvent::TerminalOutput => {
                state.pump_terminal();
                state.redraw();
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

fn run_mcp_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let (addr, token) = match (
        std::env::var("RUSHBROSS_MCP_ADDR"),
        std::env::var("RUSHBROSS_MCP_TOKEN"),
    ) {
        (Ok(addr), Ok(token)) if !addr.is_empty() && !token.is_empty() => (addr, token),
        _ => {
            let path = std::env::temp_dir().join("rushbross-agent.json");
            let Ok(text) = std::fs::read_to_string(path) else {
                eprintln!("rushbross --mcp: no active RushBross session found");
                std::process::exit(1);
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                eprintln!("rushbross --mcp: RushBross session file is corrupt");
                std::process::exit(1);
            };
            (
                value
                    .get("addr")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value
                    .get("token")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        }
    };
    if addr.is_empty() || token.is_empty() {
        eprintln!("rushbross --mcp: address or token is empty");
        std::process::exit(1);
    }
    if let Err(error) = rb_agent::serve_stdio(&addr, &token) {
        eprintln!("rushbross --mcp: {error}");
        std::process::exit(1);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().any(|arg| arg.to_string_lossy() == "--mcp") {
        return run_mcp_stdio();
    }
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

    #[test]
    fn bridge_command_uses_current_executable_with_mcp_flag() {
        let (command, args) = bridge_command().expect("bridge command");
        assert!(std::path::Path::new(&command).exists());
        assert_eq!(args, vec!["--mcp".to_string()]);
    }

    #[test]
    fn managed_skill_files_are_created_and_fully_removed() {
        let folder = std::env::temp_dir().join(format!("rushbross-mcp-{}", new_session_token()));
        std::fs::create_dir_all(&folder).expect("temp folder");
        let files = managed_files(&folder);
        assert!(files.len() >= 6);
        for file in &files {
            assert!(upsert_managed_file(file).expect("write"));
            let text = std::fs::read_to_string(&file.path).expect("read");
            assert!(text.contains(MCP_START));
            assert!(text.contains("browser_eval"));
        }
        for file in &files {
            assert!(remove_managed_file(file).expect("remove"));
            assert!(!file.path.exists());
        }
        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn managed_section_preserves_existing_instructions() {
        let folder = std::env::temp_dir().join(format!("rushbross-mcp-{}", new_session_token()));
        std::fs::create_dir_all(&folder).expect("temp folder");
        let path = folder.join("AGENTS.md");
        std::fs::write(&path, "user instructions\n").expect("seed");
        let file = ManagedFile {
            path: path.clone(),
            body: MCP_INSTRUCTIONS.to_string(),
            standalone: false,
        };
        assert!(upsert_managed_file(&file).expect("write"));
        assert!(remove_managed_file(&file).expect("remove"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "user instructions\n"
        );
        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn mcp_manifest_restores_existing_files_and_removes_created_ones() {
        let folder = std::env::temp_dir().join(format!("rushbross-mcp-{}", new_session_token()));
        std::fs::create_dir_all(&folder).expect("temp folder");
        let existing = folder.join("AGENTS.md");
        let created = folder.join(".cursor/rules/rushbross.mdc");
        std::fs::write(&existing, "user instructions\n").expect("seed");
        let paths = vec![existing.clone(), created.clone()];
        let snapshot = snapshot_mcp_files(&paths).expect("snapshot");
        write_mcp_manifest(&folder, &snapshot).expect("manifest");
        std::fs::write(&existing, "changed by MCP\n").expect("change");
        std::fs::create_dir_all(created.parent().unwrap()).expect("created dir");
        std::fs::write(&created, "created by MCP\n").expect("created file");
        let mut removed = Vec::new();
        let mut failures = Vec::new();
        assert!(restore_mcp_manifest(&folder, &mut removed, &mut failures));
        assert!(failures.is_empty());
        assert_eq!(
            std::fs::read_to_string(&existing).expect("restored"),
            "user instructions\n"
        );
        assert!(!created.exists());
        assert!(!mcp_manifest_path(&folder).exists());
        assert!(!created.parent().unwrap().exists());
        assert!(!mcp_manifest_path(&folder).parent().unwrap().exists());
        std::fs::remove_dir_all(&folder).expect("cleanup");
    }
}
