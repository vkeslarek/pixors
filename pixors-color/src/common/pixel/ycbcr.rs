use pixors_engine::common::pixel::{AlphaPolicy, Pixel};
use half::f16;
use wide::f32x4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YCbCr<T> {
    pub y: T,
    pub cb: T,
    pub cr: T,
}

unsafe impl bytemuck::Pod for YCbCr<u8> {}
unsafe impl bytemuck::Zeroable for YCbCr<u8> {}
unsafe impl bytemuck::Pod for YCbCr<f16> {}
unsafe impl bytemuck::Zeroable for YCbCr<f16> {}
unsafe impl bytemuck::Pod for YCbCr<f32> {}
unsafe impl bytemuck::Zeroable for YCbCr<f32> {}

impl<T> YCbCr<T> {
    pub const fn new(y: T, cb: T, cr: T) -> Self {
        Self { y, cb, cr }
    }
}

impl Pixel for YCbCr<u8> {
    fn unpack(self) -> [f32; 4] {
        [
            self.y as f32 / 255.0,
            self.cb as f32 / 255.0,
            self.cr as f32 / 255.0,
            1.0,
        ]
    }

    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            y: (rgba[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            cb: (rgba[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            cr: (rgba[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        }
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        for i in 0..4 {
            out[i] = Self {
                y: (r[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                cb: (g[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                cr: (b[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            };
        }
    }
}

impl Pixel for YCbCr<f16> {
    fn unpack(self) -> [f32; 4] {
        [self.y.to_f32(), self.cb.to_f32(), self.cr.to_f32(), 1.0]
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            y: f16::from_f32(rgba[0].clamp(0.0, 1.0)),
            cb: f16::from_f32(rgba[1].clamp(0.0, 1.0)),
            cr: f16::from_f32(rgba[2].clamp(0.0, 1.0)),
        }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        for i in 0..4 {
            out[i] = Self {
                y: f16::from_f32(r[i].clamp(0.0, 1.0)),
                cb: f16::from_f32(g[i].clamp(0.0, 1.0)),
                cr: f16::from_f32(b[i].clamp(0.0, 1.0)),
            };
        }
    }
}

impl Pixel for YCbCr<f32> {
    fn unpack(self) -> [f32; 4] {
        [self.y, self.cb, self.cr, 1.0]
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        Self {
            y: rgba[0].clamp(0.0, 1.0),
            cb: rgba[1].clamp(0.0, 1.0),
            cr: rgba[2].clamp(0.0, 1.0),
        }
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r = rr.to_array();
        let g = gg.to_array();
        let b = bb.to_array();
        for i in 0..4 {
            out[i] = Self {
                y: r[i].clamp(0.0, 1.0),
                cb: g[i].clamp(0.0, 1.0),
                cr: b[i].clamp(0.0, 1.0),
            };
        }
    }
}
