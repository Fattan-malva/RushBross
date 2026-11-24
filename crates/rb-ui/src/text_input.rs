#[derive(Clone, Debug, Default)]
pub struct TextInput {
    text: String,
    cursor: usize,
    anchor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
    }

    pub fn set_caret(&mut self, index: usize) {
        let index = floor_char_boundary(&self.text, index.min(self.text.len()));
        self.cursor = index;
        self.anchor = index;
    }

    pub fn extend_to(&mut self, index: usize) {
        self.cursor = floor_char_boundary(&self.text, index.min(self.text.len()));
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = 0;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    pub fn selection(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    pub fn selection_text(&self) -> &str {
        let (start, end) = self.selection();
        &self.text[start..end]
    }

    fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let (start, end) = self.selection();
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = start;
        true
    }

    pub fn insert_text(&mut self, value: &str) {
        let cleaned: String = value.chars().filter(|c| !c.is_control()).collect();
        if cleaned.is_empty() {
            return;
        }
        self.delete_selection();
        self.text.insert_str(self.cursor, &cleaned);
        self.cursor += cleaned.len();
        self.anchor = self.cursor;
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        self.anchor = prev;
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_boundary(self.cursor);
        self.text.replace_range(self.cursor..next, "");
        self.anchor = self.cursor;
    }

    pub fn move_left(&mut self, word: bool) {
        self.collapse_selection(false);
        let next = if word {
            self.prev_word(self.cursor)
        } else {
            self.prev_boundary(self.cursor)
        };
        self.cursor = next;
        self.anchor = next;
    }

    pub fn move_right(&mut self, word: bool) {
        self.collapse_selection(true);
        let next = if word {
            self.next_word(self.cursor)
        } else {
            self.next_boundary(self.cursor)
        };
        self.cursor = next;
        self.anchor = next;
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.anchor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
        self.anchor = self.cursor;
    }

    fn collapse_selection(&mut self, to_end: bool) {
        if !self.has_selection() {
            return;
        }
        let (start, end) = self.selection();
        self.cursor = if to_end { end } else { start };
        self.anchor = self.cursor;
    }

    fn prev_boundary(&self, index: usize) -> usize {
        self.text[..index]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_boundary(&self, index: usize) -> usize {
        self.text[index..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| index + i)
            .unwrap_or(self.text.len())
    }

    fn prev_word(&self, index: usize) -> usize {
        let mut position = index;
        while position > 0 {
            let prev = self.prev_boundary(position);
            let ch = self.text[prev..position].chars().next().unwrap_or(' ');
            if !ch.is_whitespace() {
                break;
            }
            position = prev;
        }
        while position > 0 {
            let prev = self.prev_boundary(position);
            let ch = self.text[prev..position].chars().next().unwrap_or(' ');
            if ch.is_whitespace() {
                break;
            }
            position = prev;
        }
        position
    }

    fn next_word(&self, index: usize) -> usize {
        let mut position = index;
        while position < self.text.len() {
            let next = self.next_boundary(position);
            let ch = self.text[position..next].chars().next().unwrap_or(' ');
            if ch.is_whitespace() {
                break;
            }
            position = next;
        }
        while position < self.text.len() {
            let next = self.next_boundary(position);
            let ch = self.text[position..next].chars().next().unwrap_or(' ');
            if !ch.is_whitespace() {
                break;
            }
            position = next;
        }
        position
    }
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Font UI yang dipakai untuk mengukur lebar teks. Harus sama dengan font yang
/// dirender oleh `rb-ui-renderer`, supaya caret, seleksi, dan klik sejajar dengan huruf.
static MEASURE_FONT: &[u8] = include_bytes!("../../../assets/fonts/Manrope-400.ttf");

fn measure_face() -> Option<&'static ttf_parser::Face<'static>> {
    static FACE: std::sync::OnceLock<Option<ttf_parser::Face<'static>>> =
        std::sync::OnceLock::new();
    FACE.get_or_init(|| ttf_parser::Face::parse(MEASURE_FONT, 0).ok())
        .as_ref()
}

/// Lebar satu karakter dalam satuan em (0..1). Dibaca dari metrik font yang dibundel,
/// dan memakai perkiraan kasar hanya jika glyph atau font tidak tersedia.
fn advance_em(ch: char) -> f32 {
    if let Some(face) = measure_face() {
        if let Some(glyph) = face.glyph_index(ch) {
            if let Some(advance) = face.glyph_hor_advance(glyph) {
                return f32::from(advance) / f32::from(face.units_per_em());
            }
        }
    }
    match ch {
        'i' | 'l' | 'I' | 'j' | 't' | 'f' | '.' | ',' | ':' | ';' | '\'' | '!' => 0.28,
        'm' | 'w' | 'M' | 'W' => 0.86,
        ' ' => 0.28,
        'A'..='Z' => 0.68,
        '0'..='9' => 0.56,
        _ if ch.is_ascii() => 0.53,
        _ => 1.0,
    }
}

pub fn char_width(ch: char, size: f32) -> f32 {
    advance_em(ch) * size
}

pub fn text_width(text: &str, size: f32) -> f32 {
    text.chars().map(|ch| char_width(ch, size)).sum()
}

pub fn index_at_offset(text: &str, offset: f32, size: f32) -> usize {
    let mut index = 0usize;
    let mut x = 0.0f32;
    for (byte_index, ch) in text.char_indices() {
        let width = char_width(ch, size);
        if x + width * 0.5 > offset {
            return byte_index;
        }
        x += width;
        index = byte_index + ch.len_utf8();
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_can_be_placed_and_selection_extended() {
        let mut input = TextInput::new();
        input.set_text("hello world");
        input.set_caret(3);
        assert_eq!(input.cursor(), 3);
        assert!(!input.has_selection());
        input.extend_to(8);
        assert_eq!(input.selection(), (3, 8));
        input.backspace();
        assert_eq!(input.text(), "helrld");
    }

    #[test]
    fn measured_widths_come_from_the_bundled_font() {
        let space = char_width(' ', 100.0);
        let wide = char_width('m', 100.0);
        let narrow = char_width('i', 100.0);
        assert!(space > 0.0);
        assert!(wide > narrow);
        assert!(text_width("hello", 14.0) > 0.0);
    }

    #[test]
    fn offset_maps_to_char_boundary() {
        assert_eq!(index_at_offset("hello", 0.0, 10.0), 0);
        assert_eq!(index_at_offset("hello", 100.0, 10.0), 5);
    }
}
