//! Alpha representation mode.

/// Alpha representation (straight, premultiplied, or absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    /// Straight (unassociated) alpha: color channels are independent of alpha.
    /// The displayed color is `(r, g, b) * α` when compositing.
    Straight,

    /// Premultiplied (associated) alpha: color channels are already multiplied by alpha.
    /// The stored value is `(r*α, g*α, b*α, α)`.
    Premultiplied,

    /// No alpha channel; equivalent to α = 1.0 everywhere.
    Opaque,
}

impl AlphaMode {
    /// Returns `true` if the mode has an alpha channel (Straight or Premultiplied).
    pub fn has_alpha(self) -> bool {
        matches!(self, AlphaMode::Straight | AlphaMode::Premultiplied)
    }

    /// Returns `true` if alpha is straight (unassociated).
    pub fn is_straight(self) -> bool {
        matches!(self, AlphaMode::Straight)
    }

    /// Returns `true` if alpha is premultiplied.
    pub fn is_premultiplied(self) -> bool {
        matches!(self, AlphaMode::Premultiplied)
    }

    /// Returns `true` if alpha is opaque (no alpha channel).
    pub fn is_opaque(self) -> bool {
        matches!(self, AlphaMode::Opaque)
    }
}