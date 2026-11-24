use std::cell::Cell;
use std::sync::mpsc;

use rb_ui::Rect;
use rb_ui_widgets::NEW_TAB_URL;
use webview2_com::{
    take_pwstr,
    Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2Environment, ICoreWebView2Controller, ICoreWebView2Controller2,
        ICoreWebView2_15, COREWEBVIEW2_COLOR, COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    },
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ContainsFullScreenElementChangedEventHandler,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DocumentTitleChangedEventHandler, FaviconChangedEventHandler, GetFaviconCompletedHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::{CoInitializeEx, IStream, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum WebViewEvent {
    /// Navigasi selesai. Host menampilkan WebView setelah ini, supaya tidak ada
    /// kotak putih saat halaman pertama belum siap.
    Loaded,
    /// Favicon PNG untuk halaman yang sedang tampil. `None` menghapus favicon lama.
    Favicon(Option<Vec<u8>>),
    TitleChanged(String),
    UrlChanged(String),
    NewWindowRequested(String),
    FullScreenChanged(bool),
    Accelerator {
        key: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
    },
}

#[derive(Debug)]
pub enum UserEvent {
    WebView(WebViewEvent),
}

struct Tokens {
    title: i64,
    navigation_start: i64,
    navigation_completed: i64,
    new_window: i64,
    fullscreen: i64,
    accelerator: i64,
}

/// Halaman tab baru: beranda lokal yang dimuat langsung ke WebView2.
const NEW_TAB_HTML: &str = include_str!("newtab.html");
/// Warna latar sebelum halaman pertama tampil. Mengganti putih default WebView2.
const PAGE_BACKGROUND: COREWEBVIEW2_COLOR = COREWEBVIEW2_COLOR {
    A: 255,
    R: 0x0a,
    G: 0x0a,
    B: 0x0a,
};
/// Alamat yang dilaporkan WebView2 untuk konten yang dimuat dengan NavigateToString.
const ABOUT_BLANK: &str = "about:blank";

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

fn is_shell_accelerator(key: u32, ctrl: bool, alt: bool) -> bool {
    if ctrl && !alt {
        matches!(key, VK_TAB | VK_A | VK_D | VK_L | VK_Q | VK_R | VK_T | VK_W)
    } else if alt && !ctrl {
        key == VK_LEFT || key == VK_RIGHT
    } else {
        false
    }
}

pub struct WebViewHost {
    controller: ICoreWebView2Controller,
    webview: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    tokens: Tokens,
    showing_new_tab: Cell<bool>,
}

impl WebViewHost {
    pub fn new(parent: HWND, proxy: EventLoopProxy<UserEvent>) -> Result<Self, String> {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

        let environment = {
            let (tx, rx) = mpsc::channel();
            let operation = CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
                Box::new(|handler| unsafe {
                    CreateCoreWebView2Environment(&handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |error_code, environment| {
                    error_code?;
                    let environment =
                        environment.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                    tx.send(environment)
                        .map_err(|_| windows::core::Error::from(E_POINTER))?;
                    Ok(())
                }),
            );
            operation.map_err(|error| error.to_string())?;
            rx.recv()
                .map_err(|_| "WebView2 environment creation was cancelled".to_string())?
        };

        let controller = {
            let (tx, rx) = mpsc::channel();
            let operation = CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| unsafe {
                    environment
                        .CreateCoreWebView2Controller(parent, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |error_code, controller| {
                    error_code?;
                    let controller =
                        controller.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                    tx.send(controller)
                        .map_err(|_| windows::core::Error::from(E_POINTER))?;
                    Ok(())
                }),
            );
            operation.map_err(|error| error.to_string())?;
            rx.recv()
                .map_err(|_| "WebView2 controller creation was cancelled".to_string())?
        };

        unsafe {
            controller
                .SetIsVisible(false)
                .map_err(|error| error.to_string())?;
        }

        // Latar WebView2 default putih. Set ke gelap agar tidak ada kotak putih
        // saat startup atau saat halaman belum selesai dimuat.
        if let Ok(controller2) = controller.cast::<ICoreWebView2Controller2>() {
            unsafe {
                controller2
                    .SetDefaultBackgroundColor(PAGE_BACKGROUND)
                    .map_err(|error| error.to_string())?;
            }
        }

        let webview = unsafe {
            controller
                .CoreWebView2()
                .map_err(|error| error.to_string())?
        };

        unsafe {
            let settings = webview.Settings().map_err(|error| error.to_string())?;
            settings
                .SetAreDefaultContextMenusEnabled(true)
                .map_err(|error| error.to_string())?;
            settings
                .SetAreDevToolsEnabled(false)
                .map_err(|error| error.to_string())?;
        }

        let mut title_token = 0i64;
        let proxy_title = proxy.clone();
        unsafe {
            webview
                .add_DocumentTitleChanged(
                    &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(webview) = sender {
                            let mut title = PWSTR::null();
                            webview.DocumentTitle(&mut title)?;
                            let title = take_pwstr(title);
                            let _ = proxy_title
                                .send_event(UserEvent::WebView(WebViewEvent::TitleChanged(title)));
                        }
                        Ok(())
                    })),
                    &mut title_token,
                )
                .map_err(|error| error.to_string())?;
        }

        let mut navigation_start_token = 0i64;
        let proxy_url = proxy.clone();
        unsafe {
            webview
                .add_NavigationStarting(
                    &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                        let _ =
                            proxy_url.send_event(UserEvent::WebView(WebViewEvent::Favicon(None)));
                        if let Some(args) = args {
                            let mut uri = PWSTR::null();
                            args.Uri(&mut uri)?;
                            let url = take_pwstr(uri);
                            let _ = proxy_url
                                .send_event(UserEvent::WebView(WebViewEvent::UrlChanged(url)));
                        }
                        Ok(())
                    })),
                    &mut navigation_start_token,
                )
                .map_err(|error| error.to_string())?;
        }

        // Favicon API ada di ICoreWebView2_15. Jika runtime WebView2 lebih lama,
        // fitur favicon dilewati dan tab tetap memakai ikon cadangan.
        let mut favicon_token = 0i64;
        let proxy_favicon = proxy.clone();
        if let Ok(webview15) = webview.cast::<ICoreWebView2_15>() {
            unsafe {
                webview15
                    .add_FaviconChanged(
                        &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                            let Some(webview) = sender else {
                                return Ok(());
                            };
                            let Ok(webview15) = webview.cast::<ICoreWebView2_15>() else {
                                return Ok(());
                            };
                            let proxy = proxy_favicon.clone();
                            let completed =
                                GetFaviconCompletedHandler::create(Box::new(move |_, stream| {
                                    let bytes = stream.as_ref().and_then(read_stream);
                                    let _ = proxy.send_event(UserEvent::WebView(
                                        WebViewEvent::Favicon(bytes),
                                    ));
                                    Ok(())
                                }));
                            webview15
                                .GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &completed)?;
                            Ok(())
                        })),
                        &mut favicon_token,
                    )
                    .map_err(|error| error.to_string())?;
            }
        }

        let mut navigation_completed_token = 0i64;
        let proxy_final_url = proxy.clone();
        unsafe {
            webview
                .add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(webview) = sender {
                            let mut uri = PWSTR::null();
                            webview.Source(&mut uri)?;
                            let url = take_pwstr(uri);
                            if !url.is_empty() {
                                let _ = proxy_final_url
                                    .send_event(UserEvent::WebView(WebViewEvent::UrlChanged(url)));
                            }
                        }
                        let _ =
                            proxy_final_url.send_event(UserEvent::WebView(WebViewEvent::Loaded));
                        Ok(())
                    })),
                    &mut navigation_completed_token,
                )
                .map_err(|error| error.to_string())?;
        }

        let mut new_window_token = 0i64;
        let proxy_new_window = proxy.clone();
        unsafe {
            webview
                .add_NewWindowRequested(
                    &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut uri = PWSTR::null();
                            args.Uri(&mut uri)?;
                            let url = take_pwstr(uri);
                            args.SetHandled(true)?;
                            if !url.is_empty() {
                                let _ = proxy_new_window.send_event(UserEvent::WebView(
                                    WebViewEvent::NewWindowRequested(url),
                                ));
                            }
                        }
                        Ok(())
                    })),
                    &mut new_window_token,
                )
                .map_err(|error| error.to_string())?;
        }

        let mut fullscreen_token = 0i64;
        let proxy_fullscreen = proxy.clone();
        unsafe {
            webview
                .add_ContainsFullScreenElementChanged(
                    &ContainsFullScreenElementChangedEventHandler::create(Box::new(
                        move |sender, _| {
                            if let Some(webview) = sender {
                                let mut contains = windows::core::BOOL(0);
                                webview.ContainsFullScreenElement(&mut contains)?;
                                let _ = proxy_fullscreen.send_event(UserEvent::WebView(
                                    WebViewEvent::FullScreenChanged(contains.0 != 0),
                                ));
                            }
                            Ok(())
                        },
                    )),
                    &mut fullscreen_token,
                )
                .map_err(|error| error.to_string())?;
        }

        let mut accelerator_token = 0i64;
        let proxy_accelerator = proxy.clone();
        unsafe {
            controller
                .add_AcceleratorKeyPressed(
                    &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
                        args.KeyEventKind(&mut kind)?;
                        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                            && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                        {
                            return Ok(());
                        }
                        let mut key = 0u32;
                        args.VirtualKey(&mut key)?;
                        let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                        let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                        let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                        if is_shell_accelerator(key, ctrl, alt) {
                            args.SetHandled(true)?;
                            let _ = proxy_accelerator.send_event(UserEvent::WebView(
                                WebViewEvent::Accelerator {
                                    key,
                                    ctrl,
                                    alt,
                                    shift,
                                },
                            ));
                        }
                        Ok(())
                    })),
                    &mut accelerator_token,
                )
                .map_err(|error| error.to_string())?;
        }

        Ok(Self {
            controller,
            webview,
            tokens: Tokens {
                title: title_token,
                navigation_start: navigation_start_token,
                navigation_completed: navigation_completed_token,
                new_window: new_window_token,
                fullscreen: fullscreen_token,
                accelerator: accelerator_token,
            },
            showing_new_tab: Cell::new(false),
        })
    }

    pub fn load(&self, url: &str) -> Result<(), String> {
        if url == NEW_TAB_URL {
            return self.load_new_tab();
        }
        self.showing_new_tab.set(false);
        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            self.webview
                .Navigate(PCWSTR(wide.as_ptr()))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn load_new_tab(&self) -> Result<(), String> {
        let wide: Vec<u16> = NEW_TAB_HTML
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            self.webview
                .NavigateToString(PCWSTR(wide.as_ptr()))
                .map_err(|error| error.to_string())?;
        }
        self.showing_new_tab.set(true);
        Ok(())
    }

    pub fn reload(&self) -> Result<(), String> {
        // Halaman tab baru tidak punya URL asli, jadi reload memuat ulang HTML-nya.
        // Setelah pengguna pindah ke situs lain, sumber tidak lagi about:blank.
        if self.showing_new_tab.get() {
            let source = self.current_source()?;
            if source.starts_with(ABOUT_BLANK) || source.starts_with("data:") {
                return self.load_new_tab();
            }
        }
        unsafe {
            self.webview.Reload().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn current_source(&self) -> Result<String, String> {
        let mut uri = PWSTR::null();
        unsafe {
            self.webview
                .Source(&mut uri)
                .map_err(|error| error.to_string())?;
        }
        Ok(take_pwstr(uri))
    }

    pub fn set_visible(&self, visible: bool) -> Result<(), String> {
        unsafe {
            self.controller
                .SetIsVisible(visible)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        unsafe {
            self.controller
                .SetBounds(RECT {
                    left: rect.x.round() as i32,
                    top: rect.y.round() as i32,
                    right: (rect.x + rect.width).round() as i32,
                    bottom: (rect.y + rect.height).round() as i32,
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

impl Drop for WebViewHost {
    fn drop(&mut self) {
        unsafe {
            let _ = self.webview.remove_DocumentTitleChanged(self.tokens.title);
            let _ = self
                .webview
                .remove_NavigationStarting(self.tokens.navigation_start);
            let _ = self
                .webview
                .remove_NavigationCompleted(self.tokens.navigation_completed);
            let _ = self
                .webview
                .remove_NewWindowRequested(self.tokens.new_window);
            let _ = self
                .webview
                .remove_ContainsFullScreenElementChanged(self.tokens.fullscreen);
            let _ = self
                .controller
                .remove_AcceleratorKeyPressed(self.tokens.accelerator);
            let _ = self.controller.Close();
        }
    }
}

/// Membaca seluruh isi stream favicon (PNG) dari WebView2.
fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut read = 0u32;
        let result = unsafe {
            stream.Read(
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                Some(&mut read as *mut u32),
            )
        };
        if result.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
    }
    (!bytes.is_empty()).then_some(bytes)
}
