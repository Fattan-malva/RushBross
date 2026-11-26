//! Ikon aplikasi dari `assets/icons.png`. Warna asli PNG dipertahankan: kotak hitam,
//! huruf R putih, dan gir. Hanya latar putih di luar kotak membulat yang dibuat transparan.
//! Modul ini juga dipakai `build.rs` untuk ikon exe.

/// Membuat area di luar kotak membulat menjadi transparan. Warna pixel di dalam kotak tidak diubah.
pub fn round_and_clear_background(rgba: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as usize, height as usize);
    // Badan ikon adalah kotak gelap. Kotak ini dipakai sebagai batas kotak membulat.
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let luma = 0.2126 * f32::from(rgba[i])
                + 0.7152 * f32::from(rgba[i + 1])
                + 0.0722 * f32::from(rgba[i + 2]);
            if rgba[i + 3] > 0 && luma < 96.0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let cx = (x0 + x1) as f32 * 0.5;
    let cy = (y0 + y1) as f32 * 0.5;
    let hw = (x1 - x0) as f32 * 0.5;
    let hh = (y1 - y0) as f32 * 0.5;
    let radius = hw.min(hh) * 0.4;
    let (bx, by) = (hw - radius, hh - radius);
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5 - cx;
            let py = y as f32 + 0.5 - cy;
            let qx = px.abs() - bx;
            let qy = py.abs() - by;
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            let i = (y * w + x) * 4;
            rgba[i + 3] = (f32::from(rgba[i + 3]) * coverage).round() as u8;
        }
    }
}

/// Memperkecil RGBA dengan rata-rata kotak dalam ruang premultiplied (tepi tetap halus).
pub fn downscale(rgba: &[u8], width: u32, height: u32, target: u32) -> Vec<u8> {
    let mut out = vec![0u8; (target * target * 4) as usize];
    if width == 0 || height == 0 || target == 0 {
        return out;
    }
    for ty in 0..target {
        let y0 = ty * height / target;
        let y1 = ((ty + 1) * height / target).max(y0 + 1).min(height);
        for tx in 0..target {
            let x0 = tx * width / target;
            let x1 = ((tx + 1) * width / target).max(x0 + 1).min(width);
            let (mut r, mut g, mut b, mut a, mut n) = (0f32, 0f32, 0f32, 0f32, 0f32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * width + sx) * 4) as usize;
                    let alpha = f32::from(rgba[i + 3]) / 255.0;
                    r += f32::from(rgba[i]) * alpha;
                    g += f32::from(rgba[i + 1]) * alpha;
                    b += f32::from(rgba[i + 2]) * alpha;
                    a += alpha;
                    n += 1.0;
                }
            }
            if a <= 0.0 {
                continue;
            }
            let o = ((ty * target + tx) * 4) as usize;
            out[o] = (r / a).round().clamp(0.0, 255.0) as u8;
            out[o + 1] = (g / a).round().clamp(0.0, 255.0) as u8;
            out[o + 2] = (b / a).round().clamp(0.0, 255.0) as u8;
            out[o + 3] = (a / n * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Persegi 10x10: latar putih, kotak hitam di tengah dengan huruf putih di pusat.
    fn sample() -> Vec<u8> {
        let mut rgba = vec![255u8; 10 * 10 * 4];
        for y in 1..9 {
            for x in 1..9 {
                let i = (y * 10 + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        for y in 4..6 {
            for x in 4..6 {
                let i = (y * 10 + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        rgba
    }

    #[test]
    fn keeps_original_colours_and_clears_outside() {
        let mut rgba = sample();
        round_and_clear_background(&mut rgba, 10, 10);
        // Sudut luar: latar putih menjadi transparan.
        assert_eq!(rgba[3], 0);
        // Pusat: huruf putih tetap putih dan tetap opaque.
        let centre = (5 * 10 + 5) * 4;
        assert_eq!(&rgba[centre..centre + 4], &[255, 255, 255, 255]);
        // Badan hitam tetap hitam di tengah tepi.
        let body = (2 * 10 + 2) * 4;
        assert_eq!(&rgba[body..body + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn downscale_keeps_size_and_alpha() {
        let rgba = [0u8, 0, 0, 255].repeat(16);
        let out = downscale(&rgba, 4, 4, 2);
        assert_eq!(out.len(), 2 * 2 * 4);
        assert_eq!(&out[0..4], &[0, 0, 0, 255]);
    }
}
