use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod chrome;

pub use chrome::{ChromeAction, ChromeElement, WindowChrome};

use rb_bookmarks::{Bookmark, BookmarkId, BookmarkStore};
use rb_tabs::{Tab, TabId, TabManager};
use rb_ui::text_input::{index_at_offset, text_width};
use rb_ui::{
    Align, Color, Frame, Icon, IconShape, ImageShape, Rect, RectShape, TextInput, TextShape, Theme,
    Vec2,
};

const TAB_STRIP_HEIGHT: f32 = 40.0;
const TAB_HEIGHT: f32 = 32.0;
const TAB_GAP: f32 = 4.0;
const TAB_MAX_RATIO: f32 = 0.42;
const TAB_MAX_WIDTH: f32 = 200.0;
const TAB_MIN_WIDTH: f32 = 64.0;
const TAB_RADIUS: f32 = 8.0;
/// Ukuran favicon di tab dan jarak ke judul.
const TAB_ICON_SIZE: f32 = 16.0;
const TAB_ICON_GAP: f32 = 8.0;
const PINNED_TAB_WIDTH: f32 = 44.0;
const SPLIT_GAP: f32 = 6.0;
const SPLIT_HEADER_HEIGHT: f32 = 28.0;
const SPLIT_MIN_RATIO: f32 = 0.18;
const SPLIT_GROUP_COLOR: Color = Color::rgb(0x3b, 0x82, 0xf6);
/// Warna border per grup split. Grup baru mengambil warna pertama yang belum dipakai.
const SPLIT_PALETTE: [Color; 5] = [
    Color::rgb(0x3b, 0x82, 0xf6),
    Color::rgb(0xf9, 0x73, 0x16),
    Color::rgb(0x22, 0xc5, 0x5e),
    Color::rgb(0xa8, 0x55, 0xf7),
    Color::rgb(0xec, 0x48, 0x99),
];
const CONTEXT_ITEM_HEIGHT: f32 = 30.0;
const TAB_CONTEXT_WIDTH: f32 = 220.0;

const TOOLBAR_HEIGHT: f32 = 48.0;
const ADDRESS_HEIGHT: f32 = 36.0;
const BOOKMARK_BAR_HEIGHT: f32 = 32.0;
const BOOKMARK_CHIP_HEIGHT: f32 = 24.0;
const POPOVER_WIDTH: f32 = 320.0;
const POPOVER_MAX_LIST_HEIGHT: f32 = 240.0;
const ROW_HEIGHT: f32 = 36.0;
const CONTEXT_WIDTH: f32 = 160.0;
const NAV_BUTTON_SIZE: f32 = 32.0;
const NAV_GAP: f32 = 6.0;
const FULLSCREEN_BAR_HEIGHT: f32 = 56.0;
const ADDRESS_FONT_SIZE: f32 = 14.0;
const WINDOW_BORDER_RADIUS: f32 = 10.0;

/// Alamat internal untuk halaman tab baru. Shell tidak pernah menampilkannya di
/// address bar, dan host WebView memuat halaman beranda lokal untuknya.
pub const NEW_TAB_URL: &str = "rushbross://newtab";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Address,
    BookmarkTitle,
    BookmarkUrl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    Tab(TabId),
    TabClose(TabId),
    NewTabButton,
    SplitDivider(usize),
    SplitPaneHeader(usize),
    SplitPaneClose(usize),
    WindowMinimize,
    WindowMaximizeToggle,
    WindowClose,
    WindowDrag,
    Back,
    Forward,
    Reload,
    ToolbarMenu,
    Profile,
    ToolbarMore,
    Address,
    AddressClear,
    BookmarkStar,
    FullscreenClose,
    BookmarkHint,
    BookmarkChip(BookmarkId),
    BookmarkManage,
    PopoverArea,
    PopoverRow(BookmarkId),
    PopoverEdit(BookmarkId),
    PopoverDelete(BookmarkId),
    EditTitle,
    EditUrl,
    EditCancel,
    EditSave,
    ContextArea,
    ContextOpen,
    ContextEdit,
    ContextDelete,
    TabContextArea,
    TabContextPin(TabId),
    TabContextDuplicate(TabId),
    TabContextReload(TabId),
    TabContextNewTabRight(TabId),
    TabContextSplit(TabId),
    /// Tambahkan tab ke grup yang memuat anchor (anggota grup tujuan).
    TabContextAddToSplit(TabId, TabId),
    TabContextRemoveFromSplit(TabId),
    TabContextCloseSplit(TabId),
    TabContextCloseOthers(TabId),
    TabContextCloseRight(TabId),
    TabContextClose(TabId),
    SplitPickerArea,
    SplitPickerRow(TabId),
    SplitPickerCancel,
    SplitPickerConfirm,
}

impl Element {
    fn is_popover(self) -> bool {
        matches!(
            self,
            Element::BookmarkManage
                | Element::PopoverArea
                | Element::PopoverRow(_)
                | Element::PopoverEdit(_)
                | Element::PopoverDelete(_)
                | Element::EditTitle
                | Element::EditUrl
                | Element::EditCancel
                | Element::EditSave
        )
    }

    fn is_context(self) -> bool {
        matches!(
            self,
            Element::ContextArea
                | Element::ContextOpen
                | Element::ContextEdit
                | Element::ContextDelete
                | Element::TabContextArea
                | Element::TabContextPin(_)
                | Element::TabContextDuplicate(_)
                | Element::TabContextReload(_)
                | Element::TabContextNewTabRight(_)
                | Element::TabContextSplit(_)
                | Element::TabContextAddToSplit(_, _)
                | Element::TabContextRemoveFromSplit(_)
                | Element::TabContextCloseSplit(_)
                | Element::TabContextCloseOthers(_)
                | Element::TabContextCloseRight(_)
                | Element::TabContextClose(_)
                | Element::SplitPickerArea
                | Element::SplitPickerRow(_)
                | Element::SplitPickerCancel
                | Element::SplitPickerConfirm
        )
    }

    fn is_split_picker(self) -> bool {
        matches!(
            self,
            Element::SplitPickerArea
                | Element::SplitPickerRow(_)
                | Element::SplitPickerCancel
                | Element::SplitPickerConfirm
        )
    }
}

#[derive(Clone, Debug)]
struct BookmarkEdit {
    id: BookmarkId,
    title: TextInput,
    url: TextInput,
}

#[derive(Clone, Copy, Debug)]
struct ContextMenu {
    id: BookmarkId,
    position: Vec2,
}

#[derive(Clone, Copy, Debug)]
struct TabContextMenu {
    id: TabId,
    position: Vec2,
}

#[derive(Clone, Debug)]
struct SplitPicker {
    selected: Vec<TabId>,
    position: Vec2,
}

#[derive(Clone, Debug)]
struct SplitView {
    panes: Vec<TabId>,
    ratios: [f32; 2],
    active_pane: usize,
    color: Color,
}

impl SplitView {
    fn new(first: TabId, second: TabId, color: Color) -> Self {
        Self {
            panes: vec![first, second],
            ratios: [0.5, 0.5],
            active_pane: 1,
            color,
        }
    }

    fn rebalance(&mut self) {
        self.ratios = match self.panes.len() {
            3 => [1.0 / 3.0, 2.0 / 3.0],
            _ => [0.5, 0.5],
        };
        self.active_pane = self.active_pane.min(self.panes.len().saturating_sub(1));
    }

    fn boundaries(&self) -> &[f32] {
        &self.ratios[..self.panes.len().saturating_sub(1)]
    }

    fn pane_rects(&self, content: Rect) -> Vec<Rect> {
        let count = self.panes.len();
        if count <= 1 {
            return vec![content];
        }
        let usable = (content.width - SPLIT_GAP * (count as f32 - 1.0)).max(0.0);
        let mut edges = Vec::with_capacity(count + 1);
        edges.push(0.0);
        edges.extend_from_slice(self.boundaries());
        edges.push(1.0);
        let mut rects = Vec::with_capacity(count);
        let mut x = content.x;
        for index in 0..count {
            let width = usable * (edges[index + 1] - edges[index]).max(0.0);
            rects.push(Rect::new(x, content.y, width, content.height));
            x += width;
            if index + 1 < count {
                x += SPLIT_GAP;
            }
        }
        rects
    }

    fn divider_rects(&self, content: Rect) -> Vec<Rect> {
        let panes = self.pane_rects(content);
        panes
            .iter()
            .take(panes.len().saturating_sub(1))
            .map(|pane| Rect::new(pane.right(), content.y, SPLIT_GAP, content.height))
            .collect()
    }

    fn set_divider(&mut self, index: usize, ratio: f32) {
        let count = self.panes.len();
        if count < 2 {
            return;
        }
        let ratio = ratio.clamp(SPLIT_MIN_RATIO, 1.0 - SPLIT_MIN_RATIO);
        if count == 2 {
            self.ratios[0] = ratio;
            return;
        }
        if index == 0 {
            let ceiling = self.ratios[1] - SPLIT_MIN_RATIO;
            self.ratios[0] = ratio.min(ceiling).max(SPLIT_MIN_RATIO);
        } else {
            let floor = self.ratios[0] + SPLIT_MIN_RATIO;
            self.ratios[1] = ratio.max(floor).min(1.0 - SPLIT_MIN_RATIO);
        }
    }
}

#[derive(Clone, Debug)]
struct TabMenuItem {
    label: String,
    element: Element,
    icon: Icon,
    destructive: bool,
    separator_before: bool,
}

