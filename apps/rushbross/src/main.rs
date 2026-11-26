#![windows_subsystem = "windows"]

mod webview;

mod app_icon;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rb_tabs::TabId;
use rb_ui::{Frame, Rect, RectShape};
use rb_ui_renderer::{RenderError, Renderer};
use rb_ui_widgets::{BrowserShell, ChromeAction, ChromeElement, CursorRole};
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
use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{
    CreateProcessW, WaitForSingleObject, CREATE_NEW_CONSOLE, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{
    IFileOpenDialog, IShellItem, FileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, PostMessageW, SetParent,
    SetWindowLongPtrW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, WM_CLOSE,
    WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
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

struct TerminalHost {
    hwnd: HWND,
    process: HANDLE,
    thread: HANDLE,
    last_bounds: Option<(i32, i32, i32, i32)>,
}

struct ConsoleWindowSearch<'a> {
    token: &'a str,
    found: Option<HWND>,
}

unsafe extern "system" fn enum_console_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lparam.0 as *mut ConsoleWindowSearch<'_>) };
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return BOOL(1);
    }
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe { GetWindowTextW(hwnd, &mut buffer) };
    let title = String::from_utf16_lossy(&buffer[..length as usize]);
    if title.contains(search.token) {
        search.found = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
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
    terminal: Option<TerminalHost>,
    /// Semua pane yang sedang tampil (satu tab biasa, atau hingga tiga pane split).
    visible_webviews: Vec<TabId>,
    /// Bounds WebView terakhir per tab, supaya SetBounds tidak dipanggil tiap frame.
    last_webview_bounds: HashMap<TabId, (i32, i32, i32, i32)>,
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
        let Ok(hwnd) = self.parent_hwnd() else {
            return;
        };
        let Some(folder) = pick_folder(hwnd) else {
            return;
        };
        self.stop_terminal();
        match start_terminal(&folder, hwnd) {
            Ok(terminal) => {
                self.terminal = Some(terminal);
                self.shell.open_terminal(folder);
            }
            Err(error) => eprintln!("start RushAgent terminal: {error}"),
        }
        self.redraw();
    }

    fn terminal_bounds(&self) -> Option<Rect> {
        let body = self
            .shell
            .terminal_layout(self.inset_content_rect())?
            .body;
        let scale = self.window.scale_factor() as f32;
        Some(Rect::new(
            body.x * scale,
            body.y * scale,
            body.width * scale,
            body.height * scale,
        ))
    }

    fn terminal_exited(&self) -> bool {
        let Some(terminal) = self.terminal.as_ref() else {
            return false;
        };
        unsafe { WaitForSingleObject(terminal.process, 0) == WAIT_OBJECT_0 }
    }

    fn stop_terminal(&mut self) {
        if let Some(terminal) = self.terminal.take() {
            unsafe {
                let _ = PostMessageW(Some(terminal.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                let _ = CloseHandle(terminal.thread);
                let _ = CloseHandle(terminal.process);
            }
        }
        self.shell.close_terminal();
    }

    fn sync_terminal(&mut self) {
        if self.terminal.is_some() && self.terminal_exited() {
            self.stop_terminal();
            return;
        }
        if self.terminal.is_some() && !self.shell.terminal_is_open() {
            self.stop_terminal();
            return;
        }
        let Some(bounds) = self.terminal_bounds() else {
            return;
        };
        let Some(terminal) = self.terminal.as_mut() else {
            return;
        };
        let key = (
            bounds.x.round() as i32,
            bounds.y.round() as i32,
            bounds.width.round() as i32,
            bounds.height.round() as i32,
        );
        if terminal.last_bounds == Some(key) {
            return;
        }
        unsafe {
            let _ = SetWindowPos(
                terminal.hwnd,
                None,
                key.0,
                key.1,
                key.2,
                key.3,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        terminal.last_bounds = Some(key);
    }

    fn draw(&mut self) {
        let (width, height) = self.size;
        if width <= 1.0 || height <= 1.0 {
            return;
        }
        if self.terminal.is_some() && self.terminal_exited() {
            self.stop_terminal();
        }
        self.shell.set_maximized(self.window.is_maximized());
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
        self.sync_terminal();
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
                        self.shell.insert_text(text);
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
                state.shell.insert_text(&text);
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
        }
        state.pump_navigation();
        state.redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            if state.terminal.is_some() && state.terminal_exited() {
                state.stop_terminal();
                state.redraw();
            }
            if state.shell.has_animation() {
                state.redraw();
            }
        }
    }
}

fn pick_folder(parent: HWND) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        dialog
            .SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog.SetTitle(w!("Pilih folder untuk RushAgent")).ok()?;
        dialog.Show(Some(parent)).ok()?;
        let item: IShellItem = dialog.GetResult().ok()?;
        let path: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let folder = path.to_string().ok()?;
        CoTaskMemFree(Some(path.0 as *const c_void));
        Some(folder)
    }
}

fn start_terminal(folder: &str, parent: HWND) -> Result<TerminalHost, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let token = format!("RushAgent-{}-{}", std::process::id(), nonce);
    let mut command: Vec<u16> = format!("cmd.exe /K title {token} && cd /d \"{folder}\"")
        .encode_utf16()
        .collect();
    command.push(0);
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NEW_CONSOLE,
            None,
            PCWSTR::null(),
            &startup,
            &mut info,
        )
        .map_err(|error| error.to_string())?;
    }
    let mut hwnd = None;
    for _ in 0..40 {
        hwnd = find_console_window(&token);
        if hwnd.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let Some(hwnd) = hwnd else {
        unsafe {
            let _ = CloseHandle(info.hThread);
            let _ = CloseHandle(info.hProcess);
        }
        return Err("jendela console tidak ditemukan".into());
    };
    unsafe {
        let _ = SetParent(hwnd, Some(parent));
        let old_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let remove = (WS_POPUP
            | WS_CAPTION
            | WS_THICKFRAME
            | WS_SYSMENU
            | WS_MAXIMIZEBOX
            | WS_MINIMIZEBOX)
            .0 as isize;
        let add = (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN).0 as isize;
        let _ = SetWindowLongPtrW(hwnd, GWL_STYLE, (old_style & !remove) | add);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            1,
            1,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(TerminalHost {
        hwnd,
        process: info.hProcess,
        thread: info.hThread,
        last_bounds: None,
    })
}

fn find_console_window(token: &str) -> Option<HWND> {
    let mut search = ConsoleWindowSearch {
        token,
        found: None,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_console_window),
            LPARAM(&mut search as *mut ConsoleWindowSearch<'_> as isize),
        );
    }
    search.found
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
