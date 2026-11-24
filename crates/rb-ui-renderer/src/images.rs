//! Gambar PNG untuk UI, misalnya favicon situs dan ikon halaman baru.
//!
//! PNG di-decode sekali menjadi RGBA (alpha lurus). Glyphon hanya meminta
//! rasterisasi pada ukuran fisik yang dibutuhkan, lalu menyimpannya di atlas
//! warna, sehingga per frame tidak ada decode ulang.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// Bit tinggi pada id glyph untuk membedakan gambar dari ikon Lucide.
pub const IMAGE_ID_FLAG: u16 = 0x8000;

#[derive(Default)]
pub struct ImageRegistry {
    entries: Vec<Entry>,
    lookup: HashMap<(usize, bool), u16>,
}

struct Entry {
    /// Menjaga `Arc` tetap hidup agar alamat yang dipakai sebagai kunci tidak dipakai ulang.
    _source: Arc<[u8]>,
    width: u32,
    height: u32,
    /// RGBA dengan alpha lurus.
    rgba: Vec<u8>,
}

impl ImageRegistry {
    /// Mengembalikan id glyph untuk gambar `source`. `None` jika PNG tidak valid.
    pub fn id(&mut self, source: &Arc<[u8]>, muted: bool) -> Option<u16> {
        let key = (Arc::as_ptr(source) as *const u8 as usize, muted);
        if let Some(id) = self.lookup.get(&key) {
            return Some(*id);
        }
        let (width, height, mut rgba) = decode_png(source)?;
        if muted {
            mute(&mut rgba);
        }
        let index = u16::try_from(self.entries.len()).ok()?;
        if index >= IMAGE_ID_FLAG {
            return None;
        }
        let id = IMAGE_ID_FLAG | index;
        self.entries.push(Entry {
            _source: Arc::clone(source),
            width,
            height,
            rgba,
        });
        self.lookup.insert(key, id);
        Some(id)
    }

    /// Menghasilkan RGBA `width` x `height` untuk id gambar. Mengembalikan `None`
    /// untuk id ikon, sehingga pemanggil bisa mencoba registry lain.
    pub fn rasterize(&self, id: u16, width: u16, height: u16) -> Option<Vec<u8>> {
        if id & IMAGE_ID_FLAG == 0 {
            return None;
        }
        let entry = self.entries.get(usize::from(id & !IMAGE_ID_FLAG))?;
        Some(resample(entry, width.into(), height.into()))
    }
}

fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let size = reader.output_buffer_size();
    let mut buffer = vec![0u8; size];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    if info.width == 0 || info.height == 0 {
        return None;
    }
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Abu-abu dan transparan sebagian. Dipakai untuk ikon halaman baru agar tidak mencolok.
fn mute(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let luma =
            0.299 * f32::from(pixel[0]) + 0.587 * f32::from(pixel[1]) + 0.114 * f32::from(pixel[2]);
        let value = luma.round().clamp(0.0, 255.0) as u8;
        pixel[0] = value;
        pixel[1] = value;
        pixel[2] = value;
        pixel[3] = (f32::from(pixel[3]) * 0.45).round() as u8;
    }
}

/// Rata-rata kotak (box filter) dalam ruang premultiplied, lalu kembali ke alpha lurus.
/// Hasil yang terlalu kecil atau terlalu besar tetap rapi tanpa perlu crate gambar tambahan.
fn resample(entry: &Entry, width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0u8; (width as usize) * (height as usize) * 4];
    if width == 0 || height == 0 {
        return out;
    }
    let (sw, sh) = (entry.width, entry.height);
    for dy in 0..height {
        let y0 = dy * sh / height;
        let y1 = ((dy + 1) * sh / height).max(y0 + 1).min(sh);
        for dx in 0..width {
            let x0 = dx * sw / width;
            let x1 = ((dx + 1) * sw / width).max(x0 + 1).min(sw);
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            let mut count = 0f32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * sw + sx) * 4) as usize;
                    let p = &entry.rgba[i..i + 4];
                    let alpha = f32::from(p[3]) / 255.0;
                    r += f32::from(p[0]) * alpha;
                    g += f32::from(p[1]) * alpha;
                    b += f32::from(p[2]) * alpha;
                    a += alpha;
                    count += 1.0;
                }
            }
            if a <= 0.0 || count <= 0.0 {
                continue;
            }
            let o = ((dy * width + dx) * 4) as usize;
            out[o] = (r / a).round().clamp(0.0, 255.0) as u8;
            out[o + 1] = (g / a).round().clamp(0.0, 255.0) as u8;
            out[o + 2] = (b / a).round().clamp(0.0, 255.0) as u8;
            out[o + 3] = (a / count * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(rgba).unwrap();
        }
        out
    }

    #[test]
    fn decodes_and_downscales_to_requested_size() {
        // 4x4 merah penuh, diturunkan ke 2x2.
        let rgba: Vec<u8> = (0..16).flat_map(|_| [255, 0, 0, 255]).collect();
        let source: Arc<[u8]> = Arc::from(encode(4, 4, &rgba));
        let mut registry = ImageRegistry::default();
        let id = registry.id(&source, false).expect("png decodes");
        assert!(id & IMAGE_ID_FLAG != 0);
        let out = registry.rasterize(id, 2, 2).expect("rasterizes");
        assert_eq!(out.len(), 2 * 2 * 4);
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn muted_images_are_grey_and_translucent() {
        let rgba = [200u8, 40, 40, 255];
        let source: Arc<[u8]> = Arc::from(encode(1, 1, &rgba));
        let mut registry = ImageRegistry::default();
        let id = registry.id(&source, true).unwrap();
        let out = registry.rasterize(id, 1, 1).unwrap();
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);
        assert!(out[3] < 255 && out[3] > 0);
    }

    #[test]
    fn icon_ids_are_not_treated_as_images() {
        let registry = ImageRegistry::default();
        assert!(registry.rasterize(3, 8, 8).is_none());
    }
}
