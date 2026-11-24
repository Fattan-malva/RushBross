use rb_ui::{Icon, IconShape, Layer, Rect, RectShape, Theme, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeAction {
    Minimize,
    MaximizeToggle,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeElement {
    Minimize,
    MaximizeToggle,
    Close,
    DragStrip,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowChrome {
    buttons: [Rect; 3],
    drag: Rect,
}

impl WindowChrome {
    const BUTTON_SIZE: f32 = 32.0;
    const SLOT_WIDTH: f32 = 44.0;
    const RIGHT_PADDING: f32 = 8.0;
    const STRIP_HEIGHT: f32 = 40.0;

    pub fn reserved_width() -> f32 {
        Self::SLOT_WIDTH * 3.0 + Self::RIGHT_PADDING
    }

    pub fn layout(width: f32, after_x: f32) -> Self {
        let start = (width - Self::reserved_width()).max(after_x);
        let y = (Self::STRIP_HEIGHT - Self::BUTTON_SIZE) * 0.5;
        let buttons = [
            Rect::new(start, y, Self::BUTTON_SIZE, Self::BUTTON_SIZE),
            Rect::new(
                start + Self::SLOT_WIDTH,
                y,
                Self::BUTTON_SIZE,
                Self::BUTTON_SIZE,
            ),
            Rect::new(
                start + Self::SLOT_WIDTH * 2.0,
                y,
                Self::BUTTON_SIZE,
                Self::BUTTON_SIZE,
            ),
        ];
        let drag = Rect::new(
            after_x,
            0.0,
            (start - 6.0 - after_x).max(0.0),
            Self::STRIP_HEIGHT,
        );
        Self { buttons, drag }
    }

    pub fn hit(&self, point: Vec2) -> Option<ChromeElement> {
        let elements = [
            ChromeElement::Minimize,
            ChromeElement::MaximizeToggle,
            ChromeElement::Close,
        ];
        for (rect, element) in self.buttons.iter().zip(elements) {
            if rect.contains(point) {
                return Some(element);
            }
        }
        if self.drag.contains(point) {
            return Some(ChromeElement::DragStrip);
        }
        None
    }

    pub fn drag_rect(&self) -> Rect {
        self.drag
    }

    pub fn hit_regions(&self) -> [(Rect, ChromeElement); 4] {
        [
            (self.buttons[0], ChromeElement::Minimize),
            (self.buttons[1], ChromeElement::MaximizeToggle),
            (self.buttons[2], ChromeElement::Close),
            (self.drag, ChromeElement::DragStrip),
        ]
    }

    pub fn draw(
        &self,
        layer: &mut Layer,
        theme: Theme,
        hover: Option<ChromeElement>,
        maximized: bool,
    ) {
        let icons = [
            Icon::Minimize,
            if maximized {
                Icon::Restore
            } else {
                Icon::Maximize
            },
            Icon::Close,
        ];
        let elements = [
            ChromeElement::Minimize,
            ChromeElement::MaximizeToggle,
            ChromeElement::Close,
        ];

        for (index, rect) in self.buttons.iter().enumerate() {
            let is_hover = hover == Some(elements[index]);
            if is_hover {
                let color = if index == 2 {
                    theme.destructive
                } else {
                    theme.accent
                };
                layer.push_rect(RectShape::filled(*rect, color, Self::BUTTON_SIZE * 0.5));
            }
            let color = if is_hover {
                theme.primary_foreground
            } else {
                theme.muted_foreground
            };
            let icon_rect = Rect::new(rect.x + 9.0, rect.y + 9.0, 14.0, 14.0);
            layer.push_icon(IconShape::new(icons[index], icon_rect, color).stroke(1.6));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_layout_places_buttons_on_the_right() {
        let chrome = WindowChrome::layout(900.0, 120.0);
        assert!(chrome.buttons[0].x > 700.0);
        assert!(chrome.buttons[2].right() <= 900.0);
        assert_eq!(chrome.drag_rect().width, chrome.buttons[0].x - 126.0);
    }

    #[test]
    fn chrome_hit_prefers_buttons_over_drag_strip() {
        let chrome = WindowChrome::layout(900.0, 120.0);
        let center = chrome.buttons[1].center();
        assert_eq!(chrome.hit(center), Some(ChromeElement::MaximizeToggle));
        assert_eq!(
            chrome.hit(Vec2::new(200.0, 20.0)),
            Some(ChromeElement::DragStrip)
        );
    }
}
