use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_SIZE: u32 = 256;
const RC_EXE: &str = r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe";

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest_dir.join("../../assets/icons.png");
    println!("cargo:rerun-if-changed={}", source.display());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    let icon_path = out_dir.join("app.ico");
    if let Err(error) = write_icon(&source, &icon_path) {
        println!("cargo:warning=app icon skipped: {error}");
        return;
    }

    let rc_path = out_dir.join("app.rc");
    let resource_path = out_dir.join("app.res");
    let icon_path_forward = icon_path.to_string_lossy().replace('\\', "/");
    let rc_source = format!("1 ICON \"{icon_path_forward}\"\n");
    if let Err(error) = fs::write(&rc_path, rc_source) {
        println!("cargo:warning=app icon rc skipped: {error}");
        return;
    }

    let rc_exe = Path::new(RC_EXE);
    if !rc_exe.exists() {
        println!("cargo:warning=app icon rc.exe not found at {RC_EXE}");
        return;
    }
    let status = Command::new(rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_path)
        .arg(&rc_path)
        .status();
    match status {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bins={}", resource_path.display());
        }
        Ok(status) => println!("cargo:warning=app icon rc failed: {status}"),
        Err(error) => println!("cargo:warning=app icon rc failed: {error}"),
    }
}

fn write_icon(source: &Path, target: &Path) -> Result<(), String> {
    let (rgba, width, height) = read_rgba(source)?;
    let resized = resize_rgba(&rgba, width, height, ICON_SIZE);
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

fn resize_rgba(source: &[u8], width: u32, height: u32, target: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((target * target * 4) as usize);
    for y in 0..target {
        let source_y = (y * height / target).min(height - 1);
        for x in 0..target {
            let source_x = (x * width / target).min(width - 1);
            let index = ((source_y * width + source_x) * 4) as usize;
            out.extend_from_slice(&source[index..index + 4]);
        }
    }
    out
}
