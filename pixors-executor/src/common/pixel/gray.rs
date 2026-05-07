use super::Component;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gray<T: Component> {
    pub v: T,
}

impl<T: Component> Gray<T> {
    pub const fn new(v: T) -> Self {
        Self { v }
    }
}

unsafe impl<T: Component> bytemuck::Pod for Gray<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for Gray<T> {}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrayAlpha<T: Component> {
    pub v: T,
    pub a: T,
}

impl<T: Component> GrayAlpha<T> {
    pub const fn new(v: T, a: T) -> Self {
        Self { v, a }
    }
}

unsafe impl<T: Component> bytemuck::Pod for GrayAlpha<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for GrayAlpha<T> {}

use super::{AlphaPolicy, Pixel};
use half::f16;
use wide::f32x4;

impl Pixel for Gray<u8> {
    fn unpack(self) -> [f32; 4] { let f = self.v as f32 / 255.0; [f, f, f, 1.0] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        let v = (rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722).clamp(0.0, 1.0);
        Self { v: (v * 255.0 + 0.5) as u8 }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array();
        for i in 0..4 {
            let v = (r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722).clamp(0.0, 1.0);
            out[i] = Self { v: (v * 255.0 + 0.5) as u8 };
        }
    }
}

impl Pixel for Gray<f32> {
    fn unpack(self) -> [f32; 4] { [self.v, self.v, self.v, 1.0] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self { v: rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722 }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array();
        for i in 0..4 { out[i] = Self { v: r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722 }; }
    }
}

impl Pixel for Gray<f16> {
    fn unpack(self) -> [f32; 4] { let f = self.v.to_f32(); [f, f, f, 1.0] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        let v = rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722;
        Self { v: f16::from_f32(v.clamp(0.0, 65504.0)) }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array();
        for i in 0..4 {
            let v = r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722;
            out[i] = Self { v: f16::from_f32(v.clamp(0.0, 65504.0)) };
        }
    }
}

impl Pixel for Gray<u16> {
    fn unpack(self) -> [f32; 4] { let f = self.v as f32 / 65535.0; [f, f, f, 1.0] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        let v = (rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722).clamp(0.0, 1.0);
        Self { v: (v * 65535.0 + 0.5) as u16 }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array();
        for i in 0..4 {
            let v = (r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722).clamp(0.0, 1.0);
            out[i] = Self { v: (v * 65535.0 + 0.5) as u16 };
        }
    }
}

impl Pixel for GrayAlpha<u8> {
    fn unpack(self) -> [f32; 4] { [self.v as f32 / 255.0, self.v as f32 / 255.0, self.v as f32 / 255.0, self.a as f32 / 255.0] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self { v: ((rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722) * 255.0 + 0.5) as u8, a: (rgba[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8 }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array(); let a = aa.to_array();
        for i in 0..4 {
            let v = r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722;
            out[i] = Self { v: (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8, a: (a[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };
        }
    }
}

impl Pixel for GrayAlpha<u16> {
    fn unpack(self) -> [f32; 4] {
        let f = self.v as f32 / 65535.0;
        [f, f, f, self.a as f32 / 65535.0]
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            v: ((rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16,
            a: (rgba[3].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16,
        }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array(); let a = aa.to_array();
        for i in 0..4 {
            let v = (r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722).clamp(0.0, 1.0);
            out[i] = Self { v: (v * 65535.0 + 0.5) as u16, a: (a[i].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16 };
        }
    }
}

impl Pixel for GrayAlpha<f16> {
    fn unpack(self) -> [f32; 4] { let f = self.v.to_f32(); [f, f, f, self.a.to_f32()] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        let v = rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722;
        Self { v: f16::from_f32(v.clamp(0.0, 65504.0)), a: f16::from_f32(rgba[3]) }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array(); let a = aa.to_array();
        for i in 0..4 {
            let v = r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722;
            out[i] = Self { v: f16::from_f32(v.clamp(0.0, 65504.0)), a: f16::from_f32(a[i]) };
        }
    }
}

impl Pixel for GrayAlpha<f32> {
    fn unpack(self) -> [f32; 4] { [self.v, self.v, self.v, self.a] }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self { v: rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722, a: rgba[3] }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array(); let g = gg.to_array(); let b = bb.to_array(); let a = aa.to_array();
        for i in 0..4 {
            out[i] = Self { v: r[i] * 0.2126 + g[i] * 0.7152 + b[i] * 0.0722, a: a[i] };
        }
    }
}
