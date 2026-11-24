use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub url: Option<String>,
    /// Favicon PNG dari situs, `None` jika belum ada atau halaman kosong.
    pub favicon: Option<Arc<[u8]>>,
    history: Vec<String>,
    history_index: usize,
}

impl Tab {
    pub fn has_url(&self) -> bool {
        self.url.is_some()
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn history_index(&self) -> usize {
        self.history_index
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    pub fn push_navigation(&mut self, url: String) {
        if self.history.get(self.history_index) == Some(&url) {
            self.url = Some(url);
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(url.clone());
        self.history_index = self.history.len() - 1;
        self.url = Some(url);
    }

    pub fn update_current_url(&mut self, url: String) {
        if self.history.is_empty() {
            self.history.push(url.clone());
            self.history_index = 0;
        } else if let Some(current) = self.history.get_mut(self.history_index) {
            *current = url.clone();
        }
        self.url = Some(url);
    }

    pub fn back(&mut self) -> Option<String> {
        if !self.can_go_back() {
            return None;
        }
        self.history_index -= 1;
        let url = self.history[self.history_index].clone();
        self.url = Some(url.clone());
        Some(url)
    }

    pub fn forward(&mut self) -> Option<String> {
        if !self.can_go_forward() {
            return None;
        }
        self.history_index += 1;
        let url = self.history[self.history_index].clone();
        self.url = Some(url.clone());
        Some(url)
    }
}

#[derive(Debug, Default)]
pub struct TabManager {
    tabs: Vec<Tab>,
    active: Option<TabId>,
    next_id: u64,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        match self.active {
            Some(id) => self.tab_mut(id),
            None => None,
        }
    }

    pub fn activate(&mut self, id: TabId) {
        if self.tab(id).is_some() {
            self.active = Some(id);
        }
    }

    pub fn open(&mut self, title: impl Into<String>, url: Option<String>) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let mut tab = Tab {
            id,
            title: title.into(),
            url: None,
            favicon: None,
            history: Vec::new(),
            history_index: 0,
        };
        if let Some(url) = url {
            tab.push_navigation(url);
        }
        self.tabs.push(tab);
        self.active = Some(id);
        id
    }

    pub fn close(&mut self, id: TabId) -> Option<TabId> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        self.tabs.remove(index);
        let fallback = self
            .tabs
            .get(index.min(self.tabs.len().saturating_sub(1)))
            .map(|tab| tab.id);
        if self.active == Some(id) {
            self.active = fallback;
        }
        fallback.or(self.active)
    }

    pub fn move_to(&mut self, id: TabId, target: usize) {
        let Some(from) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(from);
        let target = target.min(self.tabs.len());
        self.tabs.insert(target, tab);
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    pub fn cycle(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            return;
        }
        let current = self.active.and_then(|id| self.index_of(id)).unwrap_or(0) as isize;
        let count = self.tabs.len() as isize;
        let next = (current + delta + count) % count;
        self.active = Some(self.tabs[next as usize].id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_back_forward() {
        let mut manager = TabManager::new();
        let id = manager.open("first", Some("https://example.com".to_string()));
        let tab = manager.tab_mut(id).unwrap();
        tab.push_navigation("https://example.com/next".to_string());

        assert_eq!(tab.back().as_deref(), Some("https://example.com"));
        assert!(tab.can_go_forward());
        assert_eq!(tab.forward().as_deref(), Some("https://example.com/next"));
    }

    #[test]
    fn move_to_reorders_tabs() {
        let mut manager = TabManager::new();
        let first = manager.open("first", None);
        let second = manager.open("second", None);
        let third = manager.open("third", None);

        manager.move_to(third, 0);
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![third, first, second]);

        manager.move_to(first, 2);
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![third, second, first]);
    }
}
