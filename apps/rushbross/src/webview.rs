use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use rb_agent::AgentRequest;
use rb_tabs::TabId;
use rb_ui::Rect;
use rb_ui_widgets::NEW_TAB_URL;
use serde_json::{json, Value};
use webview2_com::{
    take_pwstr,
    Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2Environment, ICoreWebView2Controller, ICoreWebView2Controller2,
        ICoreWebView2Environment, ICoreWebView2_15, ICoreWebView2_19, ICoreWebView2_3,
        ICoreWebView2_8, COREWEBVIEW2_COLOR, COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    },
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CallDevToolsProtocolMethodCompletedHandler,
    ContainsFullScreenElementChangedEventHandler, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DocumentTitleChangedEventHandler,
    ExecuteScriptCompletedHandler, FaviconChangedEventHandler, GetFaviconCompletedHandler,
    IsDocumentPlayingAudioChangedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler, TrySuspendCompletedHandler,
    WebMessageReceivedEventHandler,
};
use windows::core::{Interface, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::{CoInitializeEx, IStream, COINIT_APARTMENTTHREADED};
use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum WebViewEvent {
    /// Halaman untuk tab ini selesai dimuat.
    Loaded(TabId),
    /// Favicon PNG untuk tab ini. `None` dikirim saat navigasi mulai.
    Favicon(TabId, Option<Vec<u8>>),
    TitleChanged(TabId, String),
    UrlChanged(TabId, String),
    /// Tab sedang memutar audio; dipakai untuk ikon speaker di tab.
    PlayingAudio(TabId, bool),
    NewWindowRequested(String),
    WebMessage(String),
    FullScreenChanged(TabId, bool),
    Accelerator {
        tab_id: TabId,
        key: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
    },
}

pub enum UserEvent {
    WebView(WebViewEvent),
    Agent(AgentRequest),
}

struct Tokens {
    title: i64,
    navigation_start: i64,
    navigation_completed: i64,
    favicon: i64,
    audio: i64,
    new_window: i64,
    fullscreen: i64,
    accelerator: i64,
    web_message: i64,
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

const AGENT_JS: &str = r##"(function () {
  if (window.__rbAgentReady) {
    window.__rbAgentSetPicker(window.__rbAgentPickerEnabled);
    return;
  }
  window.__rbAgentReady = true;
  window.__rbAgentPickerEnabled = false;
  function esc(value) {
    if (window.CSS && CSS.escape) return CSS.escape(value);
    return String(value).replace(/[^a-zA-Z0-9_-]/g, "\\$&");
  }
  var style = document.createElement("style");
  style.id = "rb-agent-style";
  style.textContent =
    "#rb-agent-cursor{position:fixed;left:0;top:0;width:24px;height:24px;pointer-events:none;z-index:2147483647;opacity:0;transition:transform .12s ease-out,opacity .15s;will-change:transform}" +
    "#rb-agent-cursor.rb-visible{opacity:1}" +
    "#rb-agent-cursor svg{display:block;width:22px;height:22px;filter:drop-shadow(0 1px 2px rgba(0,0,0,.6))}" +
    "#rb-agent-cursor .rb-label{position:absolute;left:18px;top:18px;padding:2px 5px;border-radius:4px;background:#22c55e;color:#04140b;font:600 10px/1.2 sans-serif;white-space:nowrap}" +
    "#rb-agent-cursor.rb-typing .rb-label{background:#16a34a;color:white}" +
    "#rb-agent-highlight{position:fixed;pointer-events:none;z-index:2147483646;display:none;border:2px solid #22c55e;background:rgba(34,197,94,.12);border-radius:3px;box-shadow:0 0 0 1px rgba(0,0,0,.4)}" +
    "#rb-agent-ripple{position:fixed;width:14px;height:14px;margin:-7px 0 0 -7px;border:2px solid #22c55e;border-radius:50%;pointer-events:none;z-index:2147483647;opacity:0;transition:transform .35s ease-out,opacity .35s ease-out}";
  (document.head || document.documentElement).appendChild(style);
  var cursor = document.createElement("div");
  cursor.id = "rb-agent-cursor";
  cursor.innerHTML = '<svg viewBox="0 0 24 24" fill="#22c55e"><path d="M4 2l15 10-7 1.2L9.4 20 4 2z"/></svg><span class="rb-label">RushAgent</span>';
  (document.body || document.documentElement).appendChild(cursor);
  var highlight = document.createElement("div");
  highlight.id = "rb-agent-highlight";
  (document.body || document.documentElement).appendChild(highlight);
  var ripple = document.createElement("div");
  ripple.id = "rb-agent-ripple";
  (document.body || document.documentElement).appendChild(ripple);
  window.__rbAgentCursor = {
    move: function (x, y) {
      cursor.style.transform = "translate3d(" + x + "px," + y + "px,0)";
      cursor.classList.add("rb-visible");
    },
    click: function (x, y) {
      this.move(x, y);
      ripple.style.transform = "translate3d(" + x + "px," + y + "px,0) scale(2.4)";
      ripple.style.opacity = "0.9";
      window.setTimeout(function () { ripple.style.opacity = "0"; }, 60);
    },
    type: function () {
      cursor.classList.add("rb-typing");
      window.setTimeout(function () { cursor.classList.remove("rb-typing"); }, 700);
    }
  };
  function selectorOf(node) {
    var parts = [];
    while (node && node.nodeType === 1 && parts.length < 8) {
      var tag = node.tagName.toLowerCase();
      if (node.id) {
        parts.unshift("#" + esc(node.id));
        break;
      }
      var classes = "";
      if (typeof node.className === "string") {
        classes = node.className.trim().split(/\s+/).filter(Boolean).slice(0, 2).map(function (name) { return "." + esc(name); }).join("");
      }
      var parent = node.parentElement;
      var suffix = "";
      if (parent) {
        var siblings = Array.prototype.filter.call(parent.children, function (child) { return child.tagName === node.tagName; });
        if (siblings.length > 1) suffix = ":nth-of-type(" + (siblings.indexOf(node) + 1) + ")";
      }
      parts.unshift(tag + classes + suffix);
      node = parent;
    }
    return parts.join(" > ");
  }
  function describe(node) {
    var rect = node.getBoundingClientRect();
    return {
      type: "rb-element-selected",
      selector: selectorOf(node),
      tag: node.tagName.toLowerCase(),
      text: (node.innerText || node.textContent || "").trim().slice(0, 300),
      html: (node.outerHTML || "").slice(0, 2000),
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      url: location.href,
      title: document.title
    };
  }
  function onMove(event) {
    if (!window.__rbAgentPickerEnabled) return;
    var node = document.elementFromPoint(event.clientX, event.clientY);
    if (!node || node === highlight || node === cursor) return;
    var rect = node.getBoundingClientRect();
    highlight.style.display = "block";
    highlight.style.transform = "translate3d(" + rect.x + "px," + rect.y + "px,0)";
    highlight.style.width = rect.width + "px";
    highlight.style.height = rect.height + "px";
    window.__rbAgentCursor.move(event.clientX, event.clientY);
  }
  function onClick(event) {
    if (!window.__rbAgentPickerEnabled) return;
    var node = event.target;
    if (!node || node.nodeType !== 1) return;
    event.preventDefault();
    event.stopPropagation();
    window.__rbAgentSetPicker(false);
    if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
      window.chrome.webview.postMessage(describe(node));
    }
  }
  window.__rbAgentSetPicker = function (enabled) {
    window.__rbAgentPickerEnabled = !!enabled;
    if (!window.__rbAgentPickerEnabled) highlight.style.display = "none";
  };
  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("click", onClick, true);
})();"##;

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

