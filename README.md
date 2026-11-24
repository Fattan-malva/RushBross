# RushBross

Browser desktop Rust dengan UI native berbasis wgpu/winit dan WebView2 sebagai satu-satunya page engine. Shell tidak membawa engine fallback, parser HTML, atau runtime JS sendiri.

Binary dibangun sebagai aplikasi GUI Windows: tidak membuka console, dan tidak ikut tertutup saat terminal yang menjalankannya ditutup.

## Prasyarat

- Windows 10/11 x64.
- Rust stable toolchain dengan MSVC target. Toolchain otomatis dipilih oleh `rust-toolchain.toml`.
- Visual Studio Build Tools dengan beban kerja **Desktop development with C++** dan Windows SDK.
- Microsoft Edge WebView2 Runtime Evergreen. Biasanya sudah terpasang bersama Microsoft Edge; jika belum, unduh dari https://developer.microsoft.com/microsoft-edge/webview2/.
- Git.

## Setup dari GitHub

```powershell
git clone <repo-url>
cd RushBross
rustup show
cargo fetch
```

`Cargo.lock` sengaja disimpan di repository agar dependency versinya identik untuk semua orang.

## Build

Release paling ringan:

```powershell
cargo build --release -p rushbross
```

Debug untuk development:

```powershell
cargo build -p rushbross
```

Profil release memakai `opt-level = "z"`, `lto = "fat"`, `panic = "abort"`, `strip = "symbols"`, dan `codegen-units = 1` untuk binary sekecil mungkin.

## Menjalankan

Disarankan menjalankan lewat `Start-Process` agar proses benar-benar lepas dari terminal:

```powershell
Start-Process .\target\release\rushbross.exe
```

Atau untuk debug:

```powershell
Start-Process .\target\debug\rushbross.exe
```

Jangan pakai `cargo run` jika ingin menutup terminal tanpa menutup proses browser.

## Pintasan

| Pintasan       | Fungsi                         |
| -------------- | ------------------------------ |
| `Ctrl+T`       | Tab baru                       |
| `Ctrl+W`       | Tutup tab aktif                |
| `Ctrl+Tab`     | Tab berikutnya                 |
| `Ctrl+Shift+Tab` | Tab sebelumnya               |
| `Ctrl+L`       | Fokus alamat                   |
| `Ctrl+R`       | Reload                         |
| `Ctrl+D`       | Tambah/hapus bookmark          |
| `Ctrl+A`       | Pilih teks di alamat           |
| `Ctrl+Q`       | Keluar                         |
| `Alt+Left`     | Kembali                        |
| `Alt+Right`    | Maju                           |

Tab juga bisa di-drag untuk diurut ulang.

## Verifikasi

```powershell
cargo test --workspace
cargo clippy --workspace
cargo fmt --all -- --check
```

## Struktur ringkas

```text
apps/rushbross/          binary Windows GUI + WebView2 host
apps/rushbross/src/newtab.html   halaman tab baru (HTML lokal, ikon Lucide, font ter-embed)
assets/fonts/            font Manrope (dibundel ke binary, dipakai semua teks UI)
crates/rb-tabs/          tab state, history, reorder
crates/rb-bookmarks/     bookmark store
crates/rb-ui/            geometri, theme, text input, frame model
crates/rb-ui-widgets/    shell UI, hit testing, drag tab
crates/rb-ui-renderer/   wgpu + glyphon renderer; ikon Lucide dirender lewat resvg
```

`.gitignore` sudah mengecualikan `target/`, profil data WebView2, binary, log, cache editor, dan file build lain yang tidak perlu masuk GitHub.
