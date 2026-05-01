use super::{AlphaPolicy, Component, Pixel};
use half::f16;
use wide::f32x4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb<T: Component> {
    pub r: T,
    pub g: T,
    pub b: T,
}

impl<T: Component> Rgb<T> {
    pub const fn new(r: T, g: T, b: T) -> Self {
        Self { r, g, b }
    }
}

unsafe impl<T: Component> bytemuck::Pod for Rgb<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for Rgb<T> {}

impl Pixel for Rgb<f16> {
    fn unpack(self) -> [f32; 4] {
        [self.r.to_f32(), self.g.to_f32(), self.b.to_f32(), 1.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        for i in 0..4 {
            out[i] = Rgb { r: f16::from_f32(r[i]), g: f16::from_f32(g[i]), b: f16::from_f32(b[i]) };
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Rgb { r: f16::from_f32(rgba[0]), g: f16::from_f32(rgba[1]), b: f16::from_f32(rgba[2]) }
    }
}

impl Pixel for Rgb<f32> {
    fn unpack(self) -> [f32; 4] {
        [self.r, self.g, self.b, 1.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        for i in 0..4 {
            out[i] = Rgb { r: r[i], g: g[i], b: b[i] };
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Rgb { r: rgba[0], g: rgba[1], b: rgba[2] }
    }
}

