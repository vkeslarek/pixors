/// Alpha representation (straight, premultiplied, or absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Straight,
    Premultiplied,
    Opaque,
}

impl AlphaMode {
    pub fn has_alpha(self) -> bool {
        matches!(self, AlphaMode::Straight | AlphaMode::Premultiplied)
    }

    pub fn is_straight(self) -> bool {
        matches!(self, AlphaMode::Straight)
    }

    pub fn is_premultiplied(self) -> bool {
        matches!(self, AlphaMode::Premultiplied)
    }

    pub fn is_opaque(self) -> bool {
        matches!(self, AlphaMode::Opaque)
    }
}
