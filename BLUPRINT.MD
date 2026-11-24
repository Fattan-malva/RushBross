# **RushBross Browser**

Rust · Native UI · WebView2 Page Engine · Ultra Lightweight

[How to turn Firefox into a usable web browser](https://images.openai.com/static-rsc-4/65wpSEOrS5yZfH7ykKykuX_thnjxM8yewACw1lylzmhHyrC3riaPVCqTU-zYI1_THdjgQjCFSNh_nH8HtUDzrT3F1Kyea8ozs9C3LWGFHurJ3G52mG-4Q1RoJ_OIK0uDcEAyFQ7bIrZ7m5weSCM1ARyNKX9oCsM8rPo53f6dQD0?purpose=inline)

[Midnight | FF Ultima](https://images.openai.com/static-rsc-4/9BWRMu_DP25L193dQAg3dCF6EPYmnOExIpAs1lcIYzOWIv80Zko9kmiscfQkNitegxNvGeqsnb0_54FZPVIrD_LB2lJnj8MsFuHYqchNCM7c-nPE67CoXLCnjVP_VOxmeOhqmLvI11C9IzmesZP45v5en2uh8gtTswXMx5S9VYA?purpose=inline)

[Why the Zen Browser Is the Perfect Firefox Replacement For Me - Make Tech Easier](https://images.openai.com/static-rsc-4/sQCgX4V3nP368W9zbe0aCbw9v_M9XfU7BypcXWTQbIvJl6hEeNxNph-kBPRTv5NEgvb9JLizE_Or-I7bYzHQVmoJdexMi2WfT93rySrXgTP_rVs457kAKntnn56xMUNtT1Ru-FhpOL_LwveqHOfxsi4psopSi5VAF32DfxVzMbE?purpose=inline)

9

**RushBross** adalah browser desktop Rust dengan UI native buatan sendiri. Konten halaman dirender oleh WebView2 yang disediakan Windows, sehingga browser shell tetap ringan, kecil, dan tidak membawa engine browser sendiri.

Bukan Chromium yang diganti nama. Bukan Electron. Satu-satunya page engine adalah WebView2. Tanpa engine alternatif dan tanpa fallback ke reader mode atau parser internal.

Target kita adalah browser yang membuka aplikasi secepat mungkin, memiliki footprint sekecil mungkin, dan tetap mampu membuka web nyata termasuk YouTube.

Keputusan WebView2-only ini menggantikan rencana HTML/CSS/JS engine custom pada dokumen ini.

# **01 — Product identity**

[R Arrow Logo Design // For SALE by Bohdan Harbaruk 🇺🇦 on Dribbble](https://images.openai.com/static-rsc-4/0FN5D9tH50Q23y1tIOo0qgYYcG4JC5VQiPhO_wMtjx9LU5cxwHLtC8LnEY_pAauttxlD7C8WMVgX7kHGye_KNZGLd-t8aaT6_h8y5YjFH-fs-c3f6mdBRU-eSYF1L-NAObd7dBH9-_64LnU0EGxnYo-rZHMHTt84wZoca49I6Ps?purpose=inline)

# **RushBross**

Built to move. Built to be fast.

CORE PRINCIPLE

Speed over everything

ENGINE

WebView2, disediakan Windows

UI

Native custom interface

PHILOSOPHY

Minimal resources

### **Brand direction**

- **Nama:** RushBross
- **Karakter:** Fast, sharp, lightweight, minimal, confident.
- **UI:** Firefox-inspired usability, bukan Firefox clone.
- **Warna:** Black, graphite, white, gray.
- **Accent:** Tidak menggunakan warna accent permanen.
- **Icon:** Monogram abstrak RB atau bentuk kecepatan.
- **Animasi:** Minimal, tidak ada animasi berat.
- **Typography:** Sans-serif clean.
- **Interaction:** Cepat, langsung, tanpa delay UI.

# **02 — Target utama**

RushBross tidak mengejar fitur sebanyak browser lain pada awal pengembangan. Kita mengejar fondasi yang tepat.

# **Fast**

Startup, navigation, input response, page rendering.

# **Light**

RAM, CPU, GPU, idle process, background services.

# **Secure**

Isolation, permissions, safe networking, site boundaries.

# **Native**

Rust UI, WebView2 page engine, no embedded Chromium.

### **Performance targets**

Ini target engineering yang akan digunakan sebagai tolok ukur saat development, bukan klaim hasil yang sudah dicapai.

| Metric                                | Target                  |
| ------------------------------------- | ----------------------- |
| Cold startup                          | ≤ 300 ms                |
| Warm startup                          | ≤ 150 ms                |
| UI idle RAM (tanpa page engine aktif) | 15–35 MB                |
| CPU idle                              | Mendekati 0%            |
| UI frame response                     | ≤ 16 ms                 |
| UI-only release binary                | 10–25 MB                |
| Empty tab                             | Resource sangat minimal |
| Idle tab                              | Suspendable             |
| Background activity                   | Minimal                 |

Target RAM akan kita ukur per konfigurasi. UI-only berbeda dengan browser yang sudah menjalankan networking, JS runtime, DOM, dan rendering.

# **03 — Teknologi inti RushBross**

Kita akan menghindari framework yang membawa runtime besar. UI tidak menggunakan HTML, CSS, JavaScript, atau React. Halaman web dirender oleh WebView2, bukan parser internal.

[Rust Icon and Logo Set for Modern Design](https://images.openai.com/static-rsc-4/_Ud88glhl17y28zMSDFYIOgcfjQulE_RW-xWSAa0LmyADbYTnyC65PVukdz9fNyBb3Pz5eXcLrn4M1OIzDPCRrQQvTJqrpwUAP5UWVTfg-dZ3OdJiiH_PPVlGd3fKmWShnao6Z_tn-JfhI160HENPZM7brXSXSF-iNk3akisHJE?purpose=inline)

**Rust**

Core language

[Wgpu图文详解（01）窗口与基本渲染-腾讯云开发者社区-腾讯云](https://images.openai.com/static-rsc-4/bcD5EuMM5pHHwFhKvWu0ixv3KQt-dtCUI_Oz71Bb9UzIzEKSNtlZNPR5xyk6Mdy8rnoz-HDHtfUTWxtymR6UJnEMAWaTrE4IKNt9bjGGeF0QMselTQlCp0qe2HAZjJOBLS0rwAWx5pvlbzy9skGRTRjc8Bg9aSd9FcWdH_LhkhQ?purpose=inline)

**winit**

Window management, input, event loop

[The story of WebGPU — The successor to WebGL | by Eytan Manor | Medium](https://images.openai.com/static-rsc-4/MEhE7fg2myfzVNv15Uj14lKQWOeHY-JuKahP0JB-wNyidoCU1pHS8wMnuMXsuQ4Ie_fqRYrZv0CnG9DenrdfQpje5PD-1oeGqxomAVX5yYPoYj30ttfCV-zCnfyBPSsF7Ih5QB9ZWV4bkqyRVlyQLz6bSJX2ng8hYqQ68_KKcSo?purpose=inline)

**wgpu**

GPU rendering, graphics pipeline

[wgpu-font-renderer — Rust gfx library // Lib.rs](https://images.openai.com/static-rsc-4/yqgpekLgEoZVl0-TOxAs3nGIvyy4TBqjIGOz9-HLObUR1kUbVL5UfHWvpZagyGfJf2qkxdLkp6s1GP6Oyv9DnHgdED023dTBwDjYnryAahxQB7Nqhf7GUQW2jIUMxw42zn2R1uVGzFL79mch1v5lI4yoWMsmvj1Tq_VYn7NMc1o?purpose=inline)

**cosmic-text + swash**

Font shaping, text rendering, glyphs

## **Stack yang akan digunakan**

| Layer              | Library / Teknologi             | Penggunaan                    |
| ------------------ | ------------------------------- | ----------------------------- |
| Language           | Rust stable                     | Seluruh source code           |
| UI Window          | `winit`                         | Window, keyboard, mouse       |
| GPU Rendering      | `wgpu`                          | GPU drawing                   |
| UI Components      | Custom RushBross UI             | Tabs, toolbar, menus          |
| Text               | `cosmic-text`                   | Text shaping & layout         |
| Font rasterization | `swash`                         | Glyph rasterization           |
| Font               | System fonts + bundled fallback | Font rendering                |
| Page engine        | WebView2                        | HTML, CSS, JS, media, layout   |
| WebView binding    | `webview2-com`                  | Native COM host                |
| Win32 bridge       | `windows`                       | HWND and COM interop           |
| URL                | `url`                           | URL normalization & search    |
| Database           | `rusqlite`                      | History, bookmarks, metadata  |
| Logging            | `tracing`                       | Structured logs               |
| Serialization      | `serde`                         | Settings & data structures    |

Dependencies di atas merupakan kandidat stack, bukan semuanya wajib dimasukkan sejak awal. Dependency akan dipilih berdasarkan benchmark, ukuran binary, memory usage, dan license compatibility.

### **UI engine: Custom Rust UI**

Saya tidak menyarankan UI browser dibuat menggunakan framework web.

RushBross UI akan mempunyai sistem widget sendiri:

- Layout engine
- Flex-like horizontal dan vertical containers
- Hit testing
- Mouse state
- Keyboard focus
- Text input
- Scroll containers
- Tab rendering
- Toolbar
- Context menu
- Popup
- Tooltip
- Window state
- GPU compositor

UI widget menggunakan komponen internal yang tidak memiliki DOM.

# **04 — Design system**

[「Microsoft Edge」vs.「Firefox」｜2021年のベストブラウザはどっち？ | ライフハッカー・ジャパン](https://images.openai.com/static-rsc-4/wnZZDeSuQGbTAjxf9YoFrt3VujQS4gMCt2yJT-W_HBux9zHHmFHNNTa6nV05lZwcVW6RlQQcTv4_xvrxbrL_k2USaq45ock-8rQLva7qrrFKDq9nqqRwM3IChnviC9CvFzOvx-LWRH-WTw53L6aH9WTkfMlDtbvEnQr1uojsPGk?purpose=inline)

RushBross akan memiliki UI yang terinspirasi usability Firefox, tetapi lebih minimalis, modern, dan berfokus pada kecepatan.

## **Browser layout**

RushBross

New Tab

\+

←

→

↻

Search or enter address

⋮

# **RushBross**

Fast. Nothing in the way.

Search the web or enter a URL

Conceptual layout — bukan screenshot aplikasi final.

### **Layout components**

| Component          | Fungsi                       |
| ------------------ | ---------------------------- |
| Tab strip          | Tab dengan ukuran compact    |
| Tab title          | Judul halaman + favicon      |
| Tab close          | Close individual tab         |
| New tab            | Create tab                   |
| Navigation toolbar | Back, forward, reload        |
| URL bar            | URL dan search input         |
| Security indicator | HTTPS / site security        |
| Bookmark           | Add / remove bookmark        |
| Downloads          | Download indicator           |
| Menu               | Settings, history, downloads |
| Status             | Loading progress             |
| Page viewport      | Rendering surface            |

### **UI behavior**

- Tab menggunakan width dinamis.
- Tab inactive tidak melakukan animasi.
- Tab tidak memiliki shadow berat.
- Animasi transisi maksimal 100–150 ms.
- Tidak ada blur, glassmorphism, atau backdrop filter.
- Tidak ada animasi looping.
- Tidak ada polling UI.
- Event-driven rendering.
- Render hanya saat state berubah.
- UI harus tetap responsif saat page engine sedang bekerja.

# **05 — Architecture**

RushBross menggunakan modular monolith pada tahap awal, tetapi setiap subsystem memiliki boundary yang jelas. Kita tidak langsung memecahnya menjadi banyak process, karena overhead process akan bertentangan dengan target memory minimal.

**RUSHBROSS DESKTOP**

Window · UI · Input · Tab Manager

**RUSHBROSS BROWSER CORE**

Navigation · Session · Site Context · WebView Host

**WebView2 Engine**

HTML · DOM · CSS · JavaScript · Network · Media

**Shell Graphics**

Paint · Text · GPU · Compositor

**RUSHBROSS PLATFORM**

Windows · Filesystem · GPU · OS Networking · Security

## **Core modules**

### **1. Desktop**

Mengelola seluruh aplikasi desktop.

```
desktop/
├── window
├── event_loop
├── keyboard
├── mouse
├── window_state
├── fullscreen
├└── clipboard
```

Responsibilities:

- Create native window.
- Handle window resizing.
- Handle DPI scaling.
- Handle keyboard shortcuts.
- Handle mouse events.
- Handle clipboard.
- Handle minimize / maximize / close.
- Manage app lifecycle.

### **2. RushBross UI**

UI framework internal.

```
ui/
├── renderer
├── layout
├── widget
├── input
├── focus
├── text
├── animation
├── theme
└── components
    ├── tabs
    ├── address_bar
    ├── toolbar
    ├── popup
    ├── menu
    ├── button
    ├── icon
    ├── scroll
    └── tooltip
```

Tidak ada framework frontend.

Seluruh UI dirender secara langsung menggunakan GPU, dan semua komponen memiliki sistem state dan event yang terkontrol.

### **3. Browser core**

```
browser_core/
├── application
├── navigation
├── tab_manager
├── session
├── site_context
├── resource_scheduler
├├── permissions
└── lifecycle
```

Tugasnya:

- Membuat dan mengelola tab.
- Navigation lifecycle.
- Mengatur URL.
- Mengatur page loading.
- Menentukan tab aktif.
- Menangani tab suspend/resume.
- Mengelola state browser.
- Menentukan kapan resource boleh digunakan.

### **4. Networking**

```
network/
├── dns
├── connection_pool
├── http
├── https
├── tls
├── headers
├── redirects
├── cookies
├── cache
├├── websocket
└── download
```

Target:

- Connection reuse.
- Keep-alive.
- HTTP/2.
- HTTP/3 sebagai tahap lanjutan.
- DNS cache.
- Compression.
- Resource prioritization.
- Cancellation request.
- Timeout.
- Download streaming.
- TLS verification.

Networking harus asynchronous agar loading halaman tidak membuat UI freeze.

### **5. HTML engine**

```
html/
├── tokenizer
├── token
├── parser
├── tree_builder
├── dom
│   ├── document
│   ├── element
│   ├── text
│   ├── attribute
│   └── node
└── serializer
```

Contoh pipeline:

\#chatgpt-mermaid-\_r_14f\_{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;fill:rgb(237, 237, 237);}@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}#chatgpt-mermaid-\_r_14f\_ .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_14f\_ .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_14f\_ .error-icon{fill:rgb(48, 48, 48);}#chatgpt-mermaid-\_r_14f\_ .error-text{fill:rgb(237, 237, 237);stroke:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .edge-thickness-normal{stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .edge-thickness-thick{stroke-width:3.5px;}#chatgpt-mermaid-\_r_14f\_ .edge-pattern-solid{stroke-dasharray:0;}#chatgpt-mermaid-\_r_14f\_ .edge-thickness-invisible{stroke-width:0;fill:none;}#chatgpt-mermaid-\_r_14f\_ .edge-pattern-dashed{stroke-dasharray:3;}#chatgpt-mermaid-\_r_14f\_ .edge-pattern-dotted{stroke-dasharray:2;}#chatgpt-mermaid-\_r_14f\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14f\_ .marker.cross{stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14f\_ svg{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;}#chatgpt-mermaid-\_r_14f\_ p{margin:0;}#chatgpt-mermaid-\_r_14f\_ .label{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .cluster-label text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .cluster-label span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .cluster-label span p{background-color:transparent;}#chatgpt-mermaid-\_r_14f\_ .label text,#chatgpt-mermaid-\_r_14f\_ span{fill:rgb(237, 237, 237);color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .node rect,#chatgpt-mermaid-\_r_14f\_ .node circle,#chatgpt-mermaid-\_r_14f\_ .node ellipse,#chatgpt-mermaid-\_r_14f\_ .node polygon,#chatgpt-mermaid-\_r_14f\_ .node path{fill:rgb(36, 22, 63);stroke:rgb(120, 73, 209);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .rough-node .label text,#chatgpt-mermaid-\_r_14f\_ .node .label text,#chatgpt-mermaid-\_r_14f\_ .image-shape .label,#chatgpt-mermaid-\_r_14f\_ .icon-shape .label{text-anchor:middle;}#chatgpt-mermaid-\_r_14f\_ .node .katex path{fill:#000;stroke:#000;stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .rough-node .label,#chatgpt-mermaid-\_r_14f\_ .node .label,#chatgpt-mermaid-\_r_14f\_ .image-shape .label,#chatgpt-mermaid-\_r_14f\_ .icon-shape .label{text-align:center;}#chatgpt-mermaid-\_r_14f\_ .node.clickable{cursor:pointer;}#chatgpt-mermaid-\_r_14f\_ .root .anchor path{fill:rgb(175, 175, 175)!important;stroke-width:0;stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14f\_ .arrowheadPath{fill:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14f\_ .edgePath .path{stroke:rgb(175, 175, 175);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .flowchart-link{stroke:rgb(175, 175, 175);fill:none;}#chatgpt-mermaid-\_r_14f\_ .edgeLabel{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_14f\_ .edgeLabel p{background-color:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14f\_ .edgeLabel rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14f\_ .labelBkg{background-color:rgba(0, 0, 0, 0.5);}#chatgpt-mermaid-\_r_14f\_ .cluster rect{fill:rgb(48, 48, 48);stroke:rgba(255, 255, 255, 0.15);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .cluster text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ .cluster span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ div.mermaidTooltip{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:12px;background:rgb(48, 48, 48);border:1px solid rgba(255, 255, 255, 0.15);border-radius:2px;pointer-events:none;z-index:100;}#chatgpt-mermaid-\_r_14f\_ .flowchartTitleText{text-anchor:middle;font-size:18px;fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14f\_ rect.text{fill:none;stroke-width:0;}#chatgpt-mermaid-\_r_14f\_ .icon-shape,#chatgpt-mermaid-\_r_14f\_ .image-shape{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_14f\_ .icon-shape p,#chatgpt-mermaid-\_r_14f\_ .image-shape p{background-color:rgb(0, 0, 0);padding:2px;}#chatgpt-mermaid-\_r_14f\_ .icon-shape .label rect,#chatgpt-mermaid-\_r_14f\_ .image-shape .label rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14f\_ .label-icon{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}#chatgpt-mermaid-\_r_14f\_ .node .label-icon path{fill:currentColor;stroke:revert;stroke-width:revert;}#chatgpt-mermaid-\_r_14f\_ .node .neo-node{stroke:rgb(120, 73, 209);}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node rect,#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].cluster rect,#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node polygon{stroke:url(#chatgpt-mermaid-\_r_14f\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].swimlane.cluster rect{filter:none;}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node path{stroke:url(#chatgpt-mermaid-\_r_14f\_-gradient);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node .outer-path{filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node .neo-line path{stroke:rgb(120, 73, 209);filter:none;}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node circle{stroke:url(#chatgpt-mermaid-\_r_14f\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].node circle .state-start{fill:#000000;}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].icon-shape .icon{fill:url(#chatgpt-mermaid-\_r_14f\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14f\_ [data-look="neo"].icon-shape .icon-neo path{stroke:url(#chatgpt-mermaid-\_r_14f\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14f\_ .node text{font-size:14px;font-weight:600;letter-spacing:normal;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_14f\_ .edgeLabels text{font-size:13px;font-weight:600;letter-spacing:-0.08px;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_14f\_ .node tspan[font-weight="normal"],#chatgpt-mermaid-\_r_14f\_ .edgeLabels tspan[font-weight="normal"]{font-weight:600;}#chatgpt-mermaid-\_r_14f\_ .edgeLabel .label rect{opacity:1;rx:13px;ry:13px;fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .node rect,#chatgpt-mermaid-\_r_14f\_ .node circle,#chatgpt-mermaid-\_r_14f\_ .node ellipse,#chatgpt-mermaid-\_r_14f\_ .node polygon,#chatgpt-mermaid-\_r_14f\_ .node path{fill:rgb(44, 24, 74);stroke:rgba(255, 255, 255, 0.1);stroke-width:1px;}#chatgpt-mermaid-\_r_14f\_ .node rect{rx:16px;ry:16px;}#chatgpt-mermaid-\_r_14f\_ .mermaid-decision .label-container{fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-dasharray:2,2;}#chatgpt-mermaid-\_r_14f\_ .edgePaths .flowchart-link{stroke:rgb(175, 175, 175);stroke-width:1px;stroke-linecap:round;stroke-linejoin:round;}#chatgpt-mermaid-\_r_14f\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14f\_ :root{--mermaid-font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";}HTML SourceTokenizerHTML TokensTree BuilderDOM TreeStyle Engine

Kita harus membangun parser yang menangani malformed HTML juga. Browser tidak boleh langsung gagal ketika HTML website tidak valid.

### **6. CSS engine**

```
css/
├── tokenizer
├── parser
├── stylesheet
├── selector
├── cascade
├── specificity
├── computed_style
├── inheritance
├── values
└── layout
    ├── box
    ├── block
    ├── inline
    ├── flex
    ├── grid
    └── position
```

Urutan pengembangan CSS:

1. Color
2. Background
3. Font
4. Margin
5. Padding
6. Border
7. Width / Height
8. Display
9. Block
10. Inline
11. Overflow
12. Position
13. Flexbox
14. Grid
15. Media queries
16. Pseudo classes

Grid dan Flexbox harus dibangun dengan testing terpisah karena keduanya sangat memengaruhi kompatibilitas website.

### **7. JavaScript runtime**

```
javascript/
├── runtime
├── execution
├── context
├── scheduler
├── bindings
├── dom_bindings
├── event_bindings
└── web_apis
    ├── console
    ├── fetch
    ├── timer
    ├── storage
    └── location
```

JavaScript tidak perlu dimuat pada tab yang hanya menampilkan HTML statis.

Lazy JavaScript Runtime

Tab tanpa script tidak membutuhkan JavaScript engine aktif. Runtime baru diinisialisasi ketika halaman membutuhkan eksekusi JavaScript.

Ini salah satu strategi utama penghematan RAM.

### **8. Rendering engine**

```
rendering/
├── style
├── layout
├── display_list
├── paint
├── raster
├── text
├── image
├── compositor
├── gpu
└── invalidation
```

Pipeline:

\#chatgpt-mermaid-\_r_14t\_{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;fill:rgb(237, 237, 237);}@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}#chatgpt-mermaid-\_r_14t\_ .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_14t\_ .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_14t\_ .error-icon{fill:rgb(48, 48, 48);}#chatgpt-mermaid-\_r_14t\_ .error-text{fill:rgb(237, 237, 237);stroke:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .edge-thickness-normal{stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .edge-thickness-thick{stroke-width:3.5px;}#chatgpt-mermaid-\_r_14t\_ .edge-pattern-solid{stroke-dasharray:0;}#chatgpt-mermaid-\_r_14t\_ .edge-thickness-invisible{stroke-width:0;fill:none;}#chatgpt-mermaid-\_r_14t\_ .edge-pattern-dashed{stroke-dasharray:3;}#chatgpt-mermaid-\_r_14t\_ .edge-pattern-dotted{stroke-dasharray:2;}#chatgpt-mermaid-\_r_14t\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14t\_ .marker.cross{stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14t\_ svg{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;}#chatgpt-mermaid-\_r_14t\_ p{margin:0;}#chatgpt-mermaid-\_r_14t\_ .label{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .cluster-label text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .cluster-label span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .cluster-label span p{background-color:transparent;}#chatgpt-mermaid-\_r_14t\_ .label text,#chatgpt-mermaid-\_r_14t\_ span{fill:rgb(237, 237, 237);color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .node rect,#chatgpt-mermaid-\_r_14t\_ .node circle,#chatgpt-mermaid-\_r_14t\_ .node ellipse,#chatgpt-mermaid-\_r_14t\_ .node polygon,#chatgpt-mermaid-\_r_14t\_ .node path{fill:rgb(36, 22, 63);stroke:rgb(120, 73, 209);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .rough-node .label text,#chatgpt-mermaid-\_r_14t\_ .node .label text,#chatgpt-mermaid-\_r_14t\_ .image-shape .label,#chatgpt-mermaid-\_r_14t\_ .icon-shape .label{text-anchor:middle;}#chatgpt-mermaid-\_r_14t\_ .node .katex path{fill:#000;stroke:#000;stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .rough-node .label,#chatgpt-mermaid-\_r_14t\_ .node .label,#chatgpt-mermaid-\_r_14t\_ .image-shape .label,#chatgpt-mermaid-\_r_14t\_ .icon-shape .label{text-align:center;}#chatgpt-mermaid-\_r_14t\_ .node.clickable{cursor:pointer;}#chatgpt-mermaid-\_r_14t\_ .root .anchor path{fill:rgb(175, 175, 175)!important;stroke-width:0;stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14t\_ .arrowheadPath{fill:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14t\_ .edgePath .path{stroke:rgb(175, 175, 175);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .flowchart-link{stroke:rgb(175, 175, 175);fill:none;}#chatgpt-mermaid-\_r_14t\_ .edgeLabel{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_14t\_ .edgeLabel p{background-color:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14t\_ .edgeLabel rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14t\_ .labelBkg{background-color:rgba(0, 0, 0, 0.5);}#chatgpt-mermaid-\_r_14t\_ .cluster rect{fill:rgb(48, 48, 48);stroke:rgba(255, 255, 255, 0.15);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .cluster text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ .cluster span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ div.mermaidTooltip{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:12px;background:rgb(48, 48, 48);border:1px solid rgba(255, 255, 255, 0.15);border-radius:2px;pointer-events:none;z-index:100;}#chatgpt-mermaid-\_r_14t\_ .flowchartTitleText{text-anchor:middle;font-size:18px;fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_14t\_ rect.text{fill:none;stroke-width:0;}#chatgpt-mermaid-\_r_14t\_ .icon-shape,#chatgpt-mermaid-\_r_14t\_ .image-shape{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_14t\_ .icon-shape p,#chatgpt-mermaid-\_r_14t\_ .image-shape p{background-color:rgb(0, 0, 0);padding:2px;}#chatgpt-mermaid-\_r_14t\_ .icon-shape .label rect,#chatgpt-mermaid-\_r_14t\_ .image-shape .label rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_14t\_ .label-icon{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}#chatgpt-mermaid-\_r_14t\_ .node .label-icon path{fill:currentColor;stroke:revert;stroke-width:revert;}#chatgpt-mermaid-\_r_14t\_ .node .neo-node{stroke:rgb(120, 73, 209);}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node rect,#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].cluster rect,#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node polygon{stroke:url(#chatgpt-mermaid-\_r_14t\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].swimlane.cluster rect{filter:none;}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node path{stroke:url(#chatgpt-mermaid-\_r_14t\_-gradient);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node .outer-path{filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node .neo-line path{stroke:rgb(120, 73, 209);filter:none;}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node circle{stroke:url(#chatgpt-mermaid-\_r_14t\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].node circle .state-start{fill:#000000;}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].icon-shape .icon{fill:url(#chatgpt-mermaid-\_r_14t\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14t\_ [data-look="neo"].icon-shape .icon-neo path{stroke:url(#chatgpt-mermaid-\_r_14t\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_14t\_ .node text{font-size:14px;font-weight:600;letter-spacing:normal;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_14t\_ .edgeLabels text{font-size:13px;font-weight:600;letter-spacing:-0.08px;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_14t\_ .node tspan[font-weight="normal"],#chatgpt-mermaid-\_r_14t\_ .edgeLabels tspan[font-weight="normal"]{font-weight:600;}#chatgpt-mermaid-\_r_14t\_ .edgeLabel .label rect{opacity:1;rx:13px;ry:13px;fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .node rect,#chatgpt-mermaid-\_r_14t\_ .node circle,#chatgpt-mermaid-\_r_14t\_ .node ellipse,#chatgpt-mermaid-\_r_14t\_ .node polygon,#chatgpt-mermaid-\_r_14t\_ .node path{fill:rgb(44, 24, 74);stroke:rgba(255, 255, 255, 0.1);stroke-width:1px;}#chatgpt-mermaid-\_r_14t\_ .node rect{rx:16px;ry:16px;}#chatgpt-mermaid-\_r_14t\_ .mermaid-decision .label-container{fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-dasharray:2,2;}#chatgpt-mermaid-\_r_14t\_ .edgePaths .flowchart-link{stroke:rgb(175, 175, 175);stroke-width:1px;stroke-linecap:round;stroke-linejoin:round;}#chatgpt-mermaid-\_r_14t\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_14t\_ :root{--mermaid-font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";}DOMStyle CalculationLayoutDisplay ListPaintRasterizationCompositorGPU / Screen

### **Prinsip rendering**

- Jangan rerender seluruh halaman setiap ada perubahan.
- Jangan recompute seluruh CSS jika hanya satu node berubah.
- Gunakan invalidation system.
- Gunakan display list.
- Gunakan caching glyph.
- Gunakan caching image.
- Gunakan dirty rectangle.
- Gunakan GPU compositing untuk kebutuhan yang sesuai.
- Gunakan CPU rendering fallback.

# **06 — Memory architecture**

Ini bagian yang paling penting untuk RushBross. Kita tidak hanya mengurangi RAM, tetapi mengontrol kapan setiap objek dibuat, berapa lama hidupnya, dan kapan dibuang.

## **Memory ownership**

**Application Lifetime**

Window · UI · Browser settings

**Tab Lifetime**

Navigation · DOM · CSS · Page state

**Resource Lifetime**

Images · Fonts · Buffers · Scripts

Setiap resource memiliki lifecycle.

| Resource             | Lifecycle                       |
| -------------------- | ------------------------------- |
| Browser UI           | Selama aplikasi berjalan        |
| Active tab           | Selama tab aktif                |
| Inactive tab         | Dapat di-suspend                |
| DOM                  | Selama halaman dibutuhkan       |
| JavaScript runtime   | Dibuat jika diperlukan          |
| Image decoded buffer | Release saat tidak dibutuhkan   |
| Network buffer       | Release setelah request selesai |
| Temporary memory     | Reset per operation/page        |
| GPU texture          | Evict berdasarkan cache budget  |

### **Memory strategy**

**1. Arena allocation**

Untuk object yang memiliki lifecycle sama, misalnya satu dokumen HTML, gunakan arena atau pool allocation yang dapat dilepas bersama.

**2. Avoid unnecessary clones**

Gunakan reference yang sesuai, `Arc` hanya ketika ownership memang harus dibagi, dan hindari clone data besar.

**3. Compact DOM**

Node tidak boleh berisi banyak data redundant. Node memiliki struktur minimal, dengan attributes, children, dan data yang benar-benar diperlukan.

**4. Image memory management**

File image compressed tidak sama dengan decoded image memory.

Contohnya, gambar 4000 × 3000 dengan RGBA bisa menggunakan:

\\[ 4000 \times 3000 \times 4 = 48{,}000{,}000\text{ bytes} \\]

Sekitar 45.8 MiB hanya untuk pixel buffer mentah. Karena itu image decoding, resizing, caching, dan eviction sangat penting.

**5. Tab sleeping**

Tab inactive dapat diturunkan melalui beberapa level:

| State     | Kondisi                                                 |
| --------- | ------------------------------------------------------- |
| Active    | Halaman aktif, rendering berjalan                       |
| Warm      | DOM tetap tersedia, render dibatasi                     |
| Frozen    | Scheduler berhenti, timer ditangguhkan sesuai kebijakan |
| Discarded | Resource halaman dilepas, tab dapat dimuat kembali      |

Discarding tidak boleh sembarangan karena halaman mungkin memiliki state yang belum disimpan.

# **07 — CPU & Thread architecture**

RushBross tidak boleh memiliki thread yang terus-menerus bekerja tanpa alasan.

## **Thread design**

**Main Thread — UI**

Window events · Input · UI state · Frame scheduling

**Network Worker**

HTTP · TLS · DNS · Streams

**Engine Worker**

HTML · CSS · Layout · JS

**Renderer / GPU**

Display list · Raster · Compositor · Presentation

### **Aturan threading**

- UI thread tidak pernah menunggu HTTP request.
- Networking asynchronous.
- HTML parsing dapat dijalankan secara background.
- JavaScript execution tidak boleh memblokir UI.
- Rendering hanya saat ada invalidation.
- Worker tidak dibuat per request.
- Gunakan worker pool jika benar-benar dibutuhkan.
- Hindari terlalu banyak thread kecil.
- Thread count tidak boleh terus meningkat mengikuti jumlah tab.
- Cancellation harus dapat menghentikan request dan pekerjaan halaman.

### **Event-driven system**

\#chatgpt-mermaid-\_r_15d\_{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;fill:rgb(237, 237, 237);}@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}#chatgpt-mermaid-\_r_15d\_ .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_15d\_ .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_15d\_ .error-icon{fill:rgb(48, 48, 48);}#chatgpt-mermaid-\_r_15d\_ .error-text{fill:rgb(237, 237, 237);stroke:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .edge-thickness-normal{stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .edge-thickness-thick{stroke-width:3.5px;}#chatgpt-mermaid-\_r_15d\_ .edge-pattern-solid{stroke-dasharray:0;}#chatgpt-mermaid-\_r_15d\_ .edge-thickness-invisible{stroke-width:0;fill:none;}#chatgpt-mermaid-\_r_15d\_ .edge-pattern-dashed{stroke-dasharray:3;}#chatgpt-mermaid-\_r_15d\_ .edge-pattern-dotted{stroke-dasharray:2;}#chatgpt-mermaid-\_r_15d\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_15d\_ .marker.cross{stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_15d\_ svg{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;}#chatgpt-mermaid-\_r_15d\_ p{margin:0;}#chatgpt-mermaid-\_r_15d\_ .label{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .cluster-label text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .cluster-label span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .cluster-label span p{background-color:transparent;}#chatgpt-mermaid-\_r_15d\_ .label text,#chatgpt-mermaid-\_r_15d\_ span{fill:rgb(237, 237, 237);color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .node rect,#chatgpt-mermaid-\_r_15d\_ .node circle,#chatgpt-mermaid-\_r_15d\_ .node ellipse,#chatgpt-mermaid-\_r_15d\_ .node polygon,#chatgpt-mermaid-\_r_15d\_ .node path{fill:rgb(36, 22, 63);stroke:rgb(120, 73, 209);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .rough-node .label text,#chatgpt-mermaid-\_r_15d\_ .node .label text,#chatgpt-mermaid-\_r_15d\_ .image-shape .label,#chatgpt-mermaid-\_r_15d\_ .icon-shape .label{text-anchor:middle;}#chatgpt-mermaid-\_r_15d\_ .node .katex path{fill:#000;stroke:#000;stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .rough-node .label,#chatgpt-mermaid-\_r_15d\_ .node .label,#chatgpt-mermaid-\_r_15d\_ .image-shape .label,#chatgpt-mermaid-\_r_15d\_ .icon-shape .label{text-align:center;}#chatgpt-mermaid-\_r_15d\_ .node.clickable{cursor:pointer;}#chatgpt-mermaid-\_r_15d\_ .root .anchor path{fill:rgb(175, 175, 175)!important;stroke-width:0;stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_15d\_ .arrowheadPath{fill:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_15d\_ .edgePath .path{stroke:rgb(175, 175, 175);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .flowchart-link{stroke:rgb(175, 175, 175);fill:none;}#chatgpt-mermaid-\_r_15d\_ .edgeLabel{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_15d\_ .edgeLabel p{background-color:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_15d\_ .edgeLabel rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_15d\_ .labelBkg{background-color:rgba(0, 0, 0, 0.5);}#chatgpt-mermaid-\_r_15d\_ .cluster rect{fill:rgb(48, 48, 48);stroke:rgba(255, 255, 255, 0.15);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .cluster text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ .cluster span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ div.mermaidTooltip{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:12px;background:rgb(48, 48, 48);border:1px solid rgba(255, 255, 255, 0.15);border-radius:2px;pointer-events:none;z-index:100;}#chatgpt-mermaid-\_r_15d\_ .flowchartTitleText{text-anchor:middle;font-size:18px;fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_15d\_ rect.text{fill:none;stroke-width:0;}#chatgpt-mermaid-\_r_15d\_ .icon-shape,#chatgpt-mermaid-\_r_15d\_ .image-shape{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_15d\_ .icon-shape p,#chatgpt-mermaid-\_r_15d\_ .image-shape p{background-color:rgb(0, 0, 0);padding:2px;}#chatgpt-mermaid-\_r_15d\_ .icon-shape .label rect,#chatgpt-mermaid-\_r_15d\_ .image-shape .label rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_15d\_ .label-icon{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}#chatgpt-mermaid-\_r_15d\_ .node .label-icon path{fill:currentColor;stroke:revert;stroke-width:revert;}#chatgpt-mermaid-\_r_15d\_ .node .neo-node{stroke:rgb(120, 73, 209);}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node rect,#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].cluster rect,#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node polygon{stroke:url(#chatgpt-mermaid-\_r_15d\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].swimlane.cluster rect{filter:none;}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node path{stroke:url(#chatgpt-mermaid-\_r_15d\_-gradient);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node .outer-path{filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node .neo-line path{stroke:rgb(120, 73, 209);filter:none;}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node circle{stroke:url(#chatgpt-mermaid-\_r_15d\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].node circle .state-start{fill:#000000;}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].icon-shape .icon{fill:url(#chatgpt-mermaid-\_r_15d\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_15d\_ [data-look="neo"].icon-shape .icon-neo path{stroke:url(#chatgpt-mermaid-\_r_15d\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_15d\_ .node text{font-size:14px;font-weight:600;letter-spacing:normal;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_15d\_ .edgeLabels text{font-size:13px;font-weight:600;letter-spacing:-0.08px;fill:rgb(206, 176, 251);}#chatgpt-mermaid-\_r_15d\_ .node tspan[font-weight="normal"],#chatgpt-mermaid-\_r_15d\_ .edgeLabels tspan[font-weight="normal"]{font-weight:600;}#chatgpt-mermaid-\_r_15d\_ .edgeLabel .label rect{opacity:1;rx:13px;ry:13px;fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .node rect,#chatgpt-mermaid-\_r_15d\_ .node circle,#chatgpt-mermaid-\_r_15d\_ .node ellipse,#chatgpt-mermaid-\_r_15d\_ .node polygon,#chatgpt-mermaid-\_r_15d\_ .node path{fill:rgb(44, 24, 74);stroke:rgba(255, 255, 255, 0.1);stroke-width:1px;}#chatgpt-mermaid-\_r_15d\_ .node rect{rx:16px;ry:16px;}#chatgpt-mermaid-\_r_15d\_ .mermaid-decision .label-container{fill:rgb(23, 12, 38);stroke:rgb(65, 47, 92);stroke-dasharray:2,2;}#chatgpt-mermaid-\_r_15d\_ .edgePaths .flowchart-link{stroke:rgb(175, 175, 175);stroke-width:1px;stroke-linecap:round;stroke-linejoin:round;}#chatgpt-mermaid-\_r_15d\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_15d\_ :root{--mermaid-font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";}OS EventEvent LoopEvent TypeUI EventNetwork EventPage EventUpdate UI StateUpdate Resource StateInvalidate PageRender If Needed

Browser idle harus benar-benar idle.

# **08 — Storage & browser profile**

Semua data browser disimpan lokal dan dikontrol RushBross.

```
RushBross/
└── profile/
    ├── config/
    │   └── settings.toml
    ├── database/
    │   └── browser.db
    ├── history/
    ├── bookmarks/
    ├── cookies/
    ├── cache/
    │   ├── http/
    │   ├── images/
    │   ├── fonts/
    │   └── gpu/
    ├── sessions/
    ├── downloads/
    └── logs/
```

## **Database**

SQLite digunakan untuk:

- History
- Bookmarks
- Saved sessions
- Browser settings
- Download metadata
- Site permissions
- Search history jika diaktifkan

Tidak perlu database server.

## **Storage policies**

| Data        | Kebijakan                      |
| ----------- | ------------------------------ |
| History     | Batas configurable             |
| HTTP cache  | Size limit                     |
| Image cache | LRU eviction                   |
| Font cache  | Reuse                          |
| GPU cache   | Evict saat memory pressure     |
| Cookies     | Domain isolation               |
| Session     | Restore sesuai user preference |
| Downloads   | File system langsung           |

Cache harus mempunyai batas yang jelas. Jangan sampai browser menjadi ringan saat startup, tetapi cache tumbuh tanpa batas.

# **09 — Security architecture**

Browser memproses website yang tidak kita kontrol. Karena itu performa tidak boleh mengorbankan keamanan.

**Security requirements**

- HTTPS certificate verification.
- Origin isolation.
- Same-origin policy.
- Cookie isolation.
- Permission control.
- Safe download handling.
- URL scheme validation.
- Prevent arbitrary local file access.
- Resource limits.
- Protection against malformed HTML/CSS.
- JavaScript restrictions.
- Sandbox strategy.

### **Process strategy**

Kita tidak harus memaksakan multi-process pada versi pertama karena overhead RAM cukup besar.

Namun, untuk browser yang nantinya digunakan membuka website publik, arsitektur jangka panjang sebaiknya memiliki isolasi:

- UI / Browser process
- Renderer process
- Network service boundary

Catatan penting: Single-process browser custom jauh lebih mudah dan hemat resource, tetapi memiliki risiko keamanan lebih tinggi ketika menjalankan JavaScript dan memproses website tidak tepercaya. Kita tidak boleh menganggap browser custom aman untuk penggunaan sensitif hanya karena ditulis menggunakan Rust.

Isolasi process, OS sandbox, permissions, dan security review harus menjadi syarat sebelum RushBross digunakan sebagai browser harian untuk aktivitas sensitif.

# **10 — New Tab design**

New Tab RushBross tidak boleh mengandung animasi, background video, atau widget berat.

[Browser web window template with search bar frames and buttons, website mockup for PC laptop in dark mode](https://images.openai.com/static-rsc-4/XEQLQGGDrmmXTsfB91mcDyh6S4Pa19ymhzJHTmKX9o9DjERzaxuXQHhMOZpA0o43VrFOg_ijSK_GkbZnrkGKhcV5GTWY3ZYfVPcTgYxgEJsu5nHOHfvbHatFBPuAe6Fya84Bp70IPA8rzkx-zGbKay99AEZGyvHlZCdJi-c4qfd_IhhRRZpQVjPDMOZBPqls?purpose=inline)

[Firefox opens maxmised but is actually windowed · Issue #1643 · arkenfox/user.js · GitHub](https://images.openai.com/static-rsc-4/NbdY1d_2j45taZw6pY4LP_Z4C4EsH2N1G7cNSqfeE8nSYRd1pJl1ReyYsCQ-8ZTRtlIzWiT4hsyd0t4Jc4tgRTzYNhZ_qw9jozGDIYSvd5-fgIAm97UTOlaBqzrMPv-HNaocXrMgE6KwoVXvZ7o1tenXicfHhi9x6KYiPdf7kIE?purpose=inline)

[Minimal New Tab - Simple Blank Page for Edge](https://images.openai.com/static-rsc-4/-P9r4SokQjJqoH0vPrKaS1nrVZnxKo6i-12vv7oP9T33nKHxUeUm9AxFNFJvPqnZ9VzpSJhS5OTFzXcuzNQfeos3IdARb-5aonrlNTS2vv1ABnF5ZGlQJ_NBsu0gs-yNClxq5zWwV6f25b3eyhThyWHf8nh3ota5G9YLqZeDt08?purpose=inline)

6

# **RushBross**

Search faster. Browse lighter.

Search or enter address

Google

GitHub

YouTube

Konsep visual, bukan screenshot produk.

### **New Tab features**

- Search bar.
- Favorite shortcuts.
- Recently visited.
- Optional clock.
- Optional wallpaper.
- Quick settings.

Default configuration:

- Clock: Off.
- Wallpaper: Off.
- Widgets: Off.
- News feed: Off.
- Background network request: Off.

# **11 — Browser features roadmap**

Kita tidak menambahkan semua fitur sekaligus.

## **Core Features**

Version 0.1 — 0.5

- Native window
- Custom UI
- New Tab
- URL bar
- Search
- Navigation
- HTML
- Text rendering
- Image rendering
- Basic CSS
- History
- Bookmarks
- Download
  ## **Browser Features**
  Version 0.6 — 0.9
- Tabs
- Cookies
- HTTP cache
- CSS Flexbox
- Forms
- JavaScript
- DOM events
- Fetch
- WebSockets
- Session restore
- Private mode
  ## **Modern Web**
  Version 1.0+
- CSS Grid
- Web APIs
- Advanced JavaScript
- HTTP/2
- HTTP/3
- WebAssembly
- GPU acceleration
- Process sandbox
- Extensions
- Developer tools

# **12 — Repository blueprint**

Saya merekomendasikan workspace Cargo, bukan satu file `main.rs` besar.

```
RushBross/
│
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── README.md
├── LICENSE
├── .gitignore
│
├── apps/
│   └── rushbross/
│       └── src/
│           ├── main.rs
│           ├── app.rs
│           ├── lifecycle.rs
│           └── config.rs
│
├── crates/
│   ├── rb-platform/
│   ├── rb-window/
│   ├── rb-ui/
│   ├── rb-ui-renderer/
│   ├── rb-ui-widgets/
│   │
│   ├── rb-browser/
│   ├── rb-navigation/
│   ├── rb-tabs/
│   ├── rb-session/
│   ├── rb-history/
│   ├── rb-bookmarks/
│   ├── rb-downloads/
│   │
│   ├── rb-network/
│   ├── rb-http/
│   ├── rb-tls/
│   ├── rb-cache/
│   ├── rb-cookies/
│   │
│   ├── rb-html/
│   ├── rb-dom/
│   ├── rb-css/
│   ├── rb-layout/
│   ├── rb-script/
│   ├── rb-webapi/
│   │
│   ├── rb-renderer/
│   ├── rb-paint/
│   ├── rb-text/
│   ├── rb-image/
│   ├── rb-compositor/
│   ├── rb-gpu/
│   │
│   ├── rb-storage/
│   ├── rb-security/
│   ├── rb-permissions/
│   ├── rb-devtools/
│   └── rb-benchmark/
│
├── assets/
│   ├── icons/
│   ├── fonts/
│   └── shaders/
│
├── tests/
│   ├── html/
│   ├── css/
│   ├── navigation/
│   ├── networking/
│   ├── ui/
│   └── performance/
│
├── benchmarks/
│   ├── startup/
│   ├── memory/
│   ├── rendering/
│   └── networking/
│
└── docs/
    ├── architecture/
    ├── rendering/
    ├── memory/
    ├── security/
    └── roadmap/
```

## **Workspace dependency rules**

**Rule 1**

UI tidak boleh bergantung pada HTML engine.

**Rule 2**

Browser core tidak boleh bergantung pada UI.

**Rule 3**

Rendering engine tidak boleh mengakses database secara langsung.

**Rule 4**

Networking tidak boleh mengubah DOM.

**Rule 5**

Semua komunikasi antar-module menggunakan interface, event, atau data model yang jelas.

**Rule 6**

Tidak boleh terjadi circular dependency.

# **13 — Milestone development plan**

Urutan pengembangan harus menghasilkan aplikasi yang bisa dijalankan di setiap milestone. Jangan membangun 15 crate sekaligus tanpa hasil yang bisa diuji.

### **M0**

**Native Foundation**

Window + event loop + startup

Deliverable: aplikasi native Rust yang terbuka sangat cepat.

### **M1**

**RushBross UI**

Custom UI rendering + interaction

Deliverable: browser shell, tabs, toolbar, URL bar, menu.

### **M2**

**Networking**

HTTP + HTTPS + resource fetching

Deliverable: URL dapat diakses dan resource dapat diunduh.

### **M3**

**HTML + DOM**

Custom parser + document tree

Deliverable: HTML ditampilkan dengan text, link, dan images.

### **M4**

**CSS + Layout**

Style engine + layout engine

Deliverable: halaman web mulai memiliki struktur dan styling.

### **M5**

**JavaScript**

Runtime + bindings + Web APIs

Deliverable: halaman interaktif mulai dapat dijalankan.

### **M6**

**Optimization**

Profiling + cache + GPU + memory

Deliverable: benchmark, tuning, memory control.

### **M7**

**Release Candidate**

Security + compatibility + installer

Deliverable: build Windows yang siap diuji oleh pengguna.

## **Detail milestone pertama**

| Milestone | Scope                            | Definition of done                                       |
| --------- | -------------------------------- | -------------------------------------------------------- |
| M0        | Rust runtime, window, event loop | Window terbuka, close, resize, tidak ada loop CPU tinggi |
| M1        | Custom UI                        | Tab, URL bar, keyboard, mouse, menu                      |
| M2        | Network                          | HTTPS request, timeout, redirect, error handling         |
| M3        | HTML                             | Parser, DOM, text, links, images                         |
| M4        | CSS                              | Basic styling, layout, scroll                            |
| M5        | JavaScript                       | Script execution, events, fetch                          |
| M6        | Performance                      | Profiling, memory budgets, cache                         |
| M7        | Release                          | Security review, packaging, installer                    |

### **Milestone M0 harus sangat sederhana**

Jangan memasukkan seluruh dependency browser pada milestone ini.

```
RushBross
└── Window
    ├── Event Loop
    ├── GPU Initialization
    ├── Input Handling
    └── Shutdown
```

Kita benchmark cold start, warm start, RAM, CPU idle, dan ukuran executable sebelum UI kompleks mulai dibuat.

# **14 — Performance laboratory**

RushBross akan memiliki benchmark sejak awal. Kita tidak menunggu browser selesai baru mengukur performa.

## **Benchmark categories**

| Benchmark       | Mengukur                                     |
| --------------- | -------------------------------------------- |
| Cold startup    | Waktu dari proses dimulai sampai UI siap     |
| Warm startup    | Startup pada kondisi cache sistem sudah warm |
| Idle RAM        | RAM saat aplikasi idle                       |
| Idle CPU        | CPU ketika tidak ada aktivitas               |
| Tab switching   | Kecepatan pergantian tab                     |
| URL navigation  | Respons input hingga navigation dimulai      |
| HTML parsing    | Parse speed                                  |
| DOM creation    | Waktu dan overhead node                      |
| CSS calculation | Style computation                            |
| Layout          | Layout performance                           |
| Image decode    | Waktu decoding image                         |
| GPU             | Frame time & GPU utilization                 |
| Memory pressure | Respons saat resource meningkat              |
| Tab suspension  | RAM yang berhasil dilepas                    |

## **Benchmark output**

Contoh format laporan yang akan dihasilkan:

```
RUSHBROSS PERFORMANCE REPORT

Version: 0.1.0
Build: Release
Platform: Windows

Startup
  Cold Start       : ---
  Warm Start       : ---

Memory
  Idle RAM         : ---
  Active Tab       : ---
  5 Tabs           : ---
  10 Tabs          : ---

CPU
  Idle             : ---
  Active Rendering : ---

Graphics
  Frame Time       : ---
  GPU Usage        : ---

Binary
  Size             : ---
```

Benchmark nantinya dijalankan pada kondisi yang sama agar kita bisa membandingkan versi 0.1 dengan 0.2 dan seterusnya.

# **15 — Dependency policy**

RushBross tidak boleh memiliki ratusan dependency tanpa alasan.

Setiap dependency harus melewati evaluasi:

- Apakah benar-benar diperlukan?
- Apakah pure Rust?
- Berapa binary size yang ditambahkan?
- Berapa RAM yang dikonsumsi?
- Apakah menambah startup latency?
- Apakah ada dependency yang lebih ringan?
- Apakah bisa diimplementasikan sendiri tanpa risiko besar?
- Apakah lisensinya sesuai?

### **Dependency yang boleh dipertimbangkan**

| Dependency    | Keputusan                             |
| ------------- | ------------------------------------- |
| `winit`       | Gunakan untuk native window           |
| `wgpu`        | Gunakan untuk GPU rendering           |
| `tokio`       | Minimal features                      |
| `rustls`      | TLS                                   |
| `hyper`       | Networking                            |
| `url`         | URL parsing                           |
| `rusqlite`    | Local storage                         |
| `cosmic-text` | Text shaping                          |
| `swash`       | Font rasterization                    |
| `image`       | Image decoding                        |
| `boa_engine`  | JavaScript, tahap lanjutan            |
| `serde`       | Configuration serialization           |
| `tracing`     | Logging, dapat dimatikan saat release |

Kita tidak perlu memasukkan JavaScript engine, database, network stack, dan rendering engine ke build pertama. Dependency harus masuk sesuai kebutuhan milestone.

# **16 — Keputusan final blueprint**

# **RushBross**

Independent Rust Browser Shell

Native custom UI

WebView2-only page engine

Minimal Rust shell around the OS web engine

Satu WebView instance untuk tab aktif

Tab background hanya metadata navigation

Memory-conscious architecture

Event-driven & asynchronous

No Chromium rebrand / Electron / engine fallback

Windows-first development

## **Prioritas implementasi**

### **Priority 1**

Startup, native UI, minimal RAM, zero idle CPU.

### **Priority 2**

WebView2 integration, navigation, tab history, URL handling.

### **Priority 3**

Drag-and-drop tab reorder, compatibility, modern web.

### **Priority 4**

JavaScript, security, isolation, advanced APIs.

**Satu keputusan arsitektur yang tidak boleh kita ubah:** UI RushBross harus tetap independen dari web engine. Walaupun nanti engine HTML, CSS, dan JavaScript menjadi sangat kompleks, browser shell harus tetap ringan, responsif, dan bisa berjalan tanpa mengaktifkan engine halaman.

Dengan begitu, RushBross bisa memiliki UI yang cepat meskipun suatu website sedang berat.

Langkah berikutnya yang paling tepat adalah menyusun **RushBross M0 Technical Specification** — desain crate pertama, dependency final, GPU/window initialization, event loop, error handling, profiling, struktur code, dan benchmark yang menjadi dasar sebelum kita menulis kode.

Susun spesifikasi implementasi RushBross M0