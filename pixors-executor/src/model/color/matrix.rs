//! 3×3 matrix math for color space conversions.
//!
//! Storage is **column-major**: `self.0[col][row]`. Use `from_cols` / `from_rows`
//! to construct; use `mul_vec` to apply to a column vector.

use crate::model::color::{RgbPrimaries, WhitePoint};
use crate::error::Error;
use wide::f32x4;

/// A 3×3 column-major matrix.
#[derive(Debug, Clone, Copy)]
pub struct Matrix3x3(pub [[f32; 3]; 3]);

impl Matrix3x3 {
    /// SIMD-accelerated `self * v` for 4 vectors at once.
    /// Takes three `f32x4` registers (R, G, B components of 4 pixels)
    /// and returns three `f32x4` registers (new R, G, B).
    #[inline(always)]
    pub fn mul_vec_simd_x4(&self, r: f32x4, g: f32x4, b: f32x4) -> (f32x4, f32x4, f32x4) {
        let m00 = f32x4::splat(self.0[0][0]);
        let m10 = f32x4::splat(self.0[1][0]);
        let m20 = f32x4::splat(self.0[2][0]);
        let m01 = f32x4::splat(self.0[0][1]);
        let m11 = f32x4::splat(self.0[1][1]);
        let m21 = f32x4::splat(self.0[2][1]);
        let m02 = f32x4::splat(self.0[0][2]);
        let m12 = f32x4::splat(self.0[1][2]);
        let m22 = f32x4::splat(self.0[2][2]);
        let out_r = m00.mul_add(r, m10.mul_add(g, m20 * b));
        let out_g = m01.mul_add(r, m11.mul_add(g, m21 * b));
        let out_b = m02.mul_add(r, m12.mul_add(g, m22 * b));
        (out_r, out_g, out_b)
    }

