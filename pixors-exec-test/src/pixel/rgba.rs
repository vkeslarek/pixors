use super::{Component, Pixel};
use crate::pixel::AlphaPolicy;
use half::f16;
use wide::f32x4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba<T: Component> {
    pub r: T,
    pub g: T,
    pub b: T,
    pub a: T,
}

impl<T: Component> Rgba<T> {
    pub const fn new(r: T, g: T, b: T, a: T) -> Self {
        Self { r, g, b, a }
    }
    pub const ZERO: Self = Self {
        r: T::ZERO,
        g: T::ZERO,
        b: T::ZERO,
        a: T::ZERO,
    };
    pub const ONE: Self = Self {
        r: T::ONE,
        g: T::ONE,
        b: T::ONE,
        a: T::ONE,
    };
    pub fn black() -> Self {
        Self::new(T::ZERO, T::ZERO, T::ZERO, T::ONE)
    }
    pub fn white() -> Self {
        Self::new(T::ONE, T::ONE, T::ONE, T::ONE)
    }
}

// bytemuck safety
unsafe impl<T: Component> bytemuck::Pod for Rgba<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for Rgba<T> {}

// Pixel impls
impl Pixel for Rgba<f16> {
    fn unpack(self) -> [f32; 4] {
        let a = self.a.to_f32();
        if a > 0.0 {
            let inv = 1.0 / a;
            [
                self.r.to_f32() * inv,
                self.g.to_f32() * inv,
                self.b.to_f32() * inv,
                a,
            ]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        let a: [f32; 4] = aa.into();
        for i in 0..4 {
            out[i] = Rgba {
                r: f16::from_f32(r[i]),
                g: f16::from_f32(g[i]),
                b: f16::from_f32(b[i]),
                a: f16::from_f32(a[i]),
            };
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Rgba {
            r: f16::from_f32(rgba[0]),
            g: f16::from_f32(rgba[1]),
            b: f16::from_f32(rgba[2]),
            a: f16::from_f32(rgba[3]),
        }
    }
}

impl Pixel for Rgba<f32> {
    fn unpack(self) -> [f32; 4] {
        let a = self.a;
        if a > 1e-6 {
            let inv = 1.0 / a;
            [self.r * inv, self.g * inv, self.b * inv, a]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        let a: [f32; 4] = aa.into();
        for i in 0..4 {
            out[i] = Rgba {
                r: r[i],
                g: g[i],
                b: b[i],
                a: a[i],
            };
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Rgba {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        }
    }
}

impl Pixel for Rgba<u8> {
    fn unpack(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        let a: [f32; 4] = aa.into();
        for i in 0..4 {
            out[i] = Rgba {
                r: (r[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                g: (g[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                b: (b[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                a: (a[i].clamp(0.0, 1.0) * 255.0).round() as u8,
            };
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Rgba {
            r: (rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            a: (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}
