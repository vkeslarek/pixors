/// A point in CIE xy chromaticity space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticity {
    pub x: f32,
    pub y: f32,
}

impl Eq for Chromaticity {} // safe: f32 values are compile-time constants, never NaN

impl Chromaticity {
    /// Creates a new chromaticity point.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the z coordinate (1 - x - y).
    pub fn z(&self) -> f32 {
        1.0 - self.x - self.y
    }

    /// Converts to XYZ tristimulus values with given luminance Y = 1.
    pub fn to_xyz(&self) -> (f32, f32, f32) {
        let x = self.x / self.y;
        let y = 1.0;
        let z = self.z() / self.y;
        (x, y, z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn chromaticity_z() {
        let c = Chromaticity::new(0.3, 0.4);
        assert_approx_eq!(c.z(), 0.3);
    }

    #[test]
    fn chromaticity_to_xyz() {
        let c = Chromaticity::new(0.3, 0.4);
        let (x, y, z) = c.to_xyz();
        assert_approx_eq!(x, 0.3 / 0.4);
        assert_approx_eq!(y, 1.0);
        assert_approx_eq!(z, (1.0 - 0.3 - 0.4) / 0.4);
    }
}