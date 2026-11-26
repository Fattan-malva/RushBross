pub use lucide_static_svg::Icon as LucideIcon;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Search,
    Lock,
    Star,
    StarFilled,
    Reload,
    Loader,
    Volume,
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
    ArrowLeft,
    ArrowRight,
    Menu,
    Profile,
    Pin,
    PinOff,
    Split,
    Columns,
    Duplicate,
    Check,
    /// Tab kosong (halaman baru) di tab strip dan header pane.
    Globe,
    /// Keluar dari split view dan kembali ke satu tab penuh.
    Unsplit,
    RushAgent,
    Terminal,
}

impl Icon {
    pub fn lucide(self) -> Option<LucideIcon> {
        match self {
            Self::Search => Some(LucideIcon::Search),
            Self::Lock => Some(LucideIcon::Lock),
            Self::Star | Self::StarFilled => Some(LucideIcon::Star),
            Self::Reload => Some(LucideIcon::RotateCw),
            Self::Loader => Some(LucideIcon::LoaderCircle),
            Self::Volume => Some(LucideIcon::Volume2),
            Self::Plus => Some(LucideIcon::Plus),
            Self::Close => Some(LucideIcon::X),
            Self::More => Some(LucideIcon::Ellipsis),
            Self::Bookmark => Some(LucideIcon::Bookmark),
            Self::Edit => Some(LucideIcon::Pencil),
            Self::Trash => Some(LucideIcon::Trash),
            Self::Minimize => Some(LucideIcon::Minus),
            Self::Maximize => Some(LucideIcon::Square),
            Self::Restore => Some(LucideIcon::Copy),
            Self::ChevronLeft => Some(LucideIcon::ChevronLeft),
            Self::ChevronRight => Some(LucideIcon::ChevronRight),
            Self::ArrowLeft => Some(LucideIcon::ArrowLeft),
            Self::ArrowRight => Some(LucideIcon::ArrowRight),
            Self::Menu => Some(LucideIcon::Menu),
            Self::Profile => Some(LucideIcon::CircleUserRound),
            Self::Pin => Some(LucideIcon::Pin),
            Self::PinOff => Some(LucideIcon::PinOff),
            Self::Split => Some(LucideIcon::Split),
            Self::Columns => Some(LucideIcon::Columns3),
            Self::Duplicate => Some(LucideIcon::CopyPlus),
            Self::Check => Some(LucideIcon::Check),
            Self::Globe => Some(LucideIcon::Globe),
            Self::Unsplit => Some(LucideIcon::Maximize2),
            Self::RushAgent => Some(LucideIcon::SquareTerminal),
            Self::Terminal => Some(LucideIcon::Terminal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_resolves_to_a_lucide_icon() {
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
            Icon::Pin,
            Icon::PinOff,
            Icon::Split,
            Icon::Columns,
            Icon::Duplicate,
            Icon::Check,
            Icon::Globe,
            Icon::Unsplit,
            Icon::RushAgent,
            Icon::Terminal,
        ];
        for icon in icons {
            let lucide = icon.lucide().expect("lucide icon");
            assert!(lucide.svg_str().contains("viewBox=\"0 0 24 24\""));
        }
    }
}
