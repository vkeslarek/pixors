use super::{AlphaPolicy, Pixel};
use wide::f32x4;

impl Pixel for [u8; 4] {
    fn unpack(self) -> [f32; 4] {
        [self[0] as f32 / 255.0, self[1] as f32 / 255.0, self[2] as f32 / 255.0, self[3] as f32 / 255.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        let a: [f32; 4] = aa.into();
        for i in 0..4 {
            out[i] = [
                (r[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                (g[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                (b[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                (a[i].clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        [
            (rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

impl Pixel for [u16; 4] {
    fn unpack(self) -> [f32; 4] {
        [self[0] as f32 / 65535.0, self[1] as f32 / 65535.0, self[2] as f32 / 65535.0, self[3] as f32 / 65535.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        let a: [f32; 4] = aa.into();
        for i in 0..4 {
            out[i] = [
                (r[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
                (g[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
                (b[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
                (a[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
            ];
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        [
            (rgba[0].clamp(0.0, 1.0) * 65535.0).round() as u16,
            (rgba[1].clamp(0.0, 1.0) * 65535.0).round() as u16,
            (rgba[2].clamp(0.0, 1.0) * 65535.0).round() as u16,
            (rgba[3].clamp(0.0, 1.0) * 65535.0).round() as u16,
        ]
    }
}

impl Pixel for [u8; 3] {
    fn unpack(self) -> [f32; 4] {
        [self[0] as f32 / 255.0, self[1] as f32 / 255.0, self[2] as f32 / 255.0, 1.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        for i in 0..4 {
            out[i] = [
                (r[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                (g[i].clamp(0.0, 1.0) * 255.0).round() as u8,
                (b[i].clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        [
            (rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

impl Pixel for [u16; 3] {
    fn unpack(self) -> [f32; 4] {
        [self[0] as f32 / 65535.0, self[1] as f32 / 65535.0, self[2] as f32 / 65535.0, 1.0]
    }
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, _aa: f32x4, _mode: AlphaPolicy, out: &mut [Self]) {
        let r: [f32; 4] = rr.into();
        let g: [f32; 4] = gg.into();
        let b: [f32; 4] = bb.into();
        for i in 0..4 {
            out[i] = [
                (r[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
                (g[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
                (b[i].clamp(0.0, 1.0) * 65535.0).round() as u16,
            ];
        }
    }
    fn pack_one(rgba: [f32; 4], _mode: AlphaPolicy) -> Self {
        [
            (rgba[0].clamp(0.0, 1.0) * 65535.0).round() as u16,
            (rgba[1].clamp(0.0, 1.0) * 65535.0).round() as u16,
            (rgba[2].clamp(0.0, 1.0) * 65535.0).round() as u16,
        ]
    }
}
