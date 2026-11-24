use crate::geom::Color;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub muted_foreground: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub border: Color,
    pub ring: Color,
    pub popover: Color,
    pub popover_foreground: Color,
    pub primary: Color,
    pub primary_foreground: Color,
    pub destructive: Color,
    pub tab_strip: Color,
    pub tab_inactive: Color,
    pub tab_hover: Color,
    pub star: Color,
    pub star_fill: Color,
}

impl Theme {
    pub fn dark() -> Self {
        let background = Color::rgb(0x0a, 0x0a, 0x0a);
        let muted = Color::rgb(0x2b, 0x2b, 0x2b);

        Self {
            background,
            foreground: Color::rgb(0xf5, 0xf5, 0xf5),
            muted,
            muted_foreground: Color::rgb(0xa6, 0xa6, 0xa6),
            accent: Color::rgb(0x33, 0x33, 0x33),
            accent_foreground: Color::rgb(0xf5, 0xf5, 0xf5),
            border: Color::rgb(0x2a, 0x2a, 0x2a),
            ring: Color::rgba(0xff, 0xff, 0xff, 0x4d),
            popover: Color::rgb(0x21, 0x21, 0x21),
            popover_foreground: Color::rgb(0xf5, 0xf5, 0xf5),
            primary: Color::rgb(0xf5, 0xf5, 0xf5),
            primary_foreground: Color::rgb(0x14, 0x14, 0x14),
            destructive: Color::rgb(0xef, 0x53, 0x50),
            tab_strip: muted.mix(background, 0.6),
            tab_inactive: muted.mix(background, 0.4),
            tab_hover: muted,
            star: Color::rgb(0xe6, 0xa7, 0x00),
            star_fill: Color::rgb(0xfa, 0xcc, 0x15),
        }
    }
}
