#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BookmarkId(pub u64);

#[derive(Clone, Debug)]
pub struct Bookmark {
    pub id: BookmarkId,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Default)]
pub struct BookmarkStore {
    items: Vec<Bookmark>,
    next_id: u64,
}

impl BookmarkStore {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
        }
    }

    pub fn items(&self) -> &[Bookmark] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn add(&mut self, title: impl Into<String>, url: impl Into<String>) -> BookmarkId {
        let id = BookmarkId(self.next_id);
        self.next_id += 1;
        self.items.push(Bookmark {
            id,
            title: title.into(),
            url: url.into(),
        });
        id
    }

    pub fn remove(&mut self, id: BookmarkId) {
        self.items.retain(|item| item.id != id);
    }

    pub fn update(&mut self, id: BookmarkId, title: impl Into<String>, url: impl Into<String>) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.title = title.into();
            item.url = url.into();
        }
    }

    pub fn get(&self, id: BookmarkId) -> Option<&Bookmark> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn move_to(&mut self, id: BookmarkId, target: usize) {
        let Some(from) = self.items.iter().position(|item| item.id == id) else {
            return;
        };
        let item = self.items.remove(from);
        let target = target.min(self.items.len());
        self.items.insert(target, item);
    }

    pub fn find_by_url(&self, url: &str) -> Option<&Bookmark> {
        self.items
            .iter()
            .find(|item| item.url.eq_ignore_ascii_case(url))
    }
}
