use crate::geom::{Color, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFamily {
    Sans,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Search,
    Lock,
    Star,
    StarFilled,
    Reload,
    Plus,
    Close,
    More,
    Bookmark,
    Edit,
    Trash,
    Minimize,
    Maximize,
    Restore,
    ChevronLeft,
    ChevronRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct RectShape {
    pub rect: Rect,
    pub color: Color,
    pub radius: f32,
    pub border_width: f32,
    pub border_color: Color,
}

impl RectShape {
    pub fn filled(rect: Rect, color: Color, radius: f32) -> Self {
        Self {
            rect,
            color,
            radius,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        }
    }

    pub fn bordered(
        rect: Rect,
        color: Color,
        radius: f32,
        border_width: f32,
        border_color: Color,
    ) -> Self {
        Self {
            rect,
            color,
            radius,
            border_width,
            border_color,
        }
    }
}

#[derive(Clone, Debug)]
pub struct IconShape {
    pub icon: Icon,
    pub rect: Rect,
    pub color: Color,
    pub stroke: f32,
}

impl IconShape {
    pub fn new(icon: Icon, rect: Rect, color: Color) -> Self {
        Self {
            icon,
            rect,
            color,
            stroke: (rect.width.min(rect.height) * 0.11).max(1.0),
        }
    }

    pub fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = stroke.max(0.5);
        self
    }
}

#[derive(Clone, Debug)]
pub struct TextShape {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub size: f32,
    pub color: Color,
    pub weight: u16,
    pub family: FontFamily,
    pub align: Align,
    pub ellipsis: bool,
    pub wrap: bool,
    pub line_height: f32,
    pub clip: Option<Rect>,
    pub caret: Option<usize>,
    pub caret_color: Color,
}

impl TextShape {
    pub fn new(text: impl Into<String>, x: f32, y: f32, size: f32, color: Color) -> Self {
        Self {
            text: text.into(),
            x,
            y,
            width: f32::INFINITY,
            size,
            color,
            weight: 400,
            family: FontFamily::Sans,
            align: Align::Left,
            ellipsis: false,
            wrap: false,
            line_height: 1.25,
            clip: None,
            caret: None,
            caret_color: color,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn ellipsis(mut self) -> Self {
        self.ellipsis = true;
        self
    }

    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height.max(1.0);
        self
    }

    pub fn clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn caret(mut self, caret: Option<usize>, caret_color: Color) -> Self {
        self.caret = caret;
        self.caret_color = caret_color;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub shapes: Vec<RectShape>,
    pub icons: Vec<IconShape>,
    pub texts: Vec<TextShape>,
}

impl Layer {
    pub fn push_rect(&mut self, shape: RectShape) {
        self.shapes.push(shape);
    }

    pub fn push_icon(&mut self, icon: IconShape) {
        self.icons.push(icon);
    }

    pub fn push_text(&mut self, text: TextShape) {
        self.texts.push(text);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub base: Layer,
    pub overlay: Layer,
}