    pub const IDENTITY: Self = Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    pub const fn from_cols(c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> Self {
        Self([c0, c1, c2])
    }

    pub const fn from_rows(r0: [f32; 3], r1: [f32; 3], r2: [f32; 3]) -> Self {
        Self([
            [r0[0], r1[0], r2[0]],
            [r0[1], r1[1], r2[1]],
            [r0[2], r1[2], r2[2]],
        ])
    }

    pub fn col(&self, i: usize) -> [f32; 3] {
        self.0[i]
    }
    pub fn row(&self, i: usize) -> [f32; 3] {
        [self.0[0][i], self.0[1][i], self.0[2][i]]
    }

    /// Returns the matrix as a row-major 3x3 array.
    /// Useful for passing to SIMD code that expects row-major layout.
    pub fn as_3x3_array(&self) -> [[f32; 3]; 3] {
        [
            [self.0[0][0], self.0[1][0], self.0[2][0]],
            [self.0[0][1], self.0[1][1], self.0[2][1]],
            [self.0[0][2], self.0[1][2], self.0[2][2]],
        ]
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        let mut result = [[0.0; 3]; 3];
        for (c, row) in result.iter_mut().enumerate() {
            for (r, val) in row.iter_mut().enumerate() {
                *val = self.0[0][r] * rhs.0[c][0]
                    + self.0[1][r] * rhs.0[c][1]
                    + self.0[2][r] * rhs.0[c][2];
            }
        }
        Self(result)
    }

    pub fn mul_vec(&self, v: [f32; 3]) -> [f32; 3] {
        [
            self.0[0][0] * v[0] + self.0[1][0] * v[1] + self.0[2][0] * v[2],
            self.0[0][1] * v[0] + self.0[1][1] * v[1] + self.0[2][1] * v[2],
            self.0[0][2] * v[0] + self.0[1][2] * v[1] + self.0[2][2] * v[2],
        ]
    }

    /// Returns `Err` if the matrix is singular (det ≈ 0).
    pub fn inverse(&self) -> Result<Self, Error> {
        let a = &self.0;
        let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
            - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
        if det.abs() <= 1e-12 {
            return Err(Error::ColorConversion(format!(
                "singular matrix (det = {})",
                det
            )));
        }
        let inv_det = 1.0 / det;
        let mut inv = [[0.0; 3]; 3];
        inv[0][0] = (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * inv_det;
        inv[0][1] = (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * inv_det;
        inv[0][2] = (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv_det;
        inv[1][0] = (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * inv_det;
        inv[1][1] = (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv_det;
        inv[1][2] = (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * inv_det;
        inv[2][0] = (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * inv_det;
        inv[2][1] = (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * inv_det;
        inv[2][2] = (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv_det;
        Ok(Self(inv))
    }

    pub fn diag(d0: f32, d1: f32, d2: f32) -> Self {
        Self([[d0, 0.0, 0.0], [0.0, d1, 0.0], [0.0, 0.0, d2]])
    }
}

// --- RGB ↔ XYZ matrix derivation ---

/// Derives the RGB→XYZ matrix for given primaries and white point.
pub fn rgb_to_xyz_matrix(
    primaries: RgbPrimaries,
    white_point: WhitePoint,
) -> Result<Matrix3x3, Error> {
    let chroma = primaries.chromaticities();
    let wp_xyz = white_point.xyz();

    let mut m = [[0.0; 3]; 3];
    for (c, chr) in chroma.iter().enumerate() {
        m[c][0] = chr.x / chr.y;
        m[c][1] = 1.0;
        m[c][2] = (1.0 - chr.x - chr.y) / chr.y;
    }

    let s = Matrix3x3(m).inverse()?.mul_vec(wp_xyz);
    Ok(Matrix3x3(m).mul(&Matrix3x3::diag(s[0], s[1], s[2])))
}

// --- Bradford chromatic adaptation ---

const BRADFORD: Matrix3x3 = Matrix3x3::from_rows(
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
);

const BRADFORD_INV: Matrix3x3 = Matrix3x3::from_rows(
    [0.9869929, -0.1470543, 0.1599627],
    [0.4323053, 0.5183603, 0.0492912],
    [-0.0085287, 0.0400428, 0.9684867],
);

/// Chromatic adaptation matrix from `src_white` to `dst_white` via Bradford CAT.
pub fn bradford_cat(src_white: WhitePoint, dst_white: WhitePoint) -> Matrix3x3 {
    if src_white == dst_white {
        return Matrix3x3::IDENTITY;
    }
    let src_lms = BRADFORD.mul_vec(src_white.xyz());
    let dst_lms = BRADFORD.mul_vec(dst_white.xyz());
    let ratio = [
        if src_lms[0].abs() > 1e-12 {
            dst_lms[0] / src_lms[0]
        } else {
            1.0
        },
        if src_lms[1].abs() > 1e-12 {
            dst_lms[1] / src_lms[1]
        } else {
            1.0
        },
        if src_lms[2].abs() > 1e-12 {
            dst_lms[2] / src_lms[2]
        } else {
            1.0
        },
    ];
    BRADFORD_INV.mul(&Matrix3x3::diag(ratio[0], ratio[1], ratio[2]).mul(&BRADFORD))
}

// --- Composite RGB→RGB transform ---

/// Full matrix converting linear RGB from source to destination color space,
/// applying Bradford CAT when white points differ.
pub fn rgb_to_rgb_transform(
    src_primaries: RgbPrimaries,
    src_white: WhitePoint,
    dst_primaries: RgbPrimaries,
    dst_white: WhitePoint,
) -> Result<Matrix3x3, Error> {
    if src_primaries == dst_primaries && src_white == dst_white {
        return Ok(Matrix3x3::IDENTITY);
    }
    let src_to_xyz = rgb_to_xyz_matrix(src_primaries, src_white)?;
    let xyz_to_dst = rgb_to_xyz_matrix(dst_primaries, dst_white)?.inverse()?;
    if src_white == dst_white {
        Ok(xyz_to_dst.mul(&src_to_xyz))
    } else {
        let cat = bradford_cat(src_white, dst_white);
        Ok(xyz_to_dst.mul(&cat.mul(&src_to_xyz)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn matrix_inverse() {
        let m = Matrix3x3::from_cols([2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]);
        let inv = m.inverse().unwrap();
        let expected =
            Matrix3x3::from_cols([0.5, 0.0, 0.0], [0.0, 1.0 / 3.0, 0.0], [0.0, 0.0, 0.25]);
        for i in 0..3 {
            for j in 0..3 {
                assert_approx_eq!(inv.0[i][j], expected.0[i][j], 1e-6);
            }
        }
    }

    #[test]
    fn matrix_mul_vec() {
        let m = Matrix3x3::from_cols([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]);
        let v = [2.0, 1.0, 0.5];
        let result = m.mul_vec(v);
        assert_approx_eq!(result[0], 9.5, 1e-6);
        assert_approx_eq!(result[1], 13.0, 1e-6);
        assert_approx_eq!(result[2], 16.5, 1e-6);
    }

    #[test]
    fn rgb_to_xyz_matrix_sanity() {
        let m = rgb_to_xyz_matrix(RgbPrimaries::Bt709, WhitePoint::D65).unwrap();
        let wp = WhitePoint::D65.xyz();
        let computed = m.mul_vec([1.0, 1.0, 1.0]);
        assert_approx_eq!(computed[0], wp[0], 1e-4);
        assert_approx_eq!(computed[1], wp[1], 1e-4);
        assert_approx_eq!(computed[2], wp[2], 1e-4);
    }

    #[test]
    fn bradford_cat_d65_to_d50() {
        let cat = bradford_cat(WhitePoint::D65, WhitePoint::D50);
        let src = WhitePoint::D65.xyz();
        let dst = WhitePoint::D50.xyz();
        let transformed = cat.mul_vec(src);
        for i in 0..3 {
            assert_approx_eq!(transformed[i], dst[i], 0.1);
        }
    }

    #[test]
    fn matrix_mul_order() {
        let a = Matrix3x3::from_cols([1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        let b = Matrix3x3::from_cols([2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]);
        assert_approx_eq!(a.mul(&b).mul_vec([1.0, 1.0, 1.0])[1], 5.0, 1e-6);
        assert_approx_eq!(b.mul(&a).mul_vec([1.0, 1.0, 1.0])[1], 6.0, 1e-6);
    }
}