/// Environment WebView2 dibuat sekali dan dipakai bersama semua tab. Membuat
/// environment per tab memboroskan memori dan proses browser tambahan.
pub struct WebViewRuntime {
    environment: ICoreWebView2Environment,
    active_tab: Arc<AtomicU64>,
}

impl WebViewRuntime {
    pub fn new() -> Result<Self, String> {
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

        Ok(Self {
            environment,
            active_tab: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn set_active_tab(&self, id: TabId) {
        self.active_tab.store(id.0, Ordering::Relaxed);
    }

    pub fn create_host(
        &self,
        parent: HWND,
        tab_id: TabId,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<WebViewHost, String> {
        let controller = {
            let (tx, rx) = mpsc::channel();
            let environment = self.environment.clone();
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
                            let _ = proxy_title.send_event(UserEvent::WebView(
                                WebViewEvent::TitleChanged(tab_id, title),
                            ));
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
                        let _ = proxy_url
                            .send_event(UserEvent::WebView(WebViewEvent::Favicon(tab_id, None)));
                        if let Some(args) = args {
                            let mut uri = PWSTR::null();
                            args.Uri(&mut uri)?;
                            let url = take_pwstr(uri);
                            let _ = proxy_url.send_event(UserEvent::WebView(
                                WebViewEvent::UrlChanged(tab_id, url),
                            ));
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
                                        WebViewEvent::Favicon(tab_id, bytes),
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
                                let _ = proxy_final_url.send_event(UserEvent::WebView(
                                    WebViewEvent::UrlChanged(tab_id, url),
                                ));
                            }
                            if let Ok(webview8) = webview.cast::<ICoreWebView2_8>() {
                                let mut playing = windows::core::BOOL(0);
                                if webview8.IsDocumentPlayingAudio(&mut playing).is_ok() {
                                    let _ = proxy_final_url.send_event(UserEvent::WebView(
                                        WebViewEvent::PlayingAudio(tab_id, playing.0 != 0),
                                    ));
                                }
                            }
                        }
                        let _ = proxy_final_url
                            .send_event(UserEvent::WebView(WebViewEvent::Loaded(tab_id)));
                        Ok(())
                    })),
                    &mut navigation_completed_token,
                )
                .map_err(|error| error.to_string())?;
        }

