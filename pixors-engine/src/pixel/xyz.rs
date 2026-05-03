//! CIE XYZ and xyY color spaces.

use crate::color::Chromaticity;

/// CIE XYZ tristimulus values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xyz<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T> Xyz<T> {
    /// Creates new XYZ values.
    pub const fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }
}

impl<T: Copy> Xyz<T> {
    /// Returns the components as a tuple.
    pub fn as_tuple(&self) -> (T, T, T) {
        (self.x, self.y, self.z)
    }

    /// Returns the components as an array.
    pub fn as_array(&self) -> [T; 3] {
        [self.x, self.y, self.z]
    }
}

impl Xyz<f32> {
    /// Converts XYZ to xyY chromaticity plus luminance.
    pub fn to_xyy(&self) -> Xyy<f32> {
        let sum = self.x + self.y + self.z;
        if sum == 0.0 {
            Xyy::new(0.0, 0.0, self.y)
        } else {
            Xyy::new(self.x / sum, self.y / sum, self.y)
        }
    }

    /// Converts XYZ to chromaticity (xy) only.
    pub fn to_chromaticity(&self) -> Chromaticity {
        let sum = self.x + self.y + self.z;
        if sum == 0.0 {
            Chromaticity::new(0.0, 0.0)
        } else {
            Chromaticity::new(self.x / sum, self.y / sum)
        }
    }
}

/// CIE xyY color space (chromaticity x, y plus luminance Y).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xyy<T> {
    pub x: f32,
    pub y: f32,
    pub lum: T,
}

impl<T> Xyy<T> {
    /// Creates new xyY values.
    pub const fn new(x: f32, y: f32, lum: T) -> Self {
        Self { x, y, lum }
    }
}

impl Xyy<f32> {
    /// Converts xyY to XYZ.
    pub fn to_xyz(&self) -> Xyz<f32> {
        if self.y == 0.0 {
            Xyz::new(0.0, 0.0, 0.0)
        } else {
            let factor = self.lum / self.y;
            Xyz::new(self.x * factor, self.lum, (1.0 - self.x - self.y) * factor)
        }
    }

    /// Returns the chromaticity (x, y).
    pub fn chromaticity(&self) -> Chromaticity {
        Chromaticity::new(self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn xyz_to_xyy() {
        let xyz = Xyz::new(0.5, 0.3, 0.2);
        let xyy = xyz.to_xyy();
        let sum = 0.5 + 0.3 + 0.2;
        assert_approx_eq!(xyy.x, 0.5 / sum);
        assert_approx_eq!(xyy.y, 0.3 / sum);
        assert_approx_eq!(xyy.lum, 0.3);
    }

    #[test]
    fn xyy_to_xyz() {
        let xyy = Xyy::new(0.4, 0.3, 0.6);
        let xyz = xyy.to_xyz();
        let factor = 0.6 / 0.3;
        assert_approx_eq!(xyz.x, 0.4 * factor);
        assert_approx_eq!(xyz.y, 0.6);
        assert_approx_eq!(xyz.z, (1.0 - 0.4 - 0.3) * factor);
    }

    #[test]
    fn roundtrip_xyz_xyy() {
        let xyz = Xyz::new(0.7, 0.2, 0.1);
        let xyy = xyz.to_xyy();
        let xyz2 = xyy.to_xyz();
        assert_approx_eq!(xyz2.x, xyz.x, 1e-6);
        assert_approx_eq!(xyz2.y, xyz.y, 1e-6);
        assert_approx_eq!(xyz2.z, xyz.z, 1e-6);
    }
}
