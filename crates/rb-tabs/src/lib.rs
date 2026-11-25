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
    /// Halaman sedang dimuat; dipakai untuk spinner di ikon tab dan tombol refresh.
    pub loading: bool,
    /// WebView tab sedang memutar audio (YouTube, Discord, dll).
    pub playing_audio: bool,
    /// Tab pinned selalu berada di kiri dan diperkecil jadi favicon saja.
    pub pinned: bool,
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

    pub fn set_loading(&mut self, id: TabId, loading: bool) {
        if let Some(tab) = self.tab_mut(id) {
            tab.loading = loading;
        }
    }

    pub fn set_playing_audio(&mut self, id: TabId, playing: bool) {
        if let Some(tab) = self.tab_mut(id) {
            tab.playing_audio = playing;
        }
    }

    pub fn is_pinned(&self, id: TabId) -> bool {
        self.tab(id).is_some_and(|tab| tab.pinned)
    }

    pub fn toggle_pin(&mut self, id: TabId) -> Option<bool> {
        let pinned = {
            let tab = self.tab_mut(id)?;
            tab.pinned = !tab.pinned;
            tab.pinned
        };
        let mut pinned_tabs = Vec::new();
        let mut normal_tabs = Vec::new();
        for tab in std::mem::take(&mut self.tabs) {
            if tab.pinned {
                pinned_tabs.push(tab);
            } else {
                normal_tabs.push(tab);
            }
        }
        pinned_tabs.extend(normal_tabs);
        self.tabs = pinned_tabs;
        Some(pinned)
    }

    pub fn duplicate(&mut self, id: TabId) -> Option<TabId> {
        let (index, mut tab) = self
            .tabs
            .iter()
            .enumerate()
            .find(|(_, tab)| tab.id == id)
            .map(|(index, tab)| (index, tab.clone()))?;
        let duplicated_id = TabId(self.next_id);
        self.next_id += 1;
        tab.id = duplicated_id;
        tab.pinned = false;
        tab.loading = false;
        tab.playing_audio = false;
        let insert_at = if self.tabs[index].pinned {
            self.tabs.iter().take_while(|tab| tab.pinned).count()
        } else {
            index + 1
        };
        self.tabs.insert(insert_at, tab);
        self.active = Some(duplicated_id);
        Some(duplicated_id)
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
            loading: false,
            playing_audio: false,
            pinned: false,
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
        let pinned = self.tabs[from].pinned;
        let tab = self.tabs.remove(from);
        let pinned_count = self.tabs.iter().filter(|tab| tab.pinned).count();
        let target = if pinned {
            target.min(pinned_count)
        } else {
            target.clamp(pinned_count, self.tabs.len())
        };
        self.tabs.insert(target, tab);
    }

    pub fn move_group(&mut self, ids: &[TabId], target: usize) {
        if ids.is_empty() {
            return;
        }
        let mut group = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(index) = self.tabs.iter().position(|tab| tab.id == *id) {
                group.push(self.tabs.remove(index));
            }
        }
        if group.is_empty() {
            return;
        }
        let pinned_count = self.tabs.iter().filter(|tab| tab.pinned).count();
        let target = if group.iter().any(|tab| !tab.pinned) {
            target.clamp(pinned_count, self.tabs.len())
        } else {
            target.min(pinned_count)
        };
        for tab in group.into_iter().rev() {
            self.tabs.insert(target, tab);
        }
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

    #[test]
    fn move_group_keeps_split_tabs_together() {
        let mut manager = TabManager::new();
        let first = manager.open("first", None);
        let second = manager.open("second", None);
        let third = manager.open("third", None);
        let fourth = manager.open("fourth", None);

        manager.move_group(&[second, fourth], 0);
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![second, fourth, first, third]);
    }

    #[test]
    fn pinned_tabs_stay_at_the_front() {
        let mut manager = TabManager::new();
        let first = manager.open("first", None);
        let second = manager.open("second", None);
        let third = manager.open("third", None);

        assert_eq!(manager.toggle_pin(third), Some(true));
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![third, first, second]);

        manager.move_to(third, 2);
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![third, first, second]);
    }

    #[test]
    fn duplicate_preserves_history_and_activates_copy() {
        let mut manager = TabManager::new();
        let first = manager.open("first", Some("https://example.com".to_string()));
        manager
            .tab_mut(first)
            .unwrap()
            .push_navigation("https://example.com/next".to_string());

        let copy = manager.duplicate(first).unwrap();
        assert_ne!(copy, first);
        assert_eq!(manager.active_id(), Some(copy));
        assert_eq!(
            manager.tab(copy).unwrap().history(),
            &[
                "https://example.com".to_string(),
                "https://example.com/next".to_string(),
            ]
        );
        let order: Vec<TabId> = manager.tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(order, vec![first, copy]);
    }
}
