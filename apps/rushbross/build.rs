use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/app_icon.rs"]
mod app_icon;

const ICON_SIZE: u32 = 256;
const KITS_BIN: &str = r"C:\Program Files (x86)\Windows Kits\10\bin";

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest_dir.join("../../assets/icons.png");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed=src/app_icon.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    let icon_path = out_dir.join("app.ico");
    if let Err(error) = write_icon(&source, &icon_path) {
        println!("cargo:warning=app icon skipped: {error}");
        return;
    }

    let Some(rc_exe) = find_rc() else {
        println!("cargo:warning=app icon skipped: rc.exe not found under {KITS_BIN}");
        return;
    };

    let rc_path = out_dir.join("app.rc");
    let resource_path = out_dir.join("app.res");
    let icon_path_forward = icon_path.to_string_lossy().replace('\\', "/");
    if let Err(error) = fs::write(&rc_path, format!("1 ICON \"{icon_path_forward}\"\n")) {
        println!("cargo:warning=app icon rc skipped: {error}");
        return;
    }

    match Command::new(&rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_path)
        .arg(&rc_path)
        .status()
    {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bins={}", resource_path.display());
        }
        Ok(status) => println!("cargo:warning=app icon rc failed: {status}"),
        Err(error) => println!("cargo:warning=app icon rc failed: {error}"),
    }
}

/// Mencari rc.exe dari Windows SDK versi mana pun (ambil yang terbaru), bukan path tetap.
fn find_rc() -> Option<PathBuf> {
    if let Some(path) = env::var_os("RUSHBROSS_RC") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(KITS_BIN)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("x64").join("rc.exe"))
        .filter(|path| path.exists())
        .collect();
    candidates.sort();
    candidates.pop()
}

fn write_icon(source: &Path, target: &Path) -> Result<(), String> {
    let (mut rgba, width, height) = read_rgba(source)?;
    app_icon::round_and_clear_background(&mut rgba, width, height);
    let resized = app_icon::downscale(&rgba, width, height, ICON_SIZE);
    let image = ico::IconImage::from_rgba_data(ICON_SIZE, ICON_SIZE, resized);
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    icon_dir.add_entry(ico::IconDirEntry::encode(&image).map_err(|error| error.to_string())?);
    let file = File::create(target).map_err(|error| error.to_string())?;
    icon_dir.write(file).map_err(|error| error.to_string())
}

fn read_rgba(path: &Path) -> Result<(Vec<u8>, u32, u32), String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    let bytes = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
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
        png::ColorType::Indexed => return Err("indexed PNG is not supported".to_string()),
    };
    Ok((rgba, info.width, info.height))
}
