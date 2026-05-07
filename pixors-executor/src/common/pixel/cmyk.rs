use super::{AlphaPolicy, Component, Pixel};
use wide::f32x4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmyk<T: Component> {
    pub c: T,
    pub m: T,
    pub y: T,
    pub k: T,
}

unsafe impl<T: Component> bytemuck::Pod for Cmyk<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for Cmyk<T> {}

impl<T: Component> Cmyk<T> {
    pub const fn new(c: T, m: T, y: T, k: T) -> Self {
        Self { c, m, y, k }
    }
}

impl Pixel for Cmyk<u8> {
    fn unpack(self) -> [f32; 4] {
        [self.c as f32 / 255.0, self.m as f32 / 255.0, self.y as f32 / 255.0, self.k as f32 / 255.0]
    }

    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            c: (rgba[0].clamp(0.0, 255.0) + 0.5) as u8,
            m: (rgba[1].clamp(0.0, 255.0) + 0.5) as u8,
            y: (rgba[2].clamp(0.0, 255.0) + 0.5) as u8,
            k: (rgba[3].clamp(0.0, 255.0) + 0.5) as u8,
        }
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        let a = aa.to_array();
        for i in 0..4 {
            out[i] = Self {
                c: (r[i].clamp(0.0, 255.0) + 0.5) as u8,
                m: (g[i].clamp(0.0, 255.0) + 0.5) as u8,
                y: (b[i].clamp(0.0, 255.0) + 0.5) as u8,
                k: (a[i].clamp(0.0, 255.0) + 0.5) as u8,
            };
        }
    }
}

impl Pixel for Cmyk<u16> {
    fn unpack(self) -> [f32; 4] {
        [
            self.c as f32,
            self.m as f32,
            self.y as f32,
            self.k as f32,
        ]
    }

    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            c: (rgba[0].clamp(0.0, 65535.0) + 0.5) as u16,
            m: (rgba[1].clamp(0.0, 65535.0) + 0.5) as u16,
            y: (rgba[2].clamp(0.0, 65535.0) + 0.5) as u16,
            k: (rgba[3].clamp(0.0, 65535.0) + 0.5) as u16,
        }
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        let a = aa.to_array();
        for i in 0..4 {
            out[i] = Self {
                c: (r[i].clamp(0.0, 65535.0) + 0.5) as u16,
                m: (g[i].clamp(0.0, 65535.0) + 0.5) as u16,
                y: (b[i].clamp(0.0, 65535.0) + 0.5) as u16,
                k: (a[i].clamp(0.0, 65535.0) + 0.5) as u16,
            };
        }
    }
}
