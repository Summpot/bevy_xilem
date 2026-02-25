#![forbid(unsafe_code)]

/// Preferred family name exposed by the bundled Lucide font.
pub const LUCIDE_FONT_FAMILY: &str = "Lucide Icons";

/// Raw TrueType bytes for Lucide glyph rendering.
pub const LUCIDE_FONT_BYTES: &[u8] = lucide_icons::LUCIDE_FONT_BYTES;

/// Narrow icon set currently used by `bevy_xilem` built-in widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BevyXilemIcon {
    ChevronDown,
    ChevronUp,
    ChevronRight,
    Circle,
    CircleDot,
}

impl BevyXilemIcon {
    #[must_use]
    pub const fn as_lucide(self) -> lucide_icons::Icon {
        match self {
            Self::ChevronDown => lucide_icons::Icon::ChevronDown,
            Self::ChevronUp => lucide_icons::Icon::ChevronUp,
            Self::ChevronRight => lucide_icons::Icon::ChevronRight,
            Self::Circle => lucide_icons::Icon::Circle,
            Self::CircleDot => lucide_icons::Icon::CircleDot,
        }
    }

    #[must_use]
    pub fn glyph(self) -> char {
        char::from(self.as_lucide())
    }
}