#[derive(Clone, Debug)]
struct TabDrag {
    ids: Vec<TabId>,
    start: Vec2,
    current: Vec2,
    visual_x: f32,
    origin_x: f32,
    grab_offset: f32,
    order: Vec<TabId>,
    target: usize,
    dragging: bool,
    slots: Vec<f32>,
    settled: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Layout {
    viewport: Rect,
    manage_button: Rect,
    address_text_x: f32,
    address_scroll: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKind {
    Load,
    Reload,
}

#[derive(Clone, Debug)]
pub struct NavigationRequest {
    pub tab_id: TabId,
    pub url: String,
    pub title: String,
    pub kind: NavigationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorRole {
    Default,
    Pointer,
    Text,
}

pub struct BrowserShell {
    theme: Theme,
    tabs: TabManager,
    bookmarks: BookmarkStore,
    address: TextInput,
    focus: Option<Focus>,
    address_dragging: bool,
    fullscreen: bool,
    show_bookmarks: bool,
    editing: Option<BookmarkEdit>,
    context_menu: Option<ContextMenu>,
    tab_context_menu: Option<TabContextMenu>,
    split_picker: Option<SplitPicker>,
    /// Semua grup split. Satu tab hanya boleh menjadi anggota satu grup.
    splits: Vec<SplitView>,
    split_drag: Option<usize>,
    pointer: Vec2,
    hover: Option<Element>,
    hits: Vec<(Element, Rect)>,
    size: Vec2,
    layout: Layout,
    tab_rects: Vec<(TabId, Rect)>,
    tab_drag: Option<TabDrag>,
    chrome: WindowChrome,
    maximized: bool,
    pending_chrome: Option<ChromeAction>,
    pending_fullscreen_exit: bool,
    pending: Option<NavigationRequest>,
    closed_tabs: Vec<TabId>,
    spin_start: Option<Instant>,
}

impl Default for BrowserShell {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserShell {
    pub fn new() -> Self {
        let mut tabs = TabManager::new();
        tabs.open("Tab baru", Some(NEW_TAB_URL.to_string()));
        let mut shell = Self {
            theme: Theme::dark(),
            tabs,
            bookmarks: BookmarkStore::new(),
            address: TextInput::new(),
            focus: None,
            address_dragging: false,
            fullscreen: false,
            show_bookmarks: false,
            editing: None,
            context_menu: None,
            tab_context_menu: None,
            split_picker: None,
            splits: Vec::new(),
            split_drag: None,
            pointer: Vec2::new(-1000.0, -1000.0),
            hover: None,
            hits: Vec::new(),
            size: Vec2::new(0.0, 0.0),
            layout: Layout::default(),
            tab_rects: Vec::new(),
            tab_drag: None,
            chrome: WindowChrome::layout(0.0, 0.0),
            maximized: false,
            pending_chrome: None,
            pending_fullscreen_exit: false,
            pending: None,
            closed_tabs: Vec::new(),
            spin_start: None,
        };
        shell.sync_address_to_active();
        shell.request_active_navigation();
        shell
    }

    pub fn render(&mut self, width: f32, height: f32) -> Frame {
        self.build(width, height)
    }

    /// Memperbarui posisi pointer. Mengembalikan `true` jika tampilan perlu digambar
    /// ulang, yaitu saat hover berubah atau sedang ada seleksi/drag.
    pub fn pointer_moved(&mut self, position: Vec2) -> bool {
        let previous_hover = self.hover;
        self.pointer = position;
        // Hit area dari frame terakhir masih berlaku, jadi hover bisa langsung diperbarui
        // tanpa membangun frame dua kali.
        self.hover = self.hit_at(position).map(|(element, _)| element);
        let mut dirty = self.hover != previous_hover;
        if let Some(index) = self.split_drag {
            self.update_split_divider(index, position.x);
            return true;
        }
        if self.address_dragging && self.focus == Some(Focus::Address) {
            let text = self.address.text().to_string();
            let offset =
                (position.x - self.layout.address_text_x + self.layout.address_scroll).max(0.0);
            let index = index_at_offset(&text, offset, ADDRESS_FONT_SIZE);
            self.address.extend_to(index);
            return true;
        }
        let Some(mut drag) = self.tab_drag.take() else {
            return dirty;
        };
        dirty = true;
        drag.current = position;
        let moved = (position.x - drag.start.x).hypot(position.y - drag.start.y);
        if !drag.dragging && moved > 4.0 {
            drag.dragging = true;
        }
        if drag.dragging {
            let order = self
                .tabs
                .tabs()
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>();
            let slots: HashMap<TabId, f32> = order
                .iter()
                .copied()
                .zip(drag.slots.iter().copied())
                .collect();
            let mut target = 0usize;
            for id in &order {
                if drag.ids.contains(id) {
                    continue;
                }
                let center = slots.get(id).copied().unwrap_or(position.x);
                if position.x < center {
                    break;
                }
                target += 1;
            }
            // Tab yang bukan anggota grup tidak boleh masuk ke tengah grup. Jika titik
            // jatuhnya di dalam grup, tab ditempel ke sisi terdekat grup.
            // Berlaku untuk setiap grup yang tidak sedang di-drag utuh.
            for split in &self.splits {
                let is_group_drag = split.panes.len() >= 2
                    && split.panes.iter().all(|pane| drag.ids.contains(pane));
                if is_group_drag {
                    continue;
                }
                let rest: Vec<TabId> = order
                    .iter()
                    .copied()
                    .filter(|id| !drag.ids.contains(id))
                    .collect();
                let members: Vec<usize> = rest
                    .iter()
                    .enumerate()
                    .filter(|(_, id)| split.panes.contains(id))
                    .map(|(index, _)| index)
                    .collect();
                if let (Some(&lo), Some(&hi)) = (members.first(), members.last()) {
                    if target > lo && target <= hi {
                        target = if target - lo <= hi + 1 - target {
                            lo
                        } else {
                            hi + 1
                        };
                    }
                }
            }
            drag.order = order;
            drag.target = target;
            self.tabs.move_group(&drag.ids, target);
            self.sync_split_order();
        }
        self.tab_drag = Some(drag);
        dirty
    }

    pub fn left_click(&mut self, position: Vec2) {
        self.pointer = position;
        let hit = self.hit_at(position);

        self.address_dragging = false;
        self.split_drag = None;
        if let Some((Element::SplitDivider(index), _)) = hit {
            self.split_drag = Some(index);
        }
        if let Some((Element::Tab(id), rect)) = hit {
            let mut ids = self
                .split_of(id)
                .map(|split| split.panes.clone())
                .unwrap_or_else(|| vec![id]);
            if !ids.contains(&id) {
                ids.push(id);
            }
            ids.sort_by_key(|id| self.tabs.index_of(*id).unwrap_or(usize::MAX));
            self.tab_drag = Some(TabDrag {
                ids,
                start: position,
                current: position,
                visual_x: rect.x,
                origin_x: rect.x,
                grab_offset: position.x - rect.x,
                order: self.tabs.tabs().iter().map(|tab| tab.id).collect(),
                target: self.tabs.index_of(id).unwrap_or(0),
                dragging: false,
                slots: Vec::new(),
                settled: true,
            });
        }

        if let Some(element) = hit.map(|(element, _)| element) {
            if self.split_picker.is_some() && !element.is_split_picker() {
                self.split_picker = None;
            }
        }

        if let Some((Element::Address, rect)) = hit {
            self.focus = Some(Focus::Address);
            let text = self.address.text().to_string();
            let offset =
                (position.x - self.layout.address_text_x + self.layout.address_scroll).max(0.0);
            let index = index_at_offset(&text, offset, ADDRESS_FONT_SIZE);
            self.address.set_caret(index);
            let _ = rect;
            self.context_menu = None;
            self.split_picker = None;
            if self.show_bookmarks {
                self.show_bookmarks = false;
                self.editing = None;
            }
            self.address_dragging = true;
            return;
        }

        if let Some(element) = hit.map(|(element, _)| element) {
            if self.context_menu.is_some() && !element.is_context() {
                self.context_menu = None;
            }
            if self.tab_context_menu.is_some() && !element.is_context() {
                self.tab_context_menu = None;
            }
            if self.show_bookmarks && !element.is_popover() {
                self.show_bookmarks = false;
                self.editing = None;
                self.blur();
            }
            self.activate(element);
        } else {
            self.context_menu = None;
            self.tab_context_menu = None;
            self.split_picker = None;
            if self.show_bookmarks {
                self.show_bookmarks = false;
                self.editing = None;
            }
            self.blur();
        }
    }

    pub fn left_released(&mut self, position: Vec2) {
        self.pointer = position;
        self.tab_drag = None;
        self.address_dragging = false;
        self.split_drag = None;
    }

    pub fn animate_tabs(&mut self) -> bool {
        self.tab_drag
            .as_ref()
            .filter(|drag| drag.dragging && !drag.settled)
            .is_some()
    }

    pub fn right_click(&mut self, position: Vec2) {
        self.pointer = position;
        let hit = self.hit_at(position).map(|(element, _)| element);
        match hit {
            Some(Element::BookmarkChip(id)) | Some(Element::PopoverRow(id)) => {
                self.show_bookmarks = false;
                self.editing = None;
                self.tab_context_menu = None;
                self.split_picker = None;
                self.context_menu = Some(ContextMenu { id, position });
                self.blur();
            }
            Some(Element::Tab(id)) | Some(Element::TabClose(id)) => {
                self.context_menu = None;
                self.split_picker = None;
                self.show_bookmarks = false;
                self.editing = None;
                self.tab_context_menu = Some(TabContextMenu { id, position });
                self.blur();
            }
            _ => {
                self.context_menu = None;
                self.tab_context_menu = None;
                self.split_picker = None;
            }
        }
    }

    pub fn is_editing(&self) -> bool {
        self.focus.is_some()
    }

    pub fn set_maximized(&mut self, maximized: bool) {
        self.maximized = maximized;
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
        self.pending_fullscreen_exit = false;
        if fullscreen {
            self.show_bookmarks = false;
            self.context_menu = None;
            self.tab_context_menu = None;
            self.split_picker = None;
            self.blur();
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn cursor_role(&self, position: Vec2) -> CursorRole {
        if self.fullscreen {
            return if self
                .hits
                .iter()
                .rev()
                .find(|(_, rect)| rect.contains(position))
                .is_some_and(|(element, _)| *element == Element::FullscreenClose)
            {
                CursorRole::Pointer
            } else {
                CursorRole::Default
            };
        }
        match self
            .hits
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(position))
            .map(|(element, _)| *element)
        {
            Some(Element::Address | Element::EditTitle | Element::EditUrl) => CursorRole::Text,
            Some(Element::WindowDrag) => CursorRole::Default,
            Some(_) => CursorRole::Pointer,
            None => CursorRole::Default,
        }
    }

    pub fn hit_chrome(&self, position: Vec2) -> Option<ChromeElement> {
        self.chrome.hit(position)
    }

    pub fn take_chrome_action(&mut self) -> Option<ChromeAction> {
        self.pending_chrome.take()
    }

    pub fn take_fullscreen_exit(&mut self) -> bool {
        std::mem::take(&mut self.pending_fullscreen_exit)
    }

    pub fn insert_text(&mut self, text: &str) {
        match self.focus {
            Some(Focus::Address) => self.address.insert_text(text),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.insert_text(text);
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.insert_text(text);
                }
            }
            None => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(Focus::Address) => self.address.backspace(),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.backspace();
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.backspace();
                }
            }
            None => {}
        }
    }

    pub fn delete_forward(&mut self) {
        match self.focus {
            Some(Focus::Address) => self.address.delete_forward(),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.delete_forward();
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.delete_forward();
                }
            }
            None => {}
        }
    }

    pub fn move_left(&mut self, word: bool) {
        match self.focus {
            Some(Focus::Address) => self.address.move_left(word),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.move_left(word);
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.move_left(word);
                }
            }
            None => {}
        }
    }

    pub fn move_right(&mut self, word: bool) {
        match self.focus {
            Some(Focus::Address) => self.address.move_right(word),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.move_right(word);
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.move_right(word);
                }
            }
            None => {}
        }
    }

    pub fn move_home(&mut self) {
        self.field_mut(|field| field.move_home());
    }

    pub fn move_end(&mut self) {
        self.field_mut(|field| field.move_end());
    }

    pub fn select_all(&mut self) {
        self.field_mut(|field| field.select_all());
    }

    pub fn focus_address(&mut self) {
        self.focus = Some(Focus::Address);
        self.address.select_all();
    }

    pub fn submit(&mut self) {
        let raw = self.address.text().trim().to_string();
        if raw.is_empty() {
            return;
        }
        let url = resolve_target(&raw);
        let title = host_from_url(&url);
        self.navigate(url, title);
        self.focus = None;
    }

    pub fn escape(&mut self) {
        if self.split_picker.is_some() {
            self.split_picker = None;
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            return;
        }
        if self.tab_context_menu.is_some() {
            self.tab_context_menu = None;
            return;
        }
        if self.show_bookmarks {
            self.show_bookmarks = false;
            self.editing = None;
            return;
        }
        self.blur();
    }

    pub fn new_tab(&mut self) {
        let id = self.tabs.open("Tab baru", Some(NEW_TAB_URL.to_string()));
        self.begin_load(
            id,
            NEW_TAB_URL.to_string(),
            "Tab baru".to_string(),
            NavigationKind::Load,
        );
        self.activate_tab(id);
        // Tab baru langsung siap diketik: address bar aktif dan teks terpilih.
        self.focus_address();
        self.show_bookmarks = false;
    }

    pub fn close_active_tab(&mut self) {
        if let Some(id) = self.focused_tab_id() {
            self.close_tab(id);
        }
    }

    pub fn cycle_tabs(&mut self, delta: isize) {
        if self.tabs.tabs().len() < 2 {
            return;
        }
        // Ctrl+Tab selalu berputar di urutan tab strip, sama seperti yang terlihat user.
        self.tabs.cycle(delta);
        if let Some(id) = self.tabs.active_id() {
            self.activate_tab(id);
        }
    }

    pub fn toggle_bookmark(&mut self) {
        let Some(tab) = self
            .focused_tab_id()
            .and_then(|id| self.tabs.tab(id).cloned())
        else {
            return;
        };
        let Some(url) = tab.url.clone() else {
            return;
        };
        if is_blank_url(&url) {
            return;
        }
        let normalized = normalize_url(&url).unwrap_or(url);
        if let Some(existing) = self.bookmarks.find_by_url(&normalized) {
            let id = existing.id;
            self.bookmarks.remove(id);
            if self.editing.as_ref().map(|edit| edit.id) == Some(id) {
                self.editing = None;
            }
            return;
        }
        self.bookmarks.add(tab.title.clone(), normalized);
    }

    pub fn take_navigation(&mut self) -> Option<NavigationRequest> {
        self.pending.take()
    }

    pub fn take_closed_tabs(&mut self) -> Vec<TabId> {
        std::mem::take(&mut self.closed_tabs)
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.tabs.active_id()
    }

    /// Tab yang sedang difokuskan WebView. Dalam split view ini adalah pane aktif.
    pub fn focused_tab_id(&self) -> Option<TabId> {
        if !self.split_is_shown() {
            return self.tabs.active_id();
        }
        self.shown_split()
            .and_then(|split| split.panes.get(split.active_pane).copied())
            .or_else(|| self.tabs.active_id())
    }

    /// Grup yang memuat tab `id`, jika ada.
    fn split_of(&self, id: TabId) -> Option<&SplitView> {
        self.splits.iter().find(|split| split.panes.contains(&id))
    }

    fn split_index_of(&self, id: TabId) -> Option<usize> {
        self.splits
            .iter()
            .position(|split| split.panes.contains(&id))
    }

    /// Indeks grup yang tampil: grup milik tab aktif, jika grup itu punya minimal dua pane.
    fn shown_split_index(&self) -> Option<usize> {
        let active = self.tabs.active_id()?;
        let index = self.split_index_of(active)?;
        (self.splits[index].panes.len() >= 2).then_some(index)
    }

    fn shown_split(&self) -> Option<&SplitView> {
        self.shown_split_index().map(|index| &self.splits[index])
    }

    fn shown_split_mut(&mut self) -> Option<&mut SplitView> {
        let index = self.shown_split_index()?;
        self.splits.get_mut(index)
    }

    /// Warna pertama dari palet yang belum dipakai grup lain.
    fn next_split_color(&self) -> Color {
        SPLIT_PALETTE
            .iter()
            .copied()
            .find(|color| self.splits.iter().all(|split| split.color != *color))
            .unwrap_or(SPLIT_PALETTE[self.splits.len() % SPLIT_PALETTE.len()])
    }

    /// Rectangle WebView per pane. Header pane dan jarak antar pane tidak ikut.
    pub fn webview_layout(&self, content: Rect) -> Vec<(TabId, Rect)> {
        if let Some(split) = self.shown_split() {
            return split
                .pane_rects(content)
                .into_iter()
                .zip(split.panes.iter().copied())
                .map(|(mut rect, tab_id)| {
                    rect.y += SPLIT_HEADER_HEIGHT;
                    rect.height = (rect.height - SPLIT_HEADER_HEIGHT).max(0.0);
                    (tab_id, rect)
                })
                .collect();
        }
        self.tabs
            .active_id()
            .map(|tab_id| vec![(tab_id, content)])
            .unwrap_or_default()
    }

    fn update_split_divider(&mut self, index: usize, x: f32) {
        let content = self.content_rect(self.size.x, self.size.y);
        if content.width <= 1.0 {
            return;
        }
        let ratio = (x - content.x) / content.width;
        if let Some(split) = self.shown_split_mut() {
            split.set_divider(index, ratio);
        }
    }

    /// Merapikan semua grup agar anggotanya berdampingan di tab strip. Merapikan satu
    /// grup bisa menggeser grup lain, jadi diulang sampai stabil.
    fn sync_split_order(&mut self) {
        for _ in 0..=self.splits.len() {
            let mut moved = false;
            for index in 0..self.splits.len() {
                moved |= self.sync_split_at(index);
            }
            if !moved {
                break;
            }
        }
    }

    /// Merapikan satu grup. Mengembalikan `true` jika tab strip harus dipindah.
    fn sync_split_at(&mut self, index: usize) -> bool {
        let split = &self.splits[index];
        let active_pane = split.panes.get(split.active_pane).copied();
        let mut ordered = split.panes.clone();
        ordered.sort_by_key(|id| self.tabs.index_of(*id).unwrap_or(usize::MAX));
        let indices: Vec<usize> = ordered
            .iter()
            .filter_map(|id| self.tabs.index_of(*id))
            .collect();
        let contiguous =
            indices.len() == ordered.len() && indices.windows(2).all(|pair| pair[1] == pair[0] + 1);
        let mut moved = false;
        if !contiguous && !indices.is_empty() {
            self.tabs.move_group(&ordered, indices[0]);
            moved = true;
        }
        let split = &mut self.splits[index];
        split.panes = ordered;
        split.active_pane = active_pane
            .and_then(|id| split.panes.iter().position(|pane| *pane == id))
            .unwrap_or(0);
        moved
    }

    fn open_split_picker(&mut self, suggested: &[TabId], position: Vec2) {
        let mut selected: Vec<TabId> = self
            .tabs
            .tabs()
            .iter()
            .map(|tab| tab.id)
            .filter(|id| suggested.contains(id))
            .collect();
        if selected.is_empty() {
            if let Some(id) = self.tabs.active_id() {
                selected.push(id);
            }
        }
        selected.truncate(3);
        self.split_picker = Some(SplitPicker { selected, position });
        self.tab_context_menu = None;
    }

    fn toggle_split_picker(&mut self, id: TabId) {
        let order: HashMap<TabId, usize> = self
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (tab.id, index))
            .collect();
        let Some(picker) = self.split_picker.as_mut() else {
            return;
        };
        if let Some(index) = picker.selected.iter().position(|selected| *selected == id) {
            picker.selected.remove(index);
            return;
        }
        if picker.selected.len() < 3 {
            picker.selected.push(id);
            picker
                .selected
                .sort_by_key(|id| order.get(id).copied().unwrap_or(usize::MAX));
        }
    }

    /// Membuat grup split baru dari tab yang dipilih. Tab yang sudah ada di grup lain
    /// dilepas dari grup lamanya lebih dulu.
    fn confirm_split_picker(&mut self) {
        let order: HashMap<TabId, usize> = self
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (tab.id, index))
            .collect();
        let Some(picker) = self.split_picker.as_ref() else {
            return;
        };
        let mut ids = picker.selected.clone();
        ids.sort_by_key(|id| order.get(id).copied().unwrap_or(usize::MAX));
        ids.truncate(3);
        if ids.len() < 2 {
            return;
        }
        self.split_picker = None;
        for id in &ids {
            if self.tabs.is_pinned(*id) {
                self.tabs.toggle_pin(*id);
            }
            self.detach_from_splits(*id);
        }
        let color = self.next_split_color();
        let mut split = SplitView::new(ids[0], ids[1], color);
        if let Some(third) = ids.get(2) {
            split.panes.push(*third);
            split.active_pane = 2;
        }
        split.rebalance();
        self.splits.push(split);
        self.sync_split_order();
        if let Some(id) = ids.last() {
            self.activate_tab(*id);
        }
        self.tab_context_menu = None;
    }

    /// Menambahkan tab ke grup yang memuat `anchor`. Grup yang penuh atau tab yang
    /// sudah di grup itu tidak mengubah apa pun.
    fn add_to_split(&mut self, id: TabId, anchor: TabId) {
        self.tab_context_menu = None;
        self.split_picker = None;
        if self
            .split_of(id)
            .is_some_and(|split| split.panes.contains(&anchor))
        {
            return;
        }
        if self
            .split_of(anchor)
            .is_none_or(|split| split.panes.len() >= 3)
        {
            return;
        }
        self.detach_from_splits(id);
        let Some(index) = self.split_index_of(anchor) else {
            return;
        };
        let split = &mut self.splits[index];
        split.panes.push(id);
        split.rebalance();
        self.sync_split_order();
        self.activate_tab(id);
    }

    /// Mengaktifkan tab tanpa mengubah keanggotaan split. Tab di luar grup tampil
    /// sendirian; tab anggota grup memunculkan kembali split view di posisi pane-nya.
    fn activate_tab(&mut self, id: TabId) {
        if self.tabs.tab(id).is_none() {
            return;
        }
        self.tabs.activate(id);
        self.sync_active_pane();
        self.sync_address_to_active();
        self.focus = None;
    }

    /// Menyamakan pane aktif setiap grup dengan tab yang aktif di tab strip.
    fn sync_active_pane(&mut self) {
        let active = self.tabs.active_id();
        for split in self.splits.iter_mut() {
            if let Some(index) = split.panes.iter().position(|pane| Some(*pane) == active) {
                split.active_pane = index;
            }
        }
    }

    /// Split view hanya ditampilkan saat tab aktif adalah anggota grup. Tab lain
    /// tampil penuh, tetapi semua grup tetap utuh di tab strip.
    fn split_is_shown(&self) -> bool {
        self.shown_split_index().is_some()
    }

    fn remove_from_split(&mut self, id: TabId) {
        self.forget_split_tab(id);
        self.tab_context_menu = None;
    }

    fn remove_split_pane(&mut self, index: usize) {
        let target = self
            .shown_split()
            .and_then(|split| split.panes.get(index).copied());
        if let Some(id) = target {
            self.forget_split_tab(id);
        }
    }

    /// Membubarkan grup yang memuat `id`. Tab-tabnya tetap terbuka.
    fn close_split(&mut self, id: TabId) {
        self.splits.retain(|split| !split.panes.contains(&id));
        self.split_drag = None;
        self.tab_context_menu = None;
        self.split_picker = None;
        self.sync_address_to_active();
    }

    /// Melepas tab dari grup mana pun tanpa mengubah tab aktif. Grup yang tersisa
    /// kurang dari dua pane dibubarkan.
    fn detach_from_splits(&mut self, id: TabId) {
        for split in self.splits.iter_mut() {
            if split.panes.contains(&id) {
                split.panes.retain(|pane| *pane != id);
                split.rebalance();
            }
        }
        self.splits.retain(|split| split.panes.len() >= 2);
    }

    /// Melepas tab dari grupnya dan memilih tetangga sebagai pane aktif jika tab itu
    /// sedang aktif.
    fn forget_split_tab(&mut self, id: TabId) {
        let Some(index) = self.split_index_of(id) else {
            return;
        };
        let split = &mut self.splits[index];
        let was_active = split.panes.get(split.active_pane).copied();
        let Some(position) = split.panes.iter().position(|pane| *pane == id) else {
            return;
        };
        split.panes.remove(position);
        if split.panes.len() <= 1 {
            self.splits.remove(index);
            self.split_drag = None;
            self.split_picker = None;
            return;
        }
        split.rebalance();
        // Jika tab yang dilepas sedang aktif, pilih tetangganya; selain itu pane aktif
        // tetap sama.
        let focus = if was_active == Some(id) {
            split
                .panes
                .get(position.min(split.panes.len() - 1))
                .copied()
        } else {
            was_active
        };
        self.sync_split_order();
        if let Some(id) = focus {
            self.activate_tab(id);
        }
    }

    fn focus_pane(&mut self, index: usize) {
        let id = {
            let Some(split) = self.shown_split_mut() else {
                return;
            };
            if index >= split.panes.len() {
                return;
            }
            split.active_pane = index;
            split.panes[index]
        };
        self.tabs.activate(id);
        self.sync_address_to_active();
        self.focus = None;
        self.tab_context_menu = None;
    }

    fn duplicate_tab(&mut self, id: TabId) -> Option<TabId> {
        let tab = self.tabs.tab(id).cloned()?;
        let duplicated = self.tabs.duplicate(id)?;
        let url = tab.url.clone().unwrap_or_else(|| NEW_TAB_URL.to_string());
        self.begin_load(duplicated, url, tab.title, NavigationKind::Load);
        self.sync_address_to_active();
        self.tab_context_menu = None;
        Some(duplicated)
    }

    fn reload_tab(&mut self, id: TabId) {
        let Some(tab) = self.tabs.tab(id).cloned() else {
            return;
        };
        let url = tab.url.clone().unwrap_or_else(|| NEW_TAB_URL.to_string());
        self.begin_load(id, url, tab.title, NavigationKind::Reload);
        self.tab_context_menu = None;
    }

    fn new_tab_after(&mut self, id: TabId) {
        let new_id = self.tabs.open("Tab baru", Some(NEW_TAB_URL.to_string()));
        if let Some(index) = self.tabs.index_of(id) {
            self.tabs.move_to(new_id, index + 1);
        }
        self.begin_load(
            new_id,
            NEW_TAB_URL.to_string(),
            "Tab baru".to_string(),
            NavigationKind::Load,
        );
        self.activate_tab(new_id);
        self.tab_context_menu = None;
    }

    fn close_other_tabs(&mut self, id: TabId) {
        let ids: Vec<TabId> = self
            .tabs
            .tabs()
            .iter()
            .map(|tab| tab.id)
            .filter(|tab_id| *tab_id != id)
            .collect();
        for other in ids {
            self.close_tab(other);
        }
        self.tab_context_menu = None;
    }

    fn close_tabs_to_right(&mut self, id: TabId) {
        let Some(index) = self.tabs.index_of(id) else {
            return;
        };
        let ids: Vec<TabId> = self
            .tabs
            .tabs()
            .iter()
            .skip(index + 1)
            .map(|tab| tab.id)
            .collect();
        for other in ids {
            self.close_tab(other);
        }
        self.tab_context_menu = None;
    }

    /// Spinner harus terus beranimasi selama halaman dimuat dan sesaat setelahnya.
    pub fn has_animation(&self) -> bool {
        self.spin_start
            .is_some_and(|start| start.elapsed() < Duration::from_millis(900))
            || self.tabs.tabs().iter().any(|tab| tab.loading)
            || self
                .tab_drag
                .as_ref()
                .is_some_and(|drag| drag.dragging && !drag.settled)
    }

    fn spin_rotation(&self) -> f32 {
        let Some(start) = self.spin_start else {
            return 0.0;
        };
        let elapsed = start.elapsed().as_secs_f32();
        if elapsed > 1.2
            && !self
                .focused_tab_id()
                .and_then(|id| self.tabs.tab(id))
                .is_some_and(|tab| tab.loading)
        {
            return 0.0;
        }
        elapsed * 7.0
    }

    pub fn content_rect(&self, width: f32, height: f32) -> Rect {
        let (width, height) = (width.max(0.0), height.max(0.0));
        if self.fullscreen {
            return Rect::new(0.0, 0.0, width, height);
        }
        let top = TAB_STRIP_HEIGHT
            + TOOLBAR_HEIGHT
            + if self.bookmark_bar_visible() {
                BOOKMARK_BAR_HEIGHT
            } else {
                0.0
            };
        Rect::new(0.0, top, width, (height - top).max(0.0))
    }

    fn bookmark_bar_visible(&self) -> bool {
        if self.fullscreen || self.bookmarks.is_empty() {
            return false;
        }
        self.focused_tab_id()
            .and_then(|id| self.tabs.tab(id))
            .and_then(|tab| tab.url.as_deref())
            .map(is_blank_url)
            .unwrap_or(true)
    }

    /// Favicon dari WebView2. `None` diabaikan supaya ikon tab tidak hilang saat
    /// halaman mulai dimuat; ikon lama dipakai sampai favicon baru tersedia.
    pub fn webview_favicon(&mut self, tab_id: TabId, favicon: Option<Vec<u8>>) {
        if let Some(bytes) = favicon {
            if let Some(tab) = self.tabs.tab_mut(tab_id) {
                tab.favicon = Some(Arc::from(bytes));
            }
        }
    }

    pub fn webview_title_changed(&mut self, tab_id: TabId, title: &str) {
        if title.trim().is_empty() {
            return;
        }
        if let Some(tab) = self.tabs.tab_mut(tab_id) {
            tab.title = title.to_string();
        }
    }

    pub fn webview_url_changed(&mut self, tab_id: TabId, url: &str) {
        if is_blank_url(url) {
            return;
        }
        let Some(tab) = self.tabs.tab_mut(tab_id) else {
            return;
        };
        if tab.url.as_deref() == Some(url) {
            return;
        }
        tab.push_navigation(url.to_string());
        if self.focused_tab_id() == Some(tab_id) && self.focus != Some(Focus::Address) {
            self.address.set_text(url);
        }
    }

    pub fn webview_loaded(&mut self, tab_id: TabId) {
        self.tabs.set_loading(tab_id, false);
    }

    pub fn webview_playing_audio(&mut self, tab_id: TabId, playing: bool) {
        self.tabs.set_playing_audio(tab_id, playing);
    }

    pub fn webview_new_window(&mut self, url: &str) {
        if url.is_empty() {
            return;
        }
        let title = host_from_url(url);
        let id = self.tabs.open(title.clone(), Some(url.to_string()));
        self.begin_load(id, url.to_string(), title, NavigationKind::Load);
    }

    pub fn reload(&mut self) {
        let Some(tab) = self
            .focused_tab_id()
            .and_then(|id| self.tabs.tab(id).cloned())
        else {
            return;
        };
        let tab_id = tab.id;
        let url = tab.url.clone().unwrap_or_else(|| NEW_TAB_URL.to_string());
        let title = tab.title.clone();
        self.begin_load(tab_id, url, title, NavigationKind::Reload);
    }

    pub fn go_back(&mut self) {
        let Some(tab) = self.focused_tab_id().and_then(|id| self.tabs.tab_mut(id)) else {
            return;
        };
        if let Some(url) = tab.back() {
            let tab_id = tab.id;
            let title = tab.title.clone();
            self.address.set_text(&url);
            self.begin_load(tab_id, url, title, NavigationKind::Load);
            self.focus = None;
        }
    }

    pub fn go_forward(&mut self) {
        let Some(tab) = self.focused_tab_id().and_then(|id| self.tabs.tab_mut(id)) else {
            return;
        };
        if let Some(url) = tab.forward() {
            let tab_id = tab.id;
            let title = tab.title.clone();
            self.address.set_text(&url);
            self.begin_load(tab_id, url, title, NavigationKind::Load);
            self.focus = None;
        }
    }

    fn field_mut(&mut self, action: impl FnOnce(&mut TextInput)) {
        match self.focus {
            Some(Focus::Address) => action(&mut self.address),
            Some(Focus::BookmarkTitle) => {
                if let Some(edit) = self.editing.as_mut() {
                    action(&mut edit.title);
                }
            }
            Some(Focus::BookmarkUrl) => {
                if let Some(edit) = self.editing.as_mut() {
                    action(&mut edit.url);
                }
            }
            None => {}
        }
    }

    fn blur(&mut self) {
        self.focus = None;
    }

    fn activate(&mut self, element: Element) {
        match element {
            Element::WindowMinimize => {
                self.pending_chrome = Some(ChromeAction::Minimize);
            }
            Element::WindowMaximizeToggle => {
                self.pending_chrome = Some(ChromeAction::MaximizeToggle);
            }
            Element::WindowClose => {
                self.pending_chrome = Some(ChromeAction::Close);
            }
            Element::WindowDrag => {}
            Element::Back => self.go_back(),
            Element::Forward => self.go_forward(),
            Element::ToolbarMenu | Element::Profile => {}
            Element::ToolbarMore => {
                self.show_bookmarks = !self.show_bookmarks;
                self.editing = None;
                self.context_menu = None;
                if !self.show_bookmarks {
                    self.blur();
                }
            }
            Element::FullscreenClose => {
                self.pending_fullscreen_exit = true;
            }
            Element::Tab(id) => self.activate_tab(id),
            Element::TabClose(id) => self.close_tab(id),
            Element::NewTabButton => self.new_tab(),
            Element::SplitDivider(_) => {}
            Element::SplitPaneHeader(index) => self.focus_pane(index),
            Element::SplitPaneClose(index) => self.remove_split_pane(index),
            Element::Address => {
                self.focus = Some(Focus::Address);
            }
            Element::AddressClear => {
                self.address.clear();
                self.focus = Some(Focus::Address);
            }
            Element::BookmarkStar => self.toggle_bookmark(),
            Element::Reload => self.reload(),
            Element::BookmarkHint => {
                self.show_bookmarks = true;
            }
            Element::BookmarkChip(id) => {
                if let Some(bookmark) = self.bookmarks.get(id).cloned() {
                    self.navigate(bookmark.url, bookmark.title);
                    self.show_bookmarks = false;
                    self.focus = None;
                }
            }
            Element::BookmarkManage => {
                self.show_bookmarks = !self.show_bookmarks;
                self.editing = None;
                self.context_menu = None;
            }
            Element::PopoverArea => {}
            Element::PopoverRow(id) => {
                if let Some(bookmark) = self.bookmarks.get(id).cloned() {
                    self.navigate(bookmark.url, bookmark.title);
                    self.show_bookmarks = false;
                    self.focus = None;
                }
            }
            Element::PopoverEdit(id) => {
                if let Some(bookmark) = self.bookmarks.get(id).cloned() {
                    self.editing = Some(BookmarkEdit {
                        id,
                        title: text_input(bookmark.title),
                        url: text_input(bookmark.url),
                    });
                    if let Some(edit) = self.editing.as_mut() {
                        edit.title.select_all();
                    }
                    self.focus = Some(Focus::BookmarkTitle);
                }
            }
            Element::PopoverDelete(id) => {
                self.bookmarks.remove(id);
                if self.editing.as_ref().map(|edit| edit.id) == Some(id) {
                    self.editing = None;
                    self.focus = None;
                }
            }
            Element::EditTitle => {
                self.focus = Some(Focus::BookmarkTitle);
                if let Some(edit) = self.editing.as_mut() {
                    edit.title.select_all();
                }
            }
            Element::EditUrl => {
                self.focus = Some(Focus::BookmarkUrl);
                if let Some(edit) = self.editing.as_mut() {
                    edit.url.select_all();
                }
            }
            Element::EditCancel => {
                self.editing = None;
                self.focus = None;
            }
            Element::EditSave => self.save_bookmark_edit(),
            Element::ContextArea => {}
            Element::ContextOpen => {
                if let Some(menu) = self.context_menu {
                    if let Some(bookmark) = self.bookmarks.get(menu.id).cloned() {
                        self.navigate(bookmark.url, bookmark.title);
                    }
                }
                self.context_menu = None;
                self.focus = None;
            }
            Element::ContextEdit => {
                if let Some(menu) = self.context_menu {
                    if let Some(bookmark) = self.bookmarks.get(menu.id).cloned() {
                        self.editing = Some(BookmarkEdit {
                            id: menu.id,
                            title: text_input(bookmark.title),
                            url: text_input(bookmark.url),
                        });
                        if let Some(edit) = self.editing.as_mut() {
                            edit.title.select_all();
                        }
                        self.focus = Some(Focus::BookmarkTitle);
                    }
                    self.show_bookmarks = true;
                }
                self.context_menu = None;
            }
            Element::ContextDelete => {
                if let Some(menu) = self.context_menu {
                    self.bookmarks.remove(menu.id);
                }
                self.context_menu = None;
            }
            Element::TabContextArea => {}
            Element::TabContextPin(id) => {
                let pinned = self.tabs.toggle_pin(id);
                if pinned == Some(true) {
                    self.forget_split_tab(id);
                }
                self.sync_split_order();
                self.tab_context_menu = None;
            }
            Element::TabContextDuplicate(id) => {
                self.duplicate_tab(id);
            }
            Element::TabContextReload(id) => {
                self.reload_tab(id);
            }
            Element::TabContextNewTabRight(id) => {
                self.new_tab_after(id);
            }
            Element::TabContextSplit(id) => {
                let position = self
                    .tab_context_menu
                    .map(|menu| menu.position)
                    .unwrap_or_else(|| Vec2::new(0.0, 0.0));
                let active = self.tabs.active_id();
                let mut suggested = vec![id];
                if let Some(active) = active {
                    if active != id {
                        suggested.push(active);
                    }
                }
                self.open_split_picker(&suggested, position);
            }
            Element::TabContextAddToSplit(id, anchor) => {
                self.add_to_split(id, anchor);
            }
            Element::TabContextRemoveFromSplit(id) => {
                self.remove_from_split(id);
            }
            Element::TabContextCloseSplit(id) => {
                self.close_split(id);
            }
            Element::TabContextCloseOthers(id) => {
                self.close_other_tabs(id);
            }
            Element::TabContextCloseRight(id) => {
                self.close_tabs_to_right(id);
            }
            Element::TabContextClose(id) => {
                self.close_tab(id);
                self.tab_context_menu = None;
            }
            Element::SplitPickerArea => {}
            Element::SplitPickerRow(id) => self.toggle_split_picker(id),
            Element::SplitPickerCancel => self.split_picker = None,
            Element::SplitPickerConfirm => self.confirm_split_picker(),
        }
    }

    fn save_bookmark_edit(&mut self) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        let normalized = normalize_url(edit.url.text()).unwrap_or_default();
        if normalized.is_empty() {
            self.editing = Some(edit);
            return;
        }
        let title = if edit.title.text().trim().is_empty() {
            host_from_url(&normalized)
        } else {
            edit.title.text().trim().to_string()
        };
        self.bookmarks.update(edit.id, title, normalized);
        self.focus = None;
    }

    fn close_tab(&mut self, id: TabId) {
        let was_active = self.tabs.active_id() == Some(id);
        self.forget_split_tab(id);
        self.tabs.close(id);
        self.closed_tabs.push(id);
        if self
            .tab_drag
            .as_ref()
            .is_some_and(|drag| drag.ids.contains(&id))
        {
            self.tab_drag = None;
        }
        if let Some(picker) = self.split_picker.as_mut() {
            picker.selected.retain(|selected| *selected != id);
        }
        if was_active {
            self.sync_address_to_active();
            self.focus = None;
        }
    }

    fn sync_address_to_active(&mut self) {
        let value = self
            .focused_tab_id()
            .and_then(|id| self.tabs.tab(id))
            .and_then(|tab| tab.url.clone())
            .unwrap_or_default();
        if is_blank_url(&value) {
            self.address.clear();
        } else {
            self.address.set_text(value);
        }
    }

    fn navigate(&mut self, url: String, title: String) {
        let tab_id = if let Some(tab) = self.focused_tab_id().and_then(|id| self.tabs.tab_mut(id)) {
            tab.push_navigation(url.clone());
            tab.title = title.clone();
            tab.id
        } else {
            self.tabs.open(title.clone(), Some(url.clone()))
        };
        self.address.set_text(url.clone());
        self.begin_load(tab_id, url, title, NavigationKind::Load);
        self.show_bookmarks = false;
        self.context_menu = None;
        self.tab_context_menu = None;
        self.split_picker = None;
    }

    fn begin_load(&mut self, tab_id: TabId, url: String, title: String, kind: NavigationKind) {
        self.tabs.set_loading(tab_id, true);
        self.spin_start = Some(Instant::now());
        self.pending = Some(NavigationRequest {
            tab_id,
            url,
            title,
            kind,
        });
    }

    fn request_active_navigation(&mut self) {
        let Some(tab) = self.tabs.active() else {
            return;
        };
        let url = tab.url.clone().unwrap_or_else(|| NEW_TAB_URL.to_string());
        let title = tab.title.clone();
        self.begin_load(tab.id, url, title, NavigationKind::Load);
    }

    fn hit_at(&self, position: Vec2) -> Option<(Element, Rect)> {
        self.hits
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(position))
            .copied()
    }

    fn push_hit(&mut self, element: Element, rect: Rect) {
        self.hits.push((element, rect));
    }

    fn build(&mut self, width: f32, height: f32) -> Frame {
        self.size = Vec2::new(width, height);
        let theme = self.theme;
        let tabs: Vec<Tab> = self.tabs.tabs().to_vec();
        // Setiap grup membawa warnanya sendiri untuk bracket border di tab strip.
        let split_groups: Vec<(Color, Vec<TabId>)> = self
            .splits
            .iter()
            .map(|split| {
                let mut ids = split.panes.clone();
                ids.sort_by_key(|id| {
                    tabs.iter()
                        .position(|tab| tab.id == *id)
                        .unwrap_or(usize::MAX)
                });
                (split.color, ids)
            })
            .collect();
        let active_id = self.tabs.active_id();
        let bookmarks: Vec<Bookmark> = self.bookmarks.items().to_vec();
        let address_text = self.address.text().to_string();
        let address_cursor = self.address.cursor();
        let focus = self.focus;
        let editing = self.editing.clone();
        let hover = self.hover;
        let show_bookmarks = self.show_bookmarks;
        let context_menu = self.context_menu;
        let tab_context_menu = self.tab_context_menu;
        let split_picker = self.split_picker.clone();
        let picker_is_open = split_picker.is_some();

        self.hits.clear();
        let mut frame = Frame::default();
        let base = &mut frame.base;

        base.push_rect(RectShape::filled(
            Rect::new(0.0, 0.0, width, height),
            theme.background,
            0.0,
        ));

        if self.fullscreen {
            let viewport = Rect::new(0.0, 0.0, width, height);
            self.layout.viewport = viewport;
            self.build_viewport(base, viewport);
            if self.split_is_shown() {
                self.build_split_view(base, viewport, hover);
            }
            self.build_fullscreen(&mut frame, width, hover);
            return frame;
        }

        let tab_strip = Rect::new(0.0, 0.0, width, TAB_STRIP_HEIGHT);
        base.push_rect(RectShape::filled(tab_strip, theme.tab_strip, 0.0));

        let toolbar = Rect::new(0.0, TAB_STRIP_HEIGHT, width, TOOLBAR_HEIGHT);
        let bar_height = if self.bookmark_bar_visible() {
            BOOKMARK_BAR_HEIGHT
        } else {
            0.0
        };
        let bookmark_bar = Rect::new(0.0, toolbar.bottom(), width, bar_height);
        let viewport = Rect::new(
            0.0,
            bookmark_bar.bottom(),
            width,
            (height - bookmark_bar.bottom()).max(0.0),
        );
        self.layout.viewport = viewport;

        self.build_tabs(
            base,
            &mut frame.overlay,
            tab_strip,
            width,
            &tabs,
            &split_groups,
            active_id,
            hover,
        );
        self.build_toolbar(
            base,
            toolbar,
            width,
            active_id,
            &address_text,
            address_cursor,
            focus,
            hover,
        );
        if bar_height > 0.0 {
            self.build_bookmark_bar(base, bookmark_bar, width, &bookmarks, hover);
        }
        self.build_viewport(base, viewport);
        if self.split_is_shown() {
            self.build_split_view(base, viewport, hover);
        }
        if !self.maximized {
            base.push_rect(RectShape::bordered(
                Rect::new(0.5, 0.5, (width - 1.0).max(0.0), (height - 1.0).max(0.0)),
                Color::TRANSPARENT,
                WINDOW_BORDER_RADIUS,
                1.0,
                theme.border,
            ));
        }

        if show_bookmarks {
            self.build_bookmark_popover(
                &mut frame, width, height, &bookmarks, &editing, focus, hover,
            );
        }

        if let Some(menu) = context_menu {
            self.build_context_menu(&mut frame, width, height, &bookmarks, menu, hover);
        }
        if let Some(menu) = tab_context_menu {
            self.build_tab_context_menu(&mut frame, width, height, menu, hover);
        }
        if let Some(picker) = split_picker {
            self.build_split_picker(&mut frame, width, height, &picker, hover);
        }

        let tooltip = hover
            .and_then(|element| match element {
                Element::Tab(id) => Some(id),
                _ => None,
            })
            .and_then(|id| {
                self.tab_rects
                    .iter()
                    .find(|(tab_id, _)| *tab_id == id)
                    .map(|(_, rect)| (id, *rect))
            })
            .and_then(|(id, rect)| {
                let tab = self.tabs.tab(id)?;
                let close_x = rect.right() - 10.0 - 20.0;
                let title_width = (close_x - 8.0 - (rect.x + 10.0)).max(0.0);
                if approx_text_width(&tab.title, 12.0) > title_width {
                    Some((rect.center().x, rect.bottom() + 6.0, tab.title.clone()))
                } else {
                    None
                }
            });
        frame.popup_open = show_bookmarks
            || context_menu.is_some()
            || tab_context_menu.is_some()
            || picker_is_open
            || tooltip.is_some();
        if let Some((center_x, y, text)) = tooltip {
            let tooltip_width = (approx_text_width(&text, 12.0) + 16.0).min(width - 16.0);
            let x = (center_x - tooltip_width * 0.5)
                .max(8.0)
                .min((width - tooltip_width - 8.0).max(8.0));
            let rect = Rect::new(x, y, tooltip_width, 26.0);
            frame.overlay.push_rect(RectShape::bordered(
                rect,
                theme.popover,
                4.0,
                1.0,
                theme.border,
            ));
            frame.overlay.push_text(
                TextShape::new(
                    text,
                    rect.x + 8.0,
                    rect.y + 5.0,
                    12.0,
                    theme.popover_foreground,
                )
                .width(rect.width - 16.0)
                .ellipsis(),
            );
        }

        frame
    }

    fn build_split_view(
        &mut self,
        layer: &mut rb_ui::Layer,
        content: Rect,
        hover: Option<Element>,
    ) {
        let Some(split) = self.shown_split().cloned() else {
            return;
        };
        let theme = self.theme;
        for (index, pane) in split.pane_rects(content).into_iter().enumerate() {
            let header = Rect::new(
                pane.x,
                pane.y,
                pane.width,
                SPLIT_HEADER_HEIGHT.min(pane.height),
            );
            let is_active = index == split.active_pane;
            layer.push_rect(RectShape::filled(
                header,
                if is_active {
                    theme.tab_hover
                } else {
                    theme.tab_inactive
                },
                0.0,
            ));
            let Some(tab_id) = split.panes.get(index).copied() else {
                continue;
            };
            let close_rect = Rect::new(header.right() - 22.0, header.y + 6.0, 16.0, 16.0);
            if let Some(tab) = self.tabs.tab(tab_id).cloned() {
                let favicon_rect = Rect::new(header.x + 10.0, header.y + 6.0, 14.0, 14.0);
                if tab.loading {
                    layer.push_icon(
                        IconShape::new(Icon::Loader, favicon_rect, theme.accent)
                            .rotation(self.spin_rotation()),
                    );
                } else {
                    push_tab_favicon(layer, &tab, favicon_rect, theme);
                }
            }
            if hover == Some(Element::SplitPaneClose(index)) {
                layer.push_rect(RectShape::filled(close_rect, theme.accent, 4.0));
            }
            layer.push_icon(IconShape::new(
                Icon::Close,
                icon_rect(close_rect.center(), 10.0),
                theme.muted_foreground,
            ));
            self.push_hit(Element::SplitPaneHeader(index), header);
            self.push_hit(Element::SplitPaneClose(index), close_rect);
        }
        for (index, divider) in split.divider_rects(content).into_iter().enumerate() {
            let center = divider.center().x;
            let hovered = hover == Some(Element::SplitDivider(index));
            let color = if hovered { theme.primary } else { theme.border };
            layer.push_rect(RectShape::filled(
                Rect::new(center - 1.0, divider.y, 2.0, divider.height),
                color,
                1.0,
            ));
            layer.push_rect(RectShape::filled(
                Rect::new(center - 2.0, divider.center().y - 14.0, 4.0, 28.0),
                if hovered { theme.primary } else { theme.muted },
                2.0,
            ));
            self.push_hit(Element::SplitDivider(index), divider);
        }
    }

    /// Menggambar satu tab. Mengembalikan warna latar dan batas kanan judul jika judul terpotong,
    /// sehingga pemanggil bisa menambahkan bayangan pudar di layer overlay.
    fn draw_tab(
        &mut self,
        layer: &mut rb_ui::Layer,
        tab: &Tab,
        rect: Rect,
        is_active: bool,
        is_hover: bool,
        hover: Option<Element>,
    ) -> Option<(Color, f32)> {
        let theme = self.theme;
        let background = if is_active {
            theme.background
        } else if is_hover {
            theme.tab_hover
        } else {
            theme.tab_inactive
        };
        layer.push_rect(RectShape::filled(rect, background, TAB_RADIUS));
        // Sudut bawah dibuat tajam agar tab menyatu dengan address bar di bawahnya.
        layer.push_rect(RectShape::filled(
            Rect::new(rect.x, rect.bottom() - TAB_RADIUS, rect.width, TAB_RADIUS),
            background,
            0.0,
        ));
        self.push_hit(Element::Tab(tab.id), rect);

        let close_rect = Rect::new(rect.right() - 10.0 - 20.0, rect.y + 6.0, 20.0, 20.0);

        let favicon_rect = if tab.pinned {
            Rect::new(
                rect.center().x - TAB_ICON_SIZE * 0.5,
                rect.center().y - TAB_ICON_SIZE * 0.5,
                TAB_ICON_SIZE,
                TAB_ICON_SIZE,
            )
        } else {
            Rect::new(
                rect.x + 10.0,
                rect.y + (TAB_HEIGHT - TAB_ICON_SIZE) * 0.5,
                TAB_ICON_SIZE,
                TAB_ICON_SIZE,
            )
        };
        // Saat memuat, ikon tab diganti spinner. Favicon lama tetap disimpan dan
        // tampil lagi begitu halaman selesai, jadi ikon tidak ikut hilang.
        if tab.loading {
            layer.push_icon(
                IconShape::new(Icon::Loader, favicon_rect, theme.accent)
                    .rotation(self.spin_rotation()),
            );
        } else {
            push_tab_favicon(layer, tab, favicon_rect, theme);
        }
        if tab.pinned {
            layer.push_icon(IconShape::new(
                Icon::Pin,
                Rect::new(rect.right() - 13.0, rect.bottom() - 13.0, 9.0, 9.0),
                theme.muted_foreground,
            ));
            return None;
        }

        let audio_rect = tab
            .playing_audio
            .then(|| Rect::new(close_rect.x - 17.0, close_rect.y + 3.0, 14.0, 14.0));
        let text_x = favicon_rect.right() + TAB_ICON_GAP;
        let title_end = audio_rect.map(|rect| rect.x).unwrap_or(close_rect.x);
        let title_width = (title_end - 8.0 - text_x).max(0.0);
        let title_color = if is_active {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        let mut title = TextShape::new(
            tab.title.clone(),
            text_x,
            rect.y + (TAB_HEIGHT - 12.0 * 1.3) * 0.5,
            12.0,
            title_color,
        )
        .width(title_width)
        .ellipsis();
        if title_width <= 0.0 {
            title.text.clear();
        }
        layer.push_text(title);

        if let Some(audio_rect) = audio_rect {
            layer.push_rect(RectShape::filled(audio_rect, theme.accent, 7.0));
            layer.push_icon(IconShape::new(
                Icon::Volume,
                icon_rect(audio_rect.center(), 9.0),
                theme.background,
            ));
        }

        let close_hover = hover == Some(Element::TabClose(tab.id));
        if close_hover {
            layer.push_rect(RectShape::filled(close_rect, theme.accent, 10.0));
        }
        let close_color = if close_hover {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        layer.push_icon(IconShape::new(
            Icon::Close,
            icon_rect(close_rect.center(), 12.0),
            close_color,
        ));
        self.push_hit(Element::TabClose(tab.id), close_rect);
        (approx_text_width(&tab.title, 12.0) > title_width)
            .then_some((background, text_x + title_width))
    }

    // Layer tab biasa dan layer overlay (drag, bayangan judul) harus dipisah, jadi parameternya banyak.
    #[allow(clippy::too_many_arguments)]
    fn build_tabs(
        &mut self,
        layer: &mut rb_ui::Layer,
        overlay: &mut rb_ui::Layer,
        strip: Rect,
        width: f32,
        tabs: &[Tab],
        split_groups: &[(Color, Vec<TabId>)],
        active_id: Option<TabId>,
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        let padding = 8.0;
        let plus_size = 28.0;
        let available = (width
            - padding * 2.0
            - plus_size
            - TAB_GAP
            - WindowChrome::reserved_width()
            - TAB_GAP)
            .max(0.0);
        let max_width = (width * TAB_MAX_RATIO).min(TAB_MAX_WIDTH);

        let naturals: Vec<f32> = tabs
            .iter()
            .map(|tab| {
                if tab.pinned {
                    PINNED_TAB_WIDTH
                } else {
                    let title = approx_text_width(&tab.title, 12.0);
                    (10.0 + TAB_ICON_SIZE + TAB_ICON_GAP + title + 8.0 + 20.0 + 10.0)
                        .clamp(TAB_MIN_WIDTH, max_width.max(TAB_MIN_WIDTH))
                }
            })
            .collect();
        let mut widths = naturals.clone();
        if !tabs.is_empty() {
            let sum: f32 = naturals.iter().sum();
            let gaps = TAB_GAP * (tabs.len() as f32 - 1.0);
            if sum + gaps > available {
                let pinned_total: f32 = tabs
                    .iter()
                    .zip(naturals.iter())
                    .filter(|(tab, _)| tab.pinned)
                    .map(|(_, width)| *width)
                    .sum();
                let flexible_sum = (sum - pinned_total).max(0.0);
                let flexible_available = (available - pinned_total - gaps).max(0.0);
                let scale = if flexible_sum > 0.0 {
                    (flexible_available / flexible_sum).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let flexible_count = tabs.iter().filter(|tab| !tab.pinned).count().max(1) as f32;
                let per_tab = flexible_available / flexible_count;
                widths = tabs
                    .iter()
                    .zip(naturals.iter())
                    .map(|(tab, natural)| {
                        if tab.pinned {
                            PINNED_TAB_WIDTH
                        } else {
                            (natural * scale)
                                .max(TAB_MIN_WIDTH)
                                .min(*natural)
                                .min(per_tab.max(TAB_MIN_WIDTH))
                        }
                    })
                    .collect();
            }
        }

        let mut target_xs = Vec::with_capacity(widths.len());
        let mut x = padding;
        for tab_width in &widths {
            target_xs.push(x);
            x += *tab_width + TAB_GAP;
        }
        let target_centers: Vec<f32> = target_xs
            .iter()
            .zip(widths.iter())
            .map(|(tab_x, tab_width)| tab_x + tab_width * 0.5)
            .collect();

        let mut dragged_state: Option<(Vec<TabId>, f32)> = None;
        if let Some(drag) = self.tab_drag.as_mut() {
            if drag.dragging {
                if drag.slots.len() != target_centers.len() {
                    drag.slots = target_centers.clone();
                }
                let mut settled = true;
                for (slot, target) in drag.slots.iter_mut().zip(target_centers.iter()) {
                    *slot += (target - *slot) * 0.35;
                    if (target - *slot).abs() > 0.4 {
                        settled = false;
                    }
                }
                let target_visual = (drag.current.x - drag.grab_offset)
                    .clamp(padding, (padding + available).max(padding));
                drag.visual_x += (target_visual - drag.visual_x) * 0.35;
                if (target_visual - drag.visual_x).abs() > 0.4 {
                    settled = false;
                }
                drag.settled = settled;
                dragged_state = Some((drag.ids.clone(), drag.visual_x - drag.origin_x));
            }
        }

        let slot_centers = self
            .tab_drag
            .as_ref()
            .filter(|drag| drag.dragging)
            .map(|drag| drag.slots.clone())
            .unwrap_or_else(|| target_centers.clone());
        let visual_xs: Vec<f32> = (0..tabs.len())
            .map(|index| {
                let base = slot_centers
                    .get(index)
                    .copied()
                    .unwrap_or(target_centers[index])
                    - widths[index] * 0.5;
                match &dragged_state {
                    Some((ids, offset)) if ids.contains(&tabs[index].id) => {
                        target_xs[index] + offset
                    }
                    _ => base,
                }
            })
            .collect();

        // Satu bracket per run tab yang berdampingan, dengan warna grupnya. Tab non-anggota
        // di antara dua run tidak pernah ikut terkurung.
        let mut runs: Vec<(Color, Vec<usize>)> = Vec::new();
        for (color, ids) in split_groups {
            let group_indices: Vec<usize> = ids
                .iter()
                .filter_map(|id| tabs.iter().position(|tab| tab.id == *id))
                .collect();
            for index in group_indices {
                match runs.last_mut() {
                    Some((run_color, run))
                        if *run_color == *color
                            && run.last().is_some_and(|last| last + 1 == index) =>
                    {
                        run.push(index)
                    }
                    _ => runs.push((*color, vec![index])),
                }
            }
        }
        for (color, run) in runs.iter().filter(|(_, run)| run.len() >= 2) {
            let color = *color;
            let left = run
                .iter()
                .map(|index| visual_xs[*index] - 2.0)
                .fold(f32::INFINITY, f32::min);
            let right = run
                .iter()
                .map(|index| visual_xs[*index] + widths[*index] + 2.0)
                .fold(f32::NEG_INFINITY, f32::max);
            layer.push_rect(RectShape::bordered(
                Rect::new(
                    left,
                    strip.y + 6.0,
                    (right - left).max(0.0),
                    TAB_HEIGHT + 2.0,
                ),
                Color::TRANSPARENT,
                TAB_RADIUS + 2.0,
                2.0,
                color,
            ));
            layer.push_rect(RectShape::filled(
                Rect::new(
                    left - 2.0,
                    strip.y + TAB_HEIGHT - 2.0,
                    (right - left + 4.0).max(0.0),
                    6.0,
                ),
                theme.tab_strip,
                0.0,
            ));
            for pair in run.windows(2) {
                let boundary = visual_xs[pair[1]] - TAB_GAP * 0.5;
                layer.push_rect(RectShape::filled(
                    Rect::new(
                        boundary - 1.0,
                        strip.y + 12.0,
                        2.0,
                        (TAB_HEIGHT - 8.0).max(0.0),
                    ),
                    color,
                    1.0,
                ));
            }
        }

        self.tab_rects.clear();
        // Tab yang sedang di-drag digambar paling akhir agar selalu di atas tab lain.
        let mut dragged_draw = Vec::new();
        for (index, tab) in tabs.iter().enumerate() {
            let tab_width = widths[index];
            let tab_rect = Rect::new(visual_xs[index], strip.y + 8.0, tab_width, TAB_HEIGHT);
            self.tab_rects.push((tab.id, tab_rect));
            let is_active = active_id == Some(tab.id);
            let is_hover = hover == Some(Element::Tab(tab.id));
            if dragged_state
                .as_ref()
                .is_some_and(|(ids, _)| ids.contains(&tab.id))
            {
                dragged_draw.push((tab, tab_rect, is_active, is_hover));
                continue;
            }
            let fade = self.draw_tab(layer, tab, tab_rect, is_active, is_hover, hover);
            if let Some((color, right)) = fade {
                push_title_fade(overlay, tab_rect, right, color);
            }
        }
        // Tab yang di-drag digambar di overlay (di atas teks tab lain) dengan warna gelap penuh.
        for (tab, tab_rect, is_active, is_hover) in dragged_draw {
            let fade = self.draw_tab(overlay, tab, tab_rect, is_active, is_hover, hover);
            if let Some((color, right)) = fade {
                push_title_fade(overlay, tab_rect, right, color);
            }
        }

        let plus_rect = Rect::new(x, strip.y + 10.0, plus_size, plus_size);
        let plus_hover = hover == Some(Element::NewTabButton);
        if plus_hover {
            layer.push_rect(RectShape::filled(plus_rect, theme.accent, 14.0));
        }
        let plus_color = if plus_hover {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        layer.push_icon(IconShape::new(
            Icon::Plus,
            icon_rect(plus_rect.center(), 16.0),
            plus_color,
        ));
        self.push_hit(Element::NewTabButton, plus_rect);

        self.chrome = WindowChrome::layout(width, plus_rect.right() + TAB_GAP);
        self.chrome
            .draw(layer, theme, chrome_hover(hover), self.maximized);
        for (rect, element) in self.chrome.hit_regions() {
            self.push_hit(
                match element {
                    ChromeElement::Minimize => Element::WindowMinimize,
                    ChromeElement::MaximizeToggle => Element::WindowMaximizeToggle,
                    ChromeElement::Close => Element::WindowClose,
                    ChromeElement::DragStrip => Element::WindowDrag,
                },
                rect,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_toolbar(
        &mut self,
        layer: &mut rb_ui::Layer,
        toolbar: Rect,
        width: f32,
        active_id: Option<TabId>,
        address: &str,
        cursor: usize,
        focus: Option<Focus>,
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        let side = 8.0;
        let gap = NAV_GAP;
        let button_y = toolbar.y + (TOOLBAR_HEIGHT - NAV_BUTTON_SIZE) * 0.5;

        let back_rect = Rect::new(side, button_y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE);
        let forward_rect = Rect::new(
            back_rect.right() + gap,
            button_y,
            NAV_BUTTON_SIZE,
            NAV_BUTTON_SIZE,
        );
        let reload_rect = Rect::new(
            forward_rect.right() + gap,
            button_y,
            NAV_BUTTON_SIZE,
            NAV_BUTTON_SIZE,
        );
        let more_rect = Rect::new(
            width - side - NAV_BUTTON_SIZE,
            button_y,
            NAV_BUTTON_SIZE,
            NAV_BUTTON_SIZE,
        );
        let profile_rect = Rect::new(
            more_rect.x - gap - NAV_BUTTON_SIZE,
            button_y,
            NAV_BUTTON_SIZE,
            NAV_BUTTON_SIZE,
        );
        let menu_rect = Rect::new(
            profile_rect.x - gap - NAV_BUTTON_SIZE,
            button_y,
            NAV_BUTTON_SIZE,
            NAV_BUTTON_SIZE,
        );
        self.layout.manage_button = more_rect;

        let form = Rect::new(
            reload_rect.right() + gap,
            toolbar.y + 6.0,
            (menu_rect.x - gap - reload_rect.right() - gap).max(1.0),
            ADDRESS_HEIGHT,
        );

        let (can_back, can_forward, has_url, active_url) = active_id
            .and_then(|id| self.tabs.tab(id))
            .map(|tab| {
                (
                    tab.can_go_back(),
                    tab.can_go_forward(),
                    tab.url.is_some(),
                    tab.url.clone(),
                )
            })
            .unwrap_or((false, false, false, None));

        let nav_button = |layer: &mut rb_ui::Layer,
                          rect: Rect,
                          icon: Icon,
                          element: Element,
                          enabled: bool,
                          rotation: f32| {
            let is_hover = hover == Some(element) && enabled;
            if is_hover {
                layer.push_rect(RectShape::filled(rect, theme.accent, NAV_BUTTON_SIZE * 0.5));
            }
            let color = if !enabled {
                theme.muted_foreground.with_alpha(0.35)
            } else if is_hover {
                theme.foreground
            } else {
                theme.muted_foreground
            };
            layer.push_icon(
                IconShape::new(icon, icon_rect(rect.center(), 18.0), color).rotation(rotation),
            );
        };
        nav_button(
            layer,
            back_rect,
            Icon::ArrowLeft,
            Element::Back,
            can_back,
            0.0,
        );
        nav_button(
            layer,
            forward_rect,
            Icon::ArrowRight,
            Element::Forward,
            can_forward,
            0.0,
        );
        nav_button(
            layer,
            reload_rect,
            Icon::Reload,
            Element::Reload,
            has_url,
            self.spin_rotation(),
        );
        nav_button(
            layer,
            menu_rect,
            Icon::Menu,
            Element::ToolbarMenu,
            true,
            0.0,
        );
        nav_button(
            layer,
            profile_rect,
            Icon::Profile,
            Element::Profile,
            true,
            0.0,
        );
        nav_button(
            layer,
            more_rect,
            Icon::More,
            Element::ToolbarMore,
            true,
            0.0,
        );
        self.push_hit(Element::Back, back_rect);
        self.push_hit(Element::Forward, forward_rect);
        self.push_hit(Element::Reload, reload_rect);
        self.push_hit(Element::ToolbarMenu, menu_rect);
        self.push_hit(Element::Profile, profile_rect);
        self.push_hit(Element::ToolbarMore, more_rect);

        let form_focused = focus == Some(Focus::Address);
        let is_url = normalize_url(address).is_some();

        if form_focused {
            let ring = Rect::new(
                form.x - 2.0,
                form.y - 2.0,
                form.width + 4.0,
                form.height + 4.0,
            );
            layer.push_rect(RectShape::bordered(
                ring,
                Color::TRANSPARENT,
                20.0,
                2.0,
                theme.ring.with_alpha(0.35),
            ));
        }

        let form_bg = if form_focused {
            theme.background
        } else {
            theme.muted
        };
        if form_focused {
            layer.push_rect(RectShape::bordered(form, form_bg, 18.0, 1.0, theme.ring));
        } else {
            layer.push_rect(RectShape::filled(form, form_bg, 18.0));
        }
        self.push_hit(Element::Address, form);

        let icon_center = Vec2::new(form.x + 20.0, form.center().y);
        let leading_icon = if is_url { Icon::Lock } else { Icon::Search };
        layer.push_icon(IconShape::new(
            leading_icon,
            icon_rect(icon_center, 16.0),
            theme.muted_foreground,
        ));

        let star_rect = Rect::new(form.right() - 10.0 - 20.0, form.y + 8.0, 20.0, 20.0);
        let clear_rect = if address.is_empty() {
            None
        } else {
            Some(Rect::new(
                star_rect.x - 4.0 - 20.0,
                form.y + 8.0,
                20.0,
                20.0,
            ))
        };
        let text_x = icon_center.x + 8.0 + 8.0;
        let text_end = clear_rect.map(|rect| rect.x).unwrap_or(star_rect.x);
        let available_width = (text_end - 8.0 - text_x).max(0.0);

        let bookmarked = active_url
            .as_deref()
            .and_then(|url| self.bookmarks.find_by_url(url).map(|item| item.id))
            .is_some();
        let selection = self.address.selection();
        let has_selection = self.address.has_selection();
        let caret = if form_focused { Some(cursor) } else { None };
        let sel_start = text_width(
            &address[..char_boundary(address, selection.0)],
            ADDRESS_FONT_SIZE,
        );
        let caret_width = text_width(
            &address[..char_boundary(address, cursor)],
            ADDRESS_FONT_SIZE,
        );
        let focus_position = if has_selection {
            sel_start.min(caret_width)
        } else {
            caret_width
        };
        let max_visible = (available_width - 24.0).max(0.0);
        let scroll = if form_focused && focus_position > max_visible {
            focus_position - max_visible + 24.0
        } else {
            0.0
        };
        self.layout.address_text_x = text_x;
        self.layout.address_scroll = scroll;

        let text_y = form.y + (ADDRESS_HEIGHT - ADDRESS_FONT_SIZE * 1.3) * 0.5;
        if address.is_empty() {
            layer.push_text(
                TextShape::new(
                    "Cari atau ketik alamat",
                    text_x,
                    text_y,
                    ADDRESS_FONT_SIZE,
                    theme.muted_foreground,
                )
                .width(available_width)
                .ellipsis()
                .caret(caret, theme.foreground),
            );
        } else {
            layer.push_text(
                TextShape::new(
                    address.to_string(),
                    text_x,
                    text_y,
                    ADDRESS_FONT_SIZE,
                    theme.foreground,
                )
                .width(available_width)
                .ellipsis()
                .selection(
                    if form_focused && has_selection {
                        Some(selection)
                    } else {
                        None
                    },
                    Color::rgba(0x3f, 0x6f, 0xb5, 0x99),
                )
                .caret(caret, theme.foreground)
                .offset_x(scroll),
            );
        }

        if let Some(clear_rect) = clear_rect {
            let is_hover = hover == Some(Element::AddressClear);
            if is_hover {
                layer.push_rect(RectShape::filled(clear_rect, theme.accent, 10.0));
            }
            let color = if is_hover {
                theme.foreground
            } else {
                theme.muted_foreground
            };
            layer.push_icon(IconShape::new(
                Icon::Close,
                icon_rect(clear_rect.center(), 12.0),
                color,
            ));
            self.push_hit(Element::AddressClear, clear_rect);
        }

        let star_hover = hover == Some(Element::BookmarkStar) && has_url;
        if star_hover {
            layer.push_rect(RectShape::filled(star_rect, theme.accent, 10.0));
        }
        let star_color = if !has_url {
            theme.muted_foreground.with_alpha(0.4)
        } else if bookmarked {
            theme.star_fill
        } else if star_hover {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        let star_icon = if bookmarked {
            Icon::StarFilled
        } else {
            Icon::Star
        };
        layer.push_icon(IconShape::new(
            star_icon,
            icon_rect(star_rect.center(), 16.0),
            star_color,
        ));
        self.push_hit(Element::BookmarkStar, star_rect);
    }

    fn build_bookmark_bar(
        &mut self,
        layer: &mut rb_ui::Layer,
        bar: Rect,
        width: f32,
        bookmarks: &[Bookmark],
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        layer.push_rect(RectShape::filled(
            Rect::new(bar.x, bar.bottom() - 1.0, bar.width, 1.0),
            theme.border,
            0.0,
        ));

        let leading_rect = Rect::new(bar.x + 12.0, bar.y + 9.0, 14.0, 14.0);
        layer.push_icon(IconShape::new(
            Icon::Bookmark,
            icon_rect(leading_rect.center(), 14.0),
            theme.muted_foreground,
        ));

        let manage_rect = Rect::new(width - 12.0 - 24.0, bar.y + 4.0, 24.0, 24.0);

        let strip_left = leading_rect.right() + 8.0;
        let strip_right = manage_rect.x - 8.0;

        if bookmarks.is_empty() {
            let message = "Belum ada bookmark. Klik bintang di alamat untuk menyimpan halaman.";
            let width_est = approx_text_width(message, 11.0);
            let text_rect = Rect::new(strip_left, bar.y + 8.0, width_est, 16.0);
            let is_hover = hover == Some(Element::BookmarkHint);
            let color = if is_hover {
                theme.foreground
            } else {
                theme.muted_foreground
            };
            layer.push_text(
                TextShape::new(
                    message,
                    strip_left,
                    bar.y + (BOOKMARK_BAR_HEIGHT - 11.0 * 1.3) * 0.5,
                    11.0,
                    color,
                )
                .width((strip_right - strip_left).max(0.0))
                .ellipsis(),
            );
            self.push_hit(Element::BookmarkHint, text_rect);
        } else {
            let mut x = strip_left;
            for bookmark in bookmarks {
                if x >= strip_right {
                    break;
                }
                let chip_width =
                    (8.0 + 14.0 + 6.0 + approx_text_width(&bookmark.title, 11.0) + 8.0).min(180.0);
                let visible_width = (strip_right - x).min(chip_width);
                if visible_width < 12.0 {
                    break;
                }
                let chip_rect = Rect::new(
                    x,
                    bar.y + (BOOKMARK_BAR_HEIGHT - BOOKMARK_CHIP_HEIGHT) * 0.5,
                    visible_width,
                    BOOKMARK_CHIP_HEIGHT,
                );
                let is_hover = hover == Some(Element::BookmarkChip(bookmark.id));
                if is_hover {
                    layer.push_rect(RectShape::filled(chip_rect, theme.muted, 4.0));
                }

                let badge = Rect::new(
                    chip_rect.x + 8.0,
                    chip_rect.y + (BOOKMARK_CHIP_HEIGHT - 14.0) * 0.5,
                    14.0,
                    14.0,
                );
                layer.push_rect(RectShape::filled(badge, theme.muted, 2.0));
                let first = bookmark
                    .title
                    .chars()
                    .next()
                    .map(|ch| ch.to_uppercase().to_string())
                    .unwrap_or_default();
                layer.push_text(
                    TextShape::new(
                        first,
                        badge.x,
                        badge.y + (14.0 - 9.0 * 1.3) * 0.5,
                        9.0,
                        theme.foreground,
                    )
                    .width(badge.width)
                    .weight(600)
                    .align(Align::Center),
                );

                let label_x = badge.right() + 6.0;
                let label_width = (chip_rect.right() - 8.0 - label_x).max(0.0);
                let label_color = if is_hover {
                    theme.foreground
                } else {
                    theme.muted_foreground
                };
                layer.push_text(
                    TextShape::new(
                        bookmark.title.clone(),
                        label_x,
                        chip_rect.y + (BOOKMARK_CHIP_HEIGHT - 11.0 * 1.3) * 0.5,
                        11.0,
                        label_color,
                    )
                    .width(label_width)
                    .ellipsis(),
                );

                self.push_hit(Element::BookmarkChip(bookmark.id), chip_rect);
                x += chip_width + 4.0;
            }
        }

        let manage_hover = hover == Some(Element::BookmarkManage);
        if manage_hover {
            layer.push_rect(RectShape::filled(manage_rect, theme.accent, 12.0));
        }
        let manage_color = if manage_hover {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        layer.push_icon(IconShape::new(
            Icon::More,
            icon_rect(manage_rect.center(), 14.0),
            manage_color,
        ));
        self.push_hit(Element::BookmarkManage, manage_rect);
    }

    fn build_fullscreen(&mut self, frame: &mut Frame, width: f32, hover: Option<Element>) {
        if self.pointer.y > FULLSCREEN_BAR_HEIGHT && hover != Some(Element::FullscreenClose) {
            return;
        }
        let theme = self.theme;
        let layer = &mut frame.overlay;
        layer.push_rect(RectShape::filled(
            Rect::new(0.0, 0.0, width, FULLSCREEN_BAR_HEIGHT),
            Color::rgba(0x00, 0x00, 0x00, 0x99),
            0.0,
        ));
        let close_rect = Rect::new(
            width - 12.0 - 36.0,
            (FULLSCREEN_BAR_HEIGHT - 36.0) * 0.5,
            36.0,
            36.0,
        );
        let is_hover = hover == Some(Element::FullscreenClose);
        if is_hover {
            layer.push_rect(RectShape::filled(close_rect, theme.accent, 18.0));
        }
        layer.push_icon(IconShape::new(
            Icon::Close,
            icon_rect(close_rect.center(), 18.0),
            theme.foreground,
        ));
        layer.push_text(
            TextShape::new(
                "Keluar fullscreen",
                16.0,
                (FULLSCREEN_BAR_HEIGHT - 12.0 * 1.3) * 0.5,
                12.0,
                theme.foreground,
            )
            .weight(500),
        );
        self.push_hit(Element::FullscreenClose, close_rect);
    }

    fn build_viewport(&mut self, layer: &mut rb_ui::Layer, viewport: Rect) {
        layer.push_rect(RectShape::filled(viewport, self.theme.background, 0.0));
    }

    #[allow(clippy::too_many_arguments)]
    fn build_bookmark_popover(
        &mut self,
        frame: &mut Frame,
        width: f32,
        height: f32,
        bookmarks: &[Bookmark],
        editing: &Option<BookmarkEdit>,
        focus: Option<Focus>,
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        let manage = self.layout.manage_button;
        let x = (manage.right() - POPOVER_WIDTH)
            .max(8.0)
            .min((width - POPOVER_WIDTH - 8.0).max(8.0));
        let y = (manage.bottom() + 4.0).min((height - 120.0).max(8.0));
        let list_count = bookmarks
            .len()
            .min((POPOVER_MAX_LIST_HEIGHT / ROW_HEIGHT) as usize);
        let list_height = list_count as f32 * ROW_HEIGHT;
        let empty_height = if bookmarks.is_empty() { 48.0 } else { 0.0 };
        let edit_height = if editing.is_some() { 132.0 } else { 0.0 };
        let popover_height = 8.0 + 24.0 + 8.0 + list_height.max(empty_height) + edit_height + 8.0;

        let popover = Rect::new(x, y, POPOVER_WIDTH, popover_height);
        let layer = &mut frame.overlay;
        layer.push_rect(RectShape::bordered(
            popover,
            theme.popover,
            6.0,
            1.0,
            theme.border,
        ));
        self.push_hit(Element::PopoverArea, popover);

        let header_y = popover.y + 8.0 + 4.0;
        layer.push_text(
            TextShape::new(
                "Bookmark",
                popover.x + 16.0,
                header_y,
                12.0,
                theme.popover_foreground,
            )
            .weight(600),
        );
        layer.push_text(
            TextShape::new(
                format!("{} tersimpan", bookmarks.len()),
                popover.x + 16.0,
                header_y + 1.0,
                10.0,
                theme.muted_foreground,
            )
            .width(popover.width - 32.0)
            .align(Align::Right),
        );

        let list_y = popover.y + 8.0 + 24.0 + 8.0;
        if bookmarks.is_empty() {
            layer.push_text(
                TextShape::new(
                    "Belum ada bookmark.",
                    popover.x,
                    list_y + 16.0,
                    12.0,
                    theme.muted_foreground,
                )
                .width(popover.width)
                .align(Align::Center),
            );
        } else {
            for (index, bookmark) in bookmarks.iter().take(list_count).enumerate() {
                let row = Rect::new(
                    popover.x + 8.0,
                    list_y + index as f32 * ROW_HEIGHT,
                    popover.width - 16.0,
                    ROW_HEIGHT,
                );
                let is_hover = hover == Some(Element::PopoverRow(bookmark.id));
                if is_hover {
                    layer.push_rect(RectShape::filled(row, theme.muted, 6.0));
                }
                let edit_rect = Rect::new(row.right() - 48.0 - 4.0, row.y + 6.0, 24.0, 24.0);
                let delete_rect = Rect::new(row.right() - 24.0 - 4.0, row.y + 6.0, 24.0, 24.0);
                let title_width = (edit_rect.x - row.x - 8.0).max(0.0);
                layer.push_text(
                    TextShape::new(
                        bookmark.title.clone(),
                        row.x + 8.0,
                        row.y + 4.0,
                        12.0,
                        theme.popover_foreground,
                    )
                    .width(title_width)
                    .ellipsis(),
                );
                layer.push_text(
                    TextShape::new(
                        bookmark.url.clone(),
                        row.x + 8.0,
                        row.y + 19.0,
                        10.0,
                        theme.muted_foreground,
                    )
                    .width(title_width)
                    .ellipsis(),
                );
                self.push_hit(
                    Element::PopoverRow(bookmark.id),
                    Rect::new(row.x, row.y, title_width + 8.0, row.height),
                );

                let edit_hover = hover == Some(Element::PopoverEdit(bookmark.id));
                if edit_hover {
                    layer.push_rect(RectShape::filled(edit_rect, theme.accent, 12.0));
                }
                layer.push_icon(IconShape::new(
                    Icon::Edit,
                    icon_rect(edit_rect.center(), 12.0),
                    if edit_hover {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    },
                ));
                self.push_hit(Element::PopoverEdit(bookmark.id), edit_rect);

                let delete_hover = hover == Some(Element::PopoverDelete(bookmark.id));
                if delete_hover {
                    layer.push_rect(RectShape::filled(
                        delete_rect,
                        theme.destructive.with_alpha(0.1),
                        12.0,
                    ));
                }
                layer.push_icon(IconShape::new(
                    Icon::Trash,
                    icon_rect(delete_rect.center(), 12.0),
                    if delete_hover {
                        theme.destructive
                    } else {
                        theme.muted_foreground
                    },
                ));
                self.push_hit(Element::PopoverDelete(bookmark.id), delete_rect);
            }
        }

        if let Some(edit) = editing {
            let edit_top = list_y + list_height.max(empty_height) + 8.0;
            let separator = Rect::new(popover.x + 8.0, edit_top - 4.0, popover.width - 16.0, 1.0);
            layer.push_rect(RectShape::filled(separator, theme.border, 0.0));

            let title_rect = Rect::new(popover.x + 8.0, edit_top, popover.width - 16.0, 32.0);
            self.build_edit_input(
                layer,
                title_rect,
                edit.title.text(),
                edit.title.cursor(),
                focus == Some(Focus::BookmarkTitle),
                "Nama bookmark",
                Element::EditTitle,
            );

            let url_rect = Rect::new(popover.x + 8.0, edit_top + 40.0, popover.width - 16.0, 32.0);
            self.build_edit_input(
                layer,
                url_rect,
                edit.url.text(),
                edit.url.cursor(),
                focus == Some(Focus::BookmarkUrl),
                "https://...",
                Element::EditUrl,
            );

            let save_rect = Rect::new(popover.right() - 8.0 - 56.0, edit_top + 80.0, 56.0, 28.0);
            let cancel_rect = Rect::new(save_rect.x - 8.0 - 56.0, save_rect.y, 56.0, 28.0);
            let cancel_hover = hover == Some(Element::EditCancel);
            if cancel_hover {
                layer.push_rect(RectShape::filled(cancel_rect, theme.accent, 6.0));
            }
            layer.push_text(
                TextShape::new(
                    "Batal",
                    cancel_rect.x,
                    cancel_rect.y + 6.0,
                    12.0,
                    theme.popover_foreground,
                )
                .width(cancel_rect.width)
                .weight(500)
                .align(Align::Center),
            );
            self.push_hit(Element::EditCancel, cancel_rect);

            let save_hover = hover == Some(Element::EditSave);
            let save_bg = if save_hover {
                theme.primary.mix(Color::rgb(255, 255, 255), 0.15)
            } else {
                theme.primary
            };
            layer.push_rect(RectShape::filled(save_rect, save_bg, 6.0));
            layer.push_text(
                TextShape::new(
                    "Simpan",
                    save_rect.x,
                    save_rect.y + 6.0,
                    12.0,
                    theme.primary_foreground,
                )
                .width(save_rect.width)
                .weight(500)
                .align(Align::Center),
            );
            self.push_hit(Element::EditSave, save_rect);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_edit_input(
        &mut self,
        layer: &mut rb_ui::Layer,
        rect: Rect,
        value: &str,
        cursor: usize,
        focused: bool,
        placeholder: &str,
        element: Element,
    ) {
        let theme = self.theme;
        let border = if focused { theme.ring } else { theme.border };
        layer.push_rect(RectShape::bordered(
            rect,
            theme.background,
            4.0,
            1.0,
            border,
        ));
        let text_x = rect.x + 10.0;
        let text_width = (rect.width - 20.0).max(0.0);
        if value.is_empty() {
            layer.push_text(
                TextShape::new(
                    placeholder,
                    text_x,
                    rect.y + (rect.height - 12.0 * 1.3) * 0.5,
                    12.0,
                    theme.muted_foreground,
                )
                .width(text_width)
                .ellipsis(),
            );
        } else {
            layer.push_text(
                TextShape::new(
                    value.to_string(),
                    text_x,
                    rect.y + (rect.height - 12.0 * 1.3) * 0.5,
                    12.0,
                    theme.popover_foreground,
                )
                .width(text_width)
                .ellipsis()
                .caret(if focused { Some(cursor) } else { None }, theme.foreground),
            );
        }
        self.push_hit(element, rect);
    }

    fn build_context_menu(
        &mut self,
        frame: &mut Frame,
        width: f32,
        height: f32,
        bookmarks: &[Bookmark],
        menu: ContextMenu,
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        let item_height = 28.0;
        let menu_height = 4.0 + item_height * 3.0 + 4.0;
        let x = menu
            .position
            .x
            .max(8.0)
            .min((width - CONTEXT_WIDTH - 8.0).max(8.0));
        let y = menu
            .position
            .y
            .max(8.0)
            .min((height - menu_height - 8.0).max(8.0));
        let rect = Rect::new(x, y, CONTEXT_WIDTH, menu_height);
        let layer = &mut frame.overlay;
        layer.push_rect(RectShape::bordered(
            rect,
            theme.popover,
            6.0,
            1.0,
            theme.border,
        ));
        self.push_hit(Element::ContextArea, rect);

        let labels = ["Buka", "Edit", "Hapus"];
        let elements = [
            Element::ContextOpen,
            Element::ContextEdit,
            Element::ContextDelete,
        ];
        for (index, (label, element)) in labels.iter().zip(elements).enumerate() {
            let item = Rect::new(
                rect.x + 4.0,
                rect.y + 4.0 + index as f32 * item_height,
                rect.width - 8.0,
                item_height,
            );
            let is_hover = hover == Some(element);
            if is_hover {
                let color = if element == Element::ContextDelete {
                    theme.destructive.with_alpha(0.1)
                } else {
                    theme.muted
                };
                layer.push_rect(RectShape::filled(item, color, 4.0));
            }
            let color = if element == Element::ContextDelete {
                theme.destructive
            } else {
                theme.popover_foreground
            };
            layer.push_text(
                TextShape::new(*label, item.x + 8.0, item.y + 6.0, 12.0, color)
                    .width(item.width - 16.0),
            );
            self.push_hit(element, item);
        }

        let _ = bookmarks;
    }

    fn tab_menu_items(&self, menu: TabContextMenu) -> Vec<TabMenuItem> {
        let Some(tab) = self.tabs.tab(menu.id).cloned() else {
            return Vec::new();
        };
        let index = self.tabs.index_of(menu.id);
        let count = self.tabs.tabs().len();
        let mut items = Vec::new();
        let mut push = |label: &str,
                        element: Element,
                        icon: Icon,
                        destructive: bool,
                        separator_before: bool| {
            items.push(TabMenuItem {
                label: label.to_string(),
                element,
                icon,
                destructive,
                separator_before,
            });
        };

        if tab.pinned {
            push(
                "Unpin tab",
                Element::TabContextPin(menu.id),
                Icon::PinOff,
                false,
                false,
            );
        } else {
            push(
                "Pin tab",
                Element::TabContextPin(menu.id),
                Icon::Pin,
                false,
                false,
            );
        }
        push(
            "Duplicate",
            Element::TabContextDuplicate(menu.id),
            Icon::Duplicate,
            false,
            false,
        );
        push(
            "Reload",
            Element::TabContextReload(menu.id),
            Icon::Reload,
            false,
            false,
        );
        push(
            "New tab to the right",
            Element::TabContextNewTabRight(menu.id),
            Icon::Plus,
            false,
            true,
        );

        // Selalu ada "Add split..." untuk membuat grup baru dari beberapa tab.
        push(
            "Add split...",
            Element::TabContextSplit(menu.id),
            Icon::Split,
            false,
            true,
        );
        match self.split_index_of(menu.id) {
            Some(_) => {
                push(
                    "Remove from split view",
                    Element::TabContextRemoveFromSplit(menu.id),
                    Icon::Close,
                    false,
                    true,
                );
                push(
                    "Close split view",
                    Element::TabContextCloseSplit(menu.id),
                    Icon::Unsplit,
                    false,
                    true,
                );
            }
            None => {
                // Satu item per grup yang masih punya tempat. Anchor adalah anggota grup
                // itu, sehingga tab tujuan tetap benar walau indeks grup bergeser.
                for (index, split) in self.splits.iter().enumerate() {
                    if split.panes.len() < 3 {
                        push(
                            &format!("Add to Grup {}", index + 1),
                            Element::TabContextAddToSplit(menu.id, split.panes[0]),
                            Icon::Columns,
                            false,
                            true,
                        );
                    }
                }
            }
        }

        if count > 1 {
            push(
                "Close other tabs",
                Element::TabContextCloseOthers(menu.id),
                Icon::Close,
                false,
                true,
            );
        }
        if index.is_some_and(|index| index + 1 < count) {
            push(
                "Close tabs to the right",
                Element::TabContextCloseRight(menu.id),
                Icon::Close,
                false,
                false,
            );
        }
        push(
            "Close tab",
            Element::TabContextClose(menu.id),
            Icon::Close,
            true,
            true,
        );
        items
    }

    fn build_split_picker(
        &mut self,
        frame: &mut Frame,
        width: f32,
        height: f32,
        picker: &SplitPicker,
        hover: Option<Element>,
    ) {
        let theme = self.theme;
        let picker_width = (TAB_CONTEXT_WIDTH + 100.0).min((width - 16.0).max(160.0));
        let row_height = 30.0;
        let list_height = (self.tabs.tabs().len() as f32 * row_height).min(240.0);
        let picker_height = 8.0 + 30.0 + list_height + 8.0 + 34.0 + 8.0;
        let x = picker
            .position
            .x
            .max(8.0)
            .min((width - picker_width - 8.0).max(8.0));
        let y = picker
            .position
            .y
            .max(8.0)
            .min((height - picker_height - 8.0).max(8.0));
        let rect = Rect::new(x, y, picker_width, picker_height);
        let layer = &mut frame.overlay;
        layer.push_rect(RectShape::bordered(
            rect,
            theme.popover,
            6.0,
            1.0,
            theme.border,
        ));
        self.push_hit(Element::SplitPickerArea, rect);

        layer.push_text(
            TextShape::new(
                "Pilih tab untuk split (maks. 3)".to_string(),
                rect.x + 12.0,
                rect.y + 9.0,
                12.0,
                theme.popover_foreground,
            )
            .width(rect.width - 24.0)
            .ellipsis(),
        );
        let list_rect = Rect::new(rect.x + 6.0, rect.y + 38.0, rect.width - 12.0, list_height);
        let rows: Vec<(usize, TabId, String)> = self
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (index, tab.id, tab.title.clone()))
            .collect();
        for (index, id, title) in rows {
            let row_rect = Rect::new(
                list_rect.x,
                list_rect.y + index as f32 * row_height,
                list_rect.width,
                row_height,
            );
            if row_rect.bottom() > list_rect.bottom() {
                break;
            }
            if hover == Some(Element::SplitPickerRow(id)) {
                layer.push_rect(RectShape::filled(row_rect, theme.muted, 4.0));
            }
            let selected = picker.selected.contains(&id);
            let box_rect = Rect::new(row_rect.x + 10.0, row_rect.center().y - 7.0, 14.0, 14.0);
            if selected {
                layer.push_rect(RectShape::filled(box_rect, SPLIT_GROUP_COLOR, 3.0));
                layer.push_icon(IconShape::new(
                    Icon::Check,
                    icon_rect(box_rect.center(), 10.0),
                    theme.background,
                ));
            } else {
                layer.push_rect(RectShape::bordered(
                    box_rect,
                    Color::TRANSPARENT,
                    3.0,
                    1.0,
                    theme.border,
                ));
            }
            layer.push_text(
                TextShape::new(
                    title,
                    box_rect.right() + 10.0,
                    row_rect.y + 7.0,
                    12.0,
                    theme.popover_foreground,
                )
                .width((row_rect.right() - box_rect.right() - 18.0).max(0.0))
                .ellipsis(),
            );
            self.push_hit(Element::SplitPickerRow(id), row_rect);
        }

        let footer_y = list_rect.bottom() + 8.0;
        let cancel_rect = Rect::new(rect.x + 10.0, footer_y, 80.0, 32.0);
        let confirm_rect = Rect::new(rect.right() - 10.0 - 110.0, footer_y, 110.0, 32.0);
        let can_confirm = picker.selected.len() >= 2;
        let cancel_hover = hover == Some(Element::SplitPickerCancel);
        let confirm_hover = hover == Some(Element::SplitPickerConfirm);
        layer.push_rect(RectShape::filled(
            cancel_rect,
            if cancel_hover {
                theme.muted
            } else {
                theme.tab_inactive
            },
            4.0,
        ));
        layer.push_text(
            TextShape::new(
                "Batal",
                cancel_rect.center().x - 16.0,
                cancel_rect.y + 8.0,
                12.0,
                theme.popover_foreground,
            )
            .width(56.0),
        );
        layer.push_rect(RectShape::filled(
            confirm_rect,
            if !can_confirm {
                theme.tab_inactive
            } else if confirm_hover {
                SPLIT_GROUP_COLOR.mix(Color::rgb(255, 255, 255), 0.15)
            } else {
                SPLIT_GROUP_COLOR
            },
            4.0,
        ));
        layer.push_text(
            TextShape::new(
                format!("Split ({}/3)", picker.selected.len().min(3)),
                confirm_rect.x + 12.0,
                confirm_rect.y + 8.0,
                12.0,
                if can_confirm {
                    theme.background
                } else {
                    theme.muted_foreground
                },
            )
            .width(confirm_rect.width - 24.0),
        );
        self.push_hit(Element::SplitPickerCancel, cancel_rect);
        self.push_hit(Element::SplitPickerConfirm, confirm_rect);
    }

    fn build_tab_context_menu(
        &mut self,
        frame: &mut Frame,
        width: f32,
        height: f32,
        menu: TabContextMenu,
        hover: Option<Element>,
    ) {
        let items = self.tab_menu_items(menu);
        if items.is_empty() {
            return;
        }
        let theme = self.theme;
        let separators = items.iter().filter(|item| item.separator_before).count() as f32;
        let menu_height = 8.0 + items.len() as f32 * CONTEXT_ITEM_HEIGHT + separators * 6.0 + 8.0;
        let x = menu
            .position
            .x
            .max(8.0)
            .min((width - TAB_CONTEXT_WIDTH - 8.0).max(8.0));
        let y = menu
            .position
            .y
            .max(8.0)
            .min((height - menu_height - 8.0).max(8.0));
        let rect = Rect::new(x, y, TAB_CONTEXT_WIDTH, menu_height);
        let layer = &mut frame.overlay;
        layer.push_rect(RectShape::bordered(
            rect,
            theme.popover,
            6.0,
            1.0,
            theme.border,
        ));
        self.push_hit(Element::TabContextArea, rect);

        let mut y = rect.y + 4.0;
        for item in items {
            if item.separator_before {
                layer.push_rect(RectShape::filled(
                    Rect::new(rect.x + 4.0, y, rect.width - 8.0, 1.0),
                    theme.border,
                    0.0,
                ));
                y += 6.0;
            }
            let item_rect = Rect::new(rect.x + 4.0, y, rect.width - 8.0, CONTEXT_ITEM_HEIGHT);
            let hovered = hover == Some(item.element);
            if hovered {
                layer.push_rect(RectShape::filled(
                    item_rect,
                    if item.destructive {
                        theme.destructive.with_alpha(0.1)
                    } else {
                        theme.muted
                    },
                    4.0,
                ));
            }
            let color = if item.destructive {
                theme.destructive
            } else {
                theme.popover_foreground
            };
            layer.push_icon(
                IconShape::new(
                    item.icon,
                    icon_rect(Vec2::new(item_rect.x + 14.0, item_rect.center().y), 14.0),
                    color,
                )
                .stroke(1.7),
            );
            layer.push_text(
                TextShape::new(
                    item.label,
                    item_rect.x + 28.0,
                    item_rect.y + 7.0,
                    12.0,
                    color,
                )
                .width((item_rect.width - 36.0).max(0.0))
                .ellipsis(),
            );
            self.push_hit(item.element, item_rect);
            y += CONTEXT_ITEM_HEIGHT;
        }
    }
}

fn is_blank_url(url: &str) -> bool {
    url.is_empty()
        || url.starts_with("about:blank")
        || url.starts_with("data:")
        || url == NEW_TAB_URL
}

/// Favicon tab. Halaman kosong memakai ikon Globe vektor, bukan logo aplikasi.
fn push_tab_favicon(layer: &mut rb_ui::Layer, tab: &Tab, rect: Rect, theme: Theme) {
    if tab.url.as_deref().is_none_or(is_blank_url) {
        layer.push_icon(IconShape::new(Icon::Globe, rect, theme.muted_foreground));
    } else if let Some(source) = tab.favicon.clone() {
        layer.push_image(ImageShape::new(source, rect));
    }
}

fn icon_rect(center: Vec2, size: f32) -> Rect {
    Rect::new(center.x - size * 0.5, center.y - size * 0.5, size, size)
}

fn chrome_hover(hover: Option<Element>) -> Option<ChromeElement> {
    match hover {
        Some(Element::WindowMinimize) => Some(ChromeElement::Minimize),
        Some(Element::WindowMaximizeToggle) => Some(ChromeElement::MaximizeToggle),
        Some(Element::WindowClose) => Some(ChromeElement::Close),
        _ => None,
    }
}

fn char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn approx_text_width(text: &str, size: f32) -> f32 {
    rb_ui::text_input::text_width(text, size)
}

fn normalize_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.to_ascii_lowercase().starts_with("http://")
        || trimmed.to_ascii_lowercase().starts_with("https://")
    {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = url::Url::parse(&candidate).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    parsed.host_str()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    Some(parsed.to_string())
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return true;
    }
    if value.chars().any(|ch| ch.is_whitespace()) {
        return false;
    }
    if lower == "localhost" || lower.starts_with("localhost:") || lower.starts_with("localhost/") {
        return true;
    }
    let host = value.split('/').next().unwrap_or(value);
    host.contains('.')
}

fn resolve_target(value: &str) -> String {
    let trimmed = value.trim();
    if looks_like_url(trimmed) {
        if let Some(url) = normalize_url(trimmed) {
            return url;
        }
    }
    format!(
        "https://www.google.com/search?q={}",
        percent_encode(trimmed)
    )
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn host_from_url(value: &str) -> String {
    if value.is_empty() {
        return "Tab baru".to_string();
    }
    match url::Url::parse(value) {
        Ok(parsed) => parsed
            .host_str()
            .map(|host| host.trim_start_matches("www.").to_string())
            .unwrap_or_else(|| "Tab baru".to_string()),
        Err(_) => "Tab baru".to_string(),
    }
}

fn text_input(text: impl Into<String>) -> TextInput {
    let mut input = TextInput::new();
    input.set_text(text);
    input
}

/// Bayangan pudar di tepi kanan judul tab yang terpotong (8 langkah, 2 px per langkah).
fn push_title_fade(layer: &mut rb_ui::Layer, rect: Rect, title_right: f32, color: Color) {
    const STEPS: usize = 8;
    const STEP_WIDTH: f32 = 2.0;
    let start = title_right - STEP_WIDTH * STEPS as f32;
    for step in 0..STEPS {
        let t = (step as f32 + 1.0) / STEPS as f32;
        let alpha = (t * t * 255.0).round() as u8;
        layer.push_rect(RectShape::filled(
            Rect::new(
                start + step as f32 * STEP_WIDTH,
                rect.y + 1.0,
                STEP_WIDTH,
                rect.height - 2.0,
            ),
            Color::rgba(color.r, color.g, color.b, alpha),
            0.0,
        ));
    }
}

#[cfg(test)]
mod tests {

    /// Pembantu pengujian: membuat grup dari tab aktif dan `id`. Jika `id` adalah tab
    /// aktif, tab itu diduplikasi lebih dulu (perilaku split langsung dari menu).
    impl BrowserShell {
        fn start_split(&mut self, id: TabId) {
            let Some(active) = self.tabs.active_id() else {
                return;
            };
            let second = if id == active {
                match self.duplicate_tab(id) {
                    Some(duplicated) => duplicated,
                    None => return,
                }
            } else {
                id
            };
            self.detach_from_splits(active);
            self.detach_from_splits(second);
            let color = self.next_split_color();
            self.splits.push(SplitView::new(active, second, color));
            self.sync_split_order();
            self.activate_tab(second);
        }
    }

    use super::*;

    #[test]
    fn new_tab_never_exposes_about_blank_in_the_address_bar() {
        let mut shell = BrowserShell::new();
        assert_eq!(
            shell.tabs.active().unwrap().url.as_deref(),
            Some(NEW_TAB_URL)
        );
        assert!(shell.address.text().is_empty());

        shell.new_tab();
        let request = shell.take_navigation().expect("new tab navigation");
        assert_eq!(request.url, NEW_TAB_URL);
        assert!(shell.address.text().is_empty());
    }

    #[test]
    fn webview_blank_urls_do_not_replace_the_new_tab() {
        let mut shell = BrowserShell::new();
        let tab_id = shell.active_tab_id().unwrap();
        shell.webview_url_changed(tab_id, "about:blank");
        shell.webview_url_changed(tab_id, "");
        assert_eq!(
            shell.tabs.active().unwrap().url.as_deref(),
            Some(NEW_TAB_URL)
        );
        assert!(shell.address.text().is_empty());
    }

    #[test]
    fn real_urls_still_update_the_address_bar() {
        let mut shell = BrowserShell::new();
        let tab_id = shell.active_tab_id().unwrap();
        shell.webview_url_changed(tab_id, "https://example.com/a");
        assert_eq!(shell.address.text(), "https://example.com/a");
    }

    #[test]
    fn new_tab_cannot_be_bookmarked() {
        let mut shell = BrowserShell::new();
        shell.toggle_bookmark();
        assert!(shell.bookmarks.is_empty());
    }

    #[test]
    fn new_tab_focuses_the_address_bar() {
        let mut shell = BrowserShell::new();
        shell.new_tab();
        assert!(shell.is_editing());
        assert_eq!(shell.focus, Some(Focus::Address));
    }

    #[test]
    fn each_split_group_gets_its_own_border_color() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.start_split(first);
        shell.new_tab();
        let c = shell.active_tab_id().unwrap();
        shell.new_tab();
        let d = shell.active_tab_id().unwrap();
        shell.start_split(c);
        assert_eq!(shell.splits.len(), 2);
        assert_ne!(shell.splits[0].color, shell.splits[1].color);
        assert!(shell.split_of(d).is_none());
    }

    #[test]
    fn add_to_split_joins_the_chosen_group_only() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.start_split(first);
        shell.new_tab();
        let c = shell.active_tab_id().unwrap();
        shell.new_tab();
        let e = shell.active_tab_id().unwrap();
        shell.start_split(c);
        let anchor = shell.splits[0].panes[0];
        shell.add_to_split(e, anchor);
        assert_eq!(shell.splits.len(), 2);
        assert_eq!(shell.splits[0].panes.len(), 3);
        assert!(shell.splits[0].panes.contains(&e));
        assert_eq!(shell.splits[1].panes.len(), 2);
    }

    #[test]
    fn windowed_frame_has_border_and_maximized_does_not() {
        let mut shell = BrowserShell::new();
        shell.set_maximized(false);
        let windowed = shell.render(900.0, 600.0);
        assert!(windowed
            .base
            .shapes
            .iter()
            .any(|shape| shape.border_width > 0.0 && shape.rect.width >= 899.0));

        shell.set_maximized(true);
        let maximized = shell.render(900.0, 600.0);
        assert!(!maximized
            .base
            .shapes
            .iter()
            .any(|shape| shape.border_width > 0.0 && shape.rect.width >= 899.0));
    }

    #[test]
    fn right_click_on_tab_opens_its_context_menu() {
        let mut shell = BrowserShell::new();
        shell.render(900.0, 600.0);
        let position = shell.tab_rects[0].1.center();

        shell.right_click(position);

        assert_eq!(
            shell.tab_context_menu.map(|menu| menu.id),
            shell.tabs.active_id()
        );
        assert!(shell.context_menu.is_none());
    }

    #[test]
    fn split_view_grows_to_three_resizable_panes() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.new_tab();
        let third = shell.active_tab_id().unwrap();
        shell.tabs.activate(second);
        shell.start_split(first);
        shell.add_to_split(third, first);

        let split = shell.shown_split().expect("split view");
        assert_eq!(split.panes.len(), 3);
        assert!(split.panes.contains(&second));
        assert!(split.panes.contains(&first));
        assert!(split.panes.contains(&third));

        shell.update_split_divider(0, 200.0);
        assert!((0.18..=0.34).contains(&shell.shown_split().unwrap().ratios[0]));

        let layout = shell.webview_layout(Rect::new(0.0, 0.0, 900.0, 500.0));
        assert_eq!(layout.len(), 3);
        for pair in layout.windows(2) {
            assert!(pair[0].1.right() < pair[1].1.x);
        }
        shell.close_split(second);
        assert!(shell.splits.is_empty());
    }

    #[test]
    fn split_starts_with_a_tab_picker() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.tabs.activate(first);
        shell.render(900.0, 600.0);
        let position = shell.tab_rects[1].1.center();

        shell.right_click(position);
        let menu = shell.tab_context_menu.unwrap();
        let split_element = shell
            .tab_menu_items(menu)
            .into_iter()
            .find(|item| matches!(item.element, Element::TabContextSplit(_)))
            .map(|item| item.element)
            .expect("split menu item");
        shell.activate(split_element);

        assert!(shell.splits.is_empty());
        assert_eq!(shell.split_picker.as_ref().unwrap().selected.len(), 2);
        shell.activate(Element::SplitPickerRow(first));
        assert_eq!(shell.split_picker.as_ref().unwrap().selected.len(), 1);
        shell.activate(Element::SplitPickerConfirm);
        assert!(shell.splits.is_empty());
        shell.activate(Element::SplitPickerRow(first));
        shell.activate(Element::SplitPickerConfirm);

        let split = shell.shown_split().expect("split view");
        assert_eq!(split.panes.len(), 2);
        assert!(split.panes.contains(&first));
        assert!(split.panes.contains(&second));
        assert!(shell.split_picker.is_none());
    }

    #[test]
    fn new_tab_opens_outside_the_split_and_the_group_stays_intact() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.tabs.activate(first);
        shell.start_split(second);
        let old_panes = shell.shown_split().unwrap().panes.clone();

        shell.new_tab();
        let new_id = shell.active_tab_id().unwrap();

        assert!(!old_panes.contains(&new_id));
        assert_eq!(shell.shown_split().unwrap().panes, old_panes);
        assert_eq!(shell.focused_tab_id(), Some(new_id));
        // Tab baru tampil penuh, bukan di dalam split view.
        assert_eq!(
            shell
                .webview_layout(Rect::new(0.0, 0.0, 900.0, 500.0))
                .len(),
            1
        );
        // Grup di tab strip tetap digambar.
        let frame = shell.render(900.0, 600.0);
        assert!(frame.base.shapes.iter().any(|shape| {
            shape.border_color == SPLIT_GROUP_COLOR
                && shape.border_width >= 2.0
                && shape.radius >= TAB_RADIUS
                && shape.rect.bottom() <= TAB_STRIP_HEIGHT
        }));

        shell.activate(Element::Tab(first));
        assert_eq!(shell.focused_tab_id(), Some(first));
        assert_eq!(
            shell
                .webview_layout(Rect::new(0.0, 0.0, 900.0, 500.0))
                .len(),
            2
        );
        shell.close_split(first);
        assert!(shell.splits.is_empty());
    }

    #[test]
    fn switching_tabs_and_opening_new_tabs_never_change_the_group() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.tabs.activate(first);
        shell.start_split(second);
        let before = shell.shown_split().unwrap().clone();

        shell.cycle_tabs(1);
        shell.cycle_tabs(1);
        shell.new_tab();
        shell.new_tab();
        let outside = shell.active_tab_id().unwrap();
        shell.cycle_tabs(-1);
        shell.activate(Element::Tab(outside));
        shell.cycle_tabs(-1);

        let after = shell.shown_split().expect("group still exists");
        assert_eq!(after.panes, before.panes);
        assert_eq!(after.ratios, before.ratios);

        // Membuka kembali grup: tab anggota harus menampilkan split view lagi.
        shell.activate(Element::Tab(first));
        assert_eq!(
            shell
                .webview_layout(Rect::new(0.0, 0.0, 900.0, 500.0))
                .len(),
            2
        );
    }

    #[test]
    fn clicking_outside_tab_shows_it_alone_without_joining_the_split() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.tabs.activate(first);
        shell.start_split(second);
        let panes = shell.shown_split().unwrap().panes.clone();

        let outside = shell
            .tabs
            .open("Luar", Some("https://example.com".to_string()));
        shell.activate(Element::Tab(outside));

        assert_eq!(shell.shown_split().unwrap().panes, panes);
        assert!(!shell.shown_split().unwrap().panes.contains(&outside));
        assert_eq!(shell.focused_tab_id(), Some(outside));
        assert_eq!(
            shell
                .webview_layout(Rect::new(0.0, 0.0, 900.0, 500.0))
                .len(),
            1
        );
    }

    #[test]
    fn split_members_are_grouped_together_in_the_strip() {
        let mut shell = BrowserShell::new();
        let a = shell.active_tab_id().unwrap();
        shell.new_tab();
        let b = shell.active_tab_id().unwrap();
        shell.new_tab();
        let c = shell.active_tab_id().unwrap();
        shell.new_tab();
        let d = shell.active_tab_id().unwrap();
        shell.tabs.activate(a);
        // a dan c dipilih, b berada di antaranya. Setelah grup terbentuk, b harus pindah.
        shell.start_split(c);

        let order: Vec<TabId> = shell.tabs.tabs().iter().map(|tab| tab.id).collect();
        let index = |id: TabId| order.iter().position(|x| *x == id).unwrap();
        assert_eq!(index(c), index(a) + 1);
        assert_eq!(index(b), index(c) + 1);
        assert_eq!(index(d), index(b) + 1);
    }

    #[test]
    fn outside_tab_cannot_be_dragged_into_the_middle_of_the_group() {
        let mut shell = BrowserShell::new();
        shell.new_tab();
        shell.new_tab();
        shell.new_tab();
        let ids: Vec<TabId> = shell.tabs.tabs().iter().map(|tab| tab.id).collect();
        let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
        shell.tabs.activate(b);
        shell.start_split(c);
        assert_eq!(shell.shown_split().unwrap().panes, vec![b, c]);

        shell.render(900.0, 600.0);
        let from = shell.tab_rects.iter().find(|(id, _)| *id == d).unwrap().1;
        let to = shell.tab_rects.iter().find(|(id, _)| *id == b).unwrap().1;
        shell.left_click(from.center());
        shell.render(900.0, 600.0);
        // Gerakan kecil untuk memulai drag, lalu tarik ke sisi kanan tab b.
        shell.pointer_moved(Vec2::new(from.center().x + 10.0, from.center().y));
        shell.render(900.0, 600.0);
        shell.pointer_moved(Vec2::new(to.center().x + 2.0, to.center().y));

        let order: Vec<TabId> = shell.tabs.tabs().iter().map(|tab| tab.id).collect();
        let index = |id: TabId| order.iter().position(|x| *x == id).unwrap();
        assert_eq!(
            index(c),
            index(b) + 1,
            "grup tidak boleh terbelah: {order:?}"
        );
        assert_eq!(index(a), 0);
        assert!(index(d) < index(b) || index(d) > index(c));
    }

    #[test]
    fn dragging_one_split_tab_moves_the_whole_group() {
        let mut shell = BrowserShell::new();
        let first = shell.active_tab_id().unwrap();
        shell.new_tab();
        let second = shell.active_tab_id().unwrap();
        shell.new_tab();
        shell.new_tab();
        shell.tabs.activate(first);
        shell.start_split(second);
        let frame = shell.render(900.0, 600.0);
        assert!(frame
            .base
            .shapes
            .iter()
            .any(|shape| shape.color == SPLIT_GROUP_COLOR && shape.border_width == 0.0));
        assert!(frame
            .base
            .shapes
            .iter()
            .filter(|shape| shape.color == SPLIT_GROUP_COLOR)
            .all(|shape| shape.rect.bottom() <= TAB_STRIP_HEIGHT));
        assert!(frame.base.shapes.iter().any(|shape| {
            shape.border_color == SPLIT_GROUP_COLOR
                && shape.border_width >= 2.0
                && shape.radius >= TAB_RADIUS
                && shape.rect.bottom() <= TAB_STRIP_HEIGHT
        }));
        assert!(frame
            .base
            .shapes
            .iter()
            .filter(|shape| shape.border_color == SPLIT_GROUP_COLOR)
            .all(|shape| shape.rect.bottom() <= TAB_STRIP_HEIGHT));
        let position = shell.tab_rects[1].1.center();

        shell.left_click(position);
        assert_eq!(shell.tab_drag.as_ref().unwrap().ids.len(), 2);
        shell.render(900.0, 600.0);
        shell.pointer_moved(Vec2::new(position.x + 300.0, position.y));

        let order: Vec<TabId> = shell.tabs.tabs().iter().map(|tab| tab.id).collect();
        let first_index = order.iter().position(|id| *id == first).unwrap();
        let second_index = order.iter().position(|id| *id == second).unwrap();
        assert_eq!(second_index, first_index + 1);
        assert_eq!(shell.shown_split().unwrap().panes, vec![first, second]);
    }

    #[test]
    fn context_menu_duplicate_and_pin_change_tab_state() {
        let mut shell = BrowserShell::new();
        let original = shell.active_tab_id().unwrap();
        shell.duplicate_tab(original);
        let duplicate = shell.active_tab_id().unwrap();
        assert_ne!(duplicate, original);
        assert_eq!(shell.tabs.tabs().len(), 2);

        shell.tabs.toggle_pin(original);
        assert_eq!(shell.tabs.tabs()[0].id, original);
        assert!(shell.tabs.is_pinned(original));
    }
}
