use super::{AlphaPolicy, Pixel};
use wide::f32x4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lab<T> {
    pub l: T,
    pub a: T,
    pub b: T,
}

unsafe impl bytemuck::Pod for Lab<u8> {}
unsafe impl bytemuck::Zeroable for Lab<u8> {}
unsafe impl bytemuck::Pod for Lab<u16> {}
unsafe impl bytemuck::Zeroable for Lab<u16> {}

impl<T> Lab<T> {
    pub const fn new(l: T, a: T, b: T) -> Self {
        Self { l, a, b }
    }
}

// Lab<u8>: L in [0..255] → L_norm [0..1], a/b in [0..255] → [-1..1] (128 = 0)
impl Pixel for Lab<u8> {
    fn unpack(self) -> [f32; 4] {
        [
            self.l as f32 / 255.0,
            (self.a as f32 - 128.0) / 128.0,
            (self.b as f32 - 128.0) / 128.0,
            1.0,
        ]
    }

    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            l: (rgba[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            a: ((rgba[1].clamp(-1.0, 1.0) * 128.0 + 128.0) + 0.5) as u8,
            b: ((rgba[2].clamp(-1.0, 1.0) * 128.0 + 128.0) + 0.5) as u8,
        }
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        for i in 0..4 {
            out[i] = Self {
                l: (r[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                a: ((g[i].clamp(-1.0, 1.0) * 128.0 + 128.0) + 0.5) as u8,
                b: ((b[i].clamp(-1.0, 1.0) * 128.0 + 128.0) + 0.5) as u8,
            };
        }
    }
}

// Lab<u16>: L in [0..65535] → [0..1], a/b centered at 32768
impl Pixel for Lab<u16> {
    fn unpack(self) -> [f32; 4] {
        [
            self.l as f32 / 65535.0,
            (self.a as f32 - 32768.0) / 32768.0,
            (self.b as f32 - 32768.0) / 32768.0,
            1.0,
        ]
    }

    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            l: (rgba[0].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16,
            a: ((rgba[1].clamp(-1.0, 1.0) * 32768.0 + 32768.0) + 0.5) as u16,
            b: ((rgba[2].clamp(-1.0, 1.0) * 32768.0 + 32768.0) + 0.5) as u16,
        }
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        for i in 0..4 {
            out[i] = Self {
                l: (r[i].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16,
                a: ((g[i].clamp(-1.0, 1.0) * 32768.0 + 32768.0) + 0.5) as u16,
                b: ((b[i].clamp(-1.0, 1.0) * 32768.0 + 32768.0) + 0.5) as u16,
            };
        }
    }
}