        // Ikon speaker muncul saat halaman memutar audio, termasuk di tab background.
        let mut audio_token = 0i64;
        let proxy_audio = proxy.clone();
        if let Ok(webview8) = webview.cast::<ICoreWebView2_8>() {
            unsafe {
                webview8
                    .add_IsDocumentPlayingAudioChanged(
                        &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(
                            move |sender, _| {
                                if let Some(webview) = sender {
                                    if let Ok(webview8) = webview.cast::<ICoreWebView2_8>() {
                                        let mut playing = windows::core::BOOL(0);
                                        webview8.IsDocumentPlayingAudio(&mut playing)?;
                                        let _ = proxy_audio.send_event(UserEvent::WebView(
                                            WebViewEvent::PlayingAudio(tab_id, playing.0 != 0),
                                        ));
                                    }
                                }
                                Ok(())
                            },
                        )),
                        &mut audio_token,
                    )
                    .map_err(|error| error.to_string())?;
            }
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
                                    WebViewEvent::FullScreenChanged(tab_id, contains.0 != 0),
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
        let active_tab = Arc::clone(&self.active_tab);
        unsafe {
            controller
                .add_AcceleratorKeyPressed(
                    &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                        if active_tab.load(Ordering::Relaxed) != tab_id.0 {
                            return Ok(());
                        }
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
                                    tab_id,
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

        let mut web_message_token = 0i64;
        let proxy_message = proxy.clone();
        unsafe {
            webview
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut message = PWSTR::null();
                        args.WebMessageAsJson(&mut message)?;
                        let _ = proxy_message.send_event(UserEvent::WebView(
                            WebViewEvent::WebMessage(take_pwstr(message)),
                        ));
                        Ok(())
                    })),
                    &mut web_message_token,
                )
                .map_err(|error| error.to_string())?;
        }

        Ok(WebViewHost {
            controller,
            webview,
            tokens: Tokens {
                title: title_token,
                navigation_start: navigation_start_token,
                navigation_completed: navigation_completed_token,
                favicon: favicon_token,
                audio: audio_token,
                new_window: new_window_token,
                fullscreen: fullscreen_token,
                accelerator: accelerator_token,
                web_message: web_message_token,
            },
            showing_new_tab: Cell::new(false),
        })
    }
}

pub struct WebViewHost {
    controller: ICoreWebView2Controller,
    webview: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    tokens: Tokens,
    showing_new_tab: Cell<bool>,
}

impl WebViewHost {
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

    pub fn install_agent_helpers(&self) {
        let script: Vec<u16> = AGENT_JS.encode_utf16().chain(Some(0)).collect();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
        unsafe {
            let _ = self
                .webview
                .ExecuteScript(PCWSTR(script.as_ptr()), &handler);
        }
    }

