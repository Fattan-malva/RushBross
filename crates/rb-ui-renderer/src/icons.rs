//! Rasterisasi ikon Lucide (SVG resmi dari paket `lucide-static-svg`) menjadi
//! mask alpha dengan resvg.
//!
//! Ikon tidak lagi digambar sebagai kumpulan segmen garis. Setiap kombinasi
//! ikon + ketebalan goresan diberi id tetap, lalu glyphon merasterisasinya sekali
//! pada ukuran fisik yang diminta dan menyimpannya di atlas. Hasilnya memakai
//! anti-aliasing, round cap, dan round join yang sama seperti spesifikasi Lucide.

use std::collections::HashMap;

use rb_ui::frame::Icon;

/// Ukuran kotak viewBox Lucide. Ketebalan goresan dihitung dalam satuan ini.
const LUCIDE_GRID: f32 = 24.0;
/// Sudut spinner dibulatkan ke 24 langkah agar mask hasil rotasi bisa di-cache.
const ROTATION_STEPS: u16 = 24;

/// Registry id ikon. Id harus stabil antar frame karena dipakai sebagai kunci
/// cache glyph di atlas.
#[derive(Default)]
pub struct IconRegistry {
    entries: Vec<Entry>,
    lookup: HashMap<(Icon, u32, u16), u16>,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    icon: Icon,
    stroke_units: f32,
    rotation_step: u16,
}

impl IconRegistry {
    /// Mengembalikan id glyph untuk `icon` dengan ketebalan goresan `stroke_units`
    /// (dalam satuan grid 24) dan rotasi dalam radian. `None` hanya jika registry penuh.
    pub fn id(&mut self, icon: Icon, stroke_units: f32, rotation: f32) -> Option<u16> {
        let step = ((rotation / std::f32::consts::TAU * ROTATION_STEPS as f32).round() as i32)
            .rem_euclid(ROTATION_STEPS as i32) as u16;
        let key = (icon, (stroke_units * 100.0).round() as u32, step);
        if let Some(id) = self.lookup.get(&key) {
            return Some(*id);
        }
        let id = u16::try_from(self.entries.len()).ok()?;
        self.entries.push(Entry {
            icon,
            stroke_units,
            rotation_step: step,
        });
        self.lookup.insert(key, id);
        Some(id)
    }

    /// Menghasilkan mask alpha `width x height` untuk glyph `id`.
    pub fn rasterize(&self, id: u16, width: u16, height: u16) -> Option<Vec<u8>> {
        let entry = self.entries.get(usize::from(id))?;
        render_mask(
            entry.icon,
            entry.stroke_units,
            width.into(),
            height.into(),
            entry.rotation_step,
        )
    }
}

/// Mengubah ketebalan goresan dari piksel logis menjadi satuan grid 24 Lucide.
pub fn stroke_units(stroke_px: f32, box_size: f32) -> f32 {
    if box_size <= 0.0 {
        return 2.0;
    }
    (stroke_px * LUCIDE_GRID / box_size).max(0.1)
}

fn render_mask(
    icon: Icon,
    stroke_units: f32,
    width: u32,
    height: u32,
    rotation_step: u16,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let source = icon.lucide()?.svg_str();

    // Warna SVG tidak penting: hanya kanal alpha yang dipakai. Glyphon memberi
    // warna teks saat draw, jadi ikon otomatis mengikuti warna yang diminta.
    let mut svg = source.replace("currentColor", "#000000").replace(
        "stroke-width=\"2\"",
        &format!("stroke-width=\"{stroke_units:.3}\""),
    );
    if icon == Icon::StarFilled {
        svg = svg.replace("fill=\"none\"", "fill=\"#000000\"");
    }

    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&svg, &options).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let size = tree.size();
    let centered = resvg::tiny_skia::Transform::from_rotate_at(
        rotation_step as f32 * 360.0 / ROTATION_STEPS as f32,
        width as f32 * 0.5,
        height as f32 * 0.5,
    );
    let transform = centered.pre_concat(resvg::tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    ));
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(pixmap.data().chunks_exact(4).map(|px| px[3]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_rasterizes_to_visible_pixels() {
        let icons = [
            Icon::Search,
            Icon::Lock,
            Icon::Star,
            Icon::StarFilled,
            Icon::Reload,
            Icon::Loader,
            Icon::Volume,
            Icon::Plus,
            Icon::Close,
            Icon::More,
            Icon::Bookmark,
            Icon::Edit,
            Icon::Trash,
            Icon::Minimize,
            Icon::Maximize,
            Icon::Restore,
            Icon::ChevronLeft,
            Icon::ChevronRight,
            Icon::ArrowLeft,
            Icon::ArrowRight,
            Icon::Menu,
            Icon::Profile,
            Icon::Globe,
            Icon::Unsplit,
            Icon::Duplicate,
        ];
        let mut registry = IconRegistry::default();
        for icon in icons {
            let id = registry
                .id(icon, stroke_units(1.6, 16.0), 0.0)
                .expect("icon id");
            let mask = registry.rasterize(id, 32, 32).expect("mask");
            assert_eq!(mask.len(), 32 * 32);
            let lit = mask.iter().filter(|alpha| **alpha > 0).count();
            assert!(lit > 20, "{icon:?} produced only {lit} pixels");
        }
    }

    #[test]
    fn ids_are_stable_for_the_same_icon_and_stroke() {
        let mut registry = IconRegistry::default();
        let first = registry.id(Icon::Search, 2.0, 0.0).unwrap();
        let again = registry.id(Icon::Search, 2.0, 0.0).unwrap();
        let other = registry.id(Icon::Search, 2.5, 0.0).unwrap();
        assert_eq!(first, again);
        assert_ne!(first, other);
        let rotated = registry.id(Icon::Search, 2.0, 0.7).unwrap();
        assert_ne!(first, rotated);
    }

    #[test]
    fn stroke_units_follow_the_24_grid() {
        assert!((stroke_units(2.0, 24.0) - 2.0).abs() < f32::EPSILON);
        assert!((stroke_units(1.0, 12.0) - 2.0).abs() < f32::EPSILON);
    }
}
