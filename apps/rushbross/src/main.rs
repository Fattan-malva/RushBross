#![windows_subsystem = "windows"]

mod webview;

use std::sync::Arc;
use std::time::{Duration, Instant};

use rb_ui::Rect;
use rb_ui_renderer::{RenderError, Renderer};
use rb_ui_widgets::{BrowserShell, ChromeAction, ChromeElement};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{CursorIcon, Icon, ResizeDirection, Window, WindowId};

use webview::{UserEvent, WebViewHost};

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
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(400);
const DOUBLE_CLICK_DISTANCE: f32 = 8.0;

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    shell: BrowserShell,
    webview: WebViewHost,
    cursor: rb_ui::Vec2,
    size: (f32, f32),
    modifiers: ModifiersState,
    cursor_icon: CursorIcon,
    last_drag_click: Option<(Instant, rb_ui::Vec2)>,
}

impl State {
    fn new(
        window: Arc<Window>,
        renderer: Renderer,
        proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    ) -> Result<Self, String> {
        let scale = window.scale_factor() as f32;
        let physical = window.inner_size();
        let size = (
            physical.width as f32 / scale,
            physical.height as f32 / scale,
        );
        let shell = BrowserShell::new();
        let hwnd = match window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                RawWindowHandle::Win32(handle) => {
                    windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut std::ffi::c_void)
                }
                _ => return Err("window is not a Win32 window".to_string()),
            },
            Err(error) => return Err(error.to_string()),
        };
        let webview = WebViewHost::new(hwnd, proxy)?;
        Ok(Self {
            window,
            renderer,
            shell,
            webview,
            cursor: rb_ui::Vec2::new(-1000.0, -1000.0),
            size,
            modifiers: ModifiersState::empty(),
            cursor_icon: CursorIcon::Default,
            last_drag_click: None,
        })
    }

    fn draw(&mut self) {
        let (width, height) = self.size;
        if width <= 1.0 || height <= 1.0 {
            return;
        }
        self.shell.set_maximized(self.window.is_maximized());
        let frame = self.shell.render(width, height);
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
        self.sync_webview_bounds();
    }

    fn sync_webview_bounds(&self) {
        let (width, height) = self.size;
        let rect = self.shell.content_rect(width, height);
        let scale = self.window.scale_factor() as f32;
        let physical = Rect::new(
            (rect.x + RESIZE_EDGE) * scale,
            rect.y * scale,
            (rect.width - RESIZE_EDGE * 2.0).max(0.0) * scale,
            (rect.height - RESIZE_EDGE).max(0.0) * scale,
        );
        if let Err(error) = self.webview.set_bounds(physical) {
            eprintln!("webview bounds: {error}");
        }
    }

    fn pump_navigation(&mut self) {
        while let Some(request) = self.shell.take_navigation() {
            if let Err(error) = self.webview.load(&request.url) {
                eprintln!("webview navigate {}: {error}", request.url);
            }
        }
    }

    fn redraw(&self) {
        self.window.request_redraw();
    }

    fn apply_chrome_action(&mut self, action: ChromeAction, event_loop: &ActiveEventLoop) {
        match action {
            ChromeAction::Minimize => self.window.set_minimized(true),
            ChromeAction::MaximizeToggle => {
                self.window.set_maximized(!self.window.is_maximized());
            }
            ChromeAction::Close => event_loop.exit(),
        }
    }

    fn update_cursor(&mut self) {
        let chrome = self.shell.hit_chrome(self.cursor);
        let direction = resize_direction(self.cursor, self.size);
        let cursor = if let Some(direction) = direction {
            resize_cursor(direction)
        } else {
            match chrome {
                Some(ChromeElement::DragStrip) => CursorIcon::Grab,
                Some(_) => CursorIcon::Pointer,
                None => CursorIcon::Default,
            }
        };
        if cursor != self.cursor_icon {
            self.window.set_cursor(cursor);
            self.cursor_icon = cursor;
        }
    }

    fn start_drag(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let double_click = self.last_drag_click.is_some_and(|(time, position)| {
            now.duration_since(time) <= DOUBLE_CLICK_TIME
                && (position.x - self.cursor.x).hypot(position.y - self.cursor.y)
                    <= DOUBLE_CLICK_DISTANCE
        });
        if double_click {
            self.last_drag_click = None;
            self.apply_chrome_action(ChromeAction::MaximizeToggle, event_loop);
            return;
        }
        self.last_drag_click = Some((now, self.cursor));
        if let Err(error) = self.window.drag_window() {
            eprintln!("drag window: {error}");
        }
    }

    fn handle_accelerator(
        &mut self,
        key: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        event_loop: &ActiveEventLoop,
    ) {
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
                    if let Err(error) = self.webview.reload() {
                        eprintln!("webview reload: {error}");
                    }
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
            Key::Named(NamedKey::Escape) => self.shell.escape(),
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
                "r" => {
                    if let Err(error) = self.webview.reload() {
                        eprintln!("webview reload: {error}");
                    }
                }
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
            .with_decorations(false);
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
        let mut state = match State::new(window, renderer, proxy) {
            Ok(state) => state,
            Err(error) => {
                eprintln!("create webview: {error}");
                event_loop.exit();
                return;
            }
        };
        state.pump_navigation();
        state.redraw();
        self.state = Some(state);
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
        if window_id != state.window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                let scale = state.window.scale_factor() as f32;
                state.size = (size.width as f32 / scale, size.height as f32 / scale);
                state.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state.renderer.set_scale_factor(scale_factor as f32);
                let physical = state.window.inner_size();
                state.renderer.resize(physical.width, physical.height);
                state.size = (
                    physical.width as f32 / scale_factor as f32,
                    physical.height as f32 / scale_factor as f32,
                );
                state.redraw();
            }
            WindowEvent::RedrawRequested => state.draw(),
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = state.window.scale_factor() as f32;
                let point = rb_ui::Vec2::new(position.x as f32 / scale, position.y as f32 / scale);
                state.cursor = point;
                state.shell.pointer_moved(point);
                state.update_cursor();
                state.redraw();
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
                            if let Some(direction) = resize_direction(state.cursor, state.size) {
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
                            state.shell.left_click(state.cursor);
                            if let Some(action) = state.shell.take_chrome_action() {
                                state.apply_chrome_action(action, event_loop);
                                return;
                            }
                        }
                        ElementState::Released => state.shell.left_released(state.cursor),
                    },
                    MouseButton::Right if pressed == ElementState::Pressed => {
                        if state.shell.hit_chrome(state.cursor) == Some(ChromeElement::DragStrip) {
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
            UserEvent::WebView(webview::WebViewEvent::TitleChanged(title)) => {
                state.shell.webview_title_changed(&title)
            }
            UserEvent::WebView(webview::WebViewEvent::UrlChanged(url)) => {
                state.shell.webview_url_changed(&url)
            }
            UserEvent::WebView(webview::WebViewEvent::NewWindowRequested(url)) => {
                state.shell.webview_new_window(&url)
            }
            UserEvent::WebView(webview::WebViewEvent::Accelerator {
                key,
                ctrl,
                alt,
                shift,
            }) => {
                state.handle_accelerator(key, ctrl, alt, shift, event_loop);
                return;
            }
        }
        state.pump_navigation();
        state.redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            if state.shell.animate_tabs() {
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
        png::ColorType::Rgb => {
            let (pixels, _) = bytes.as_chunks::<3>();
            pixels
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect()
        }
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Grayscale => bytes
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => {
            let (pixels, _) = bytes.as_chunks::<2>();
            pixels
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect()
        }
        png::ColorType::Indexed => return None,
    };

    let radius = (width.min(height) as f32) * 0.22;
    let half_width = width as f32 * 0.5;
    let half_height = height as f32 * 0.5;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = (y * width as usize + x) * 4;
            let luminance = 0.2126 * rgba[index] as f32
                + 0.7152 * rgba[index + 1] as f32
                + 0.0722 * rgba[index + 2] as f32;
            let alpha = ((luminance - 24.0) * 255.0 / 210.0).clamp(0.0, 255.0) as u8;
            let dx = ((x as f32 + 0.5) - half_width).abs() - (half_width - radius);
            let dy = ((y as f32 + 0.5) - half_height).abs() - (half_height - radius);
            let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.min(dy).min(0.0) - radius;
            let corner_alpha = if distance > 1.0 {
                0
            } else if distance > 0.0 {
                (255.0 * (1.0 - distance)).round() as u8
            } else {
                255
            };
            rgba[index] = 255;
            rgba[index + 1] = 255;
            rgba[index + 2] = 255;
            rgba[index + 3] = alpha.min(corner_alpha);
        }
    }

    Some((rgba, width, height))
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
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] > 128));
    }
}