    pub fn move_cursor(&self, x: f64, y: f64) {
        let script = format!("window.__rbAgentCursor.move({x},{y})");
        let wide: Vec<u16> = script.encode_utf16().chain(Some(0)).collect();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
        unsafe {
            let _ = self.webview.ExecuteScript(PCWSTR(wide.as_ptr()), &handler);
        }
    }

    pub fn set_picker(&self, enabled: bool) {
        let script = format!("window.__rbAgentSetPicker({})", enabled);
        let wide: Vec<u16> = script.encode_utf16().chain(Some(0)).collect();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
        unsafe {
            let _ = self.webview.ExecuteScript(PCWSTR(wide.as_ptr()), &handler);
        }
    }

    pub fn eval(&self, expression: &str, reply: mpsc::Sender<Result<Value, String>>) {
        let script = format!("JSON.stringify((function(){{ return ({expression}); }})())");
        let wide: Vec<u16> = script.encode_utf16().chain(Some(0)).collect();
        let error_reply = reply.clone();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |error_code, result| {
            if error_code.is_ok() {
                let value = serde_json::from_str(&result).unwrap_or(Value::Null);
                let _ = reply.send(Ok(value));
            } else {
                let _ = reply.send(Err(error_code.err().map(|error| error.to_string()).unwrap_or_default()));
            }
            Ok(())
        }));
        if let Err(error) = unsafe {
            self.webview
                .ExecuteScript(PCWSTR(wide.as_ptr()), &handler)
                .map_err(|error| error.to_string())
        } {
            let _ = error_reply.send(Err(error));
        }
    }

    pub fn cdp(
        &self,
        method: &str,
        params: &Value,
        reply: mpsc::Sender<Result<Value, String>>,
    ) {
        let method_wide: Vec<u16> = method.encode_utf16().chain(Some(0)).collect();
        let params_wide: Vec<u16> = params
            .to_string()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let error_reply = reply.clone();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |error_code, result| {
                if error_code.is_ok() {
                    let value = serde_json::from_str(&result).unwrap_or(Value::Null);
                    let _ = reply.send(Ok(value));
                } else {
                    let _ = reply.send(Err(error_code.err().map(|error| error.to_string()).unwrap_or_default()));
                }
                Ok(())
            },
        ));
        if let Err(error) = unsafe {
            self.webview.CallDevToolsProtocolMethod(
                PCWSTR(method_wide.as_ptr()),
                PCWSTR(params_wide.as_ptr()),
                &handler,
            )
        } {
            let _ = error_reply.send(Err(error.to_string()));
        }
    }

    fn cdp_pair(
        &self,
        method: &str,
        first: Value,
        second: Value,
        reply: mpsc::Sender<Result<Value, String>>,
    ) {
        let webview = self.webview.clone();
        let method_wide: Vec<u16> = method.encode_utf16().chain(Some(0)).collect();
        let second_params = second.to_string();
        let error_reply = reply.clone();
        let first_error_reply = error_reply.clone();
        let second_handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |error_code, result| {
                if error_code.is_ok() {
                    let value = serde_json::from_str(&result).unwrap_or(Value::Null);
                    let _ = reply.send(Ok(value));
                } else {
                    let _ = reply.send(Err(error_code.err().map(|error| error.to_string()).unwrap_or_default()));
                }
                Ok(())
            },
        ));
        let first_handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |error_code, result| {
                if error_code.is_err() {
                    let _ = first_error_reply.send(Err(error_code.err().map(|error| error.to_string()).unwrap_or_default()));
                    return Ok(());
                }
                let method: Vec<u16> = method_wide.clone();
                let params: Vec<u16> = second_params
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                unsafe {
                    let _ = webview.CallDevToolsProtocolMethod(
                        PCWSTR(method.as_ptr()),
                        PCWSTR(params.as_ptr()),
                        &second_handler,
                    );
                }
                let _ = result;
                Ok(())
            },
        ));
        let first_params: Vec<u16> = first
            .to_string()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let method_wide: Vec<u16> = method.encode_utf16().chain(Some(0)).collect();
        if let Err(error) = unsafe {
            self.webview.CallDevToolsProtocolMethod(
                PCWSTR(method_wide.as_ptr()),
                PCWSTR(first_params.as_ptr()),
                &first_handler,
            )
        } {
            let _ = error_reply.send(Err(error.to_string()));
        }
    }

    pub fn click_at(&self, x: f64, y: f64, reply: mpsc::Sender<Result<Value, String>>) {
        let pressed = json!({
            "type": "mousePressed",
            "x": x,
            "y": y,
            "button": "left",
            "clickCount": 1
        });
        let released = json!({
            "type": "mouseReleased",
            "x": x,
            "y": y,
            "button": "left",
            "clickCount": 1
        });
        self.cdp_pair("Input.dispatchMouseEvent", pressed, released, reply);
    }

    pub fn insert_text(&self, text: &str, reply: mpsc::Sender<Result<Value, String>>) {
        self.cdp("Input.insertText", &json!({ "text": text }), reply);
    }

    pub fn press_key(&self, key: &str, reply: mpsc::Sender<Result<Value, String>>) {
        let (code, text, name) = match key.to_ascii_lowercase().as_str() {
            "enter" | "return" => (13u32, "\r", "Enter"),
            "tab" => (9, "\t", "Tab"),
            "escape" | "esc" => (27, "", "Escape"),
            "backspace" => (8, "", "Backspace"),
            "delete" => (46, "", "Delete"),
            "space" => (32, " ", " "),
            _ => (0, key, key),
        };
        let down = json!({
            "type": "keyDown",
            "key": name,
            "code": name,
            "windowsVirtualKeyCode": code,
            "nativeVirtualKeyCode": code,
            "text": text,
            "unmodifiedText": text
        });
        let up = json!({
            "type": "keyUp",
            "key": name,
            "code": name,
            "windowsVirtualKeyCode": code,
            "nativeVirtualKeyCode": code
        });
        self.cdp_pair("Input.dispatchKeyEvent", down, up, reply);
    }

    pub fn set_visible(&self, visible: bool) -> Result<(), String> {
        unsafe {
            self.controller
                .SetIsVisible(visible)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Menurunkan pemakaian memori WebView tab yang tidak tampil. Target memori
    /// dibuat Low dan halaman di-suspend, kecuali tab yang sedang memutar audio
    /// supaya musik/video tidak berhenti. Saat tab tampil lagi, keduanya dikembalikan.
    pub fn set_background(&self, background: bool) -> Result<(), String> {
        unsafe {
            if let Ok(webview19) = self.webview.cast::<ICoreWebView2_19>() {
                let level = if background {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                } else {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                };
                webview19
                    .SetMemoryUsageTargetLevel(level)
                    .map_err(|error| error.to_string())?;
            }
            if let Ok(webview3) = self.webview.cast::<ICoreWebView2_3>() {
                if background {
                    if self.is_playing_audio() {
                        return Ok(());
                    }
                    let handler = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
                    webview3
                        .TrySuspend(&handler)
                        .map_err(|error| error.to_string())?;
                } else {
                    webview3.Resume().map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }

    fn is_playing_audio(&self) -> bool {
        let Ok(webview8) = self.webview.cast::<ICoreWebView2_8>() else {
            return false;
        };
        let mut playing = BOOL(0);
        unsafe { webview8.IsDocumentPlayingAudio(&mut playing) }.is_ok() && playing.0 != 0
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
            if let Ok(webview15) = self.webview.cast::<ICoreWebView2_15>() {
                let _ = webview15.remove_FaviconChanged(self.tokens.favicon);
            }
            if let Ok(webview8) = self.webview.cast::<ICoreWebView2_8>() {
                let _ = webview8.remove_IsDocumentPlayingAudioChanged(self.tokens.audio);
            }
            let _ = self
                .webview
                .remove_NewWindowRequested(self.tokens.new_window);
            let _ = self
                .webview
                .remove_ContainsFullScreenElementChanged(self.tokens.fullscreen);
            let _ = self
                .webview
                .remove_WebMessageReceived(self.tokens.web_message);
            let _ = self
                .controller
                .remove_AcceleratorKeyPressed(self.tokens.accelerator);
            let _ = self.controller.Close();
        }
    }
}

/// Mengosongkan working set proses shell. Halaman yang jarang dipakai lalu dipindah
/// ke file paging oleh Windows dan dimuat lagi hanya saat dibutuhkan, sehingga angka
/// RAM di Task Manager turun saat aplikasi diminimize.
pub fn trim_working_set() {
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
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
