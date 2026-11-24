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
