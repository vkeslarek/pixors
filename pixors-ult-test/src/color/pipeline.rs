//! Generic SIMD conversion pipeline parameterized by source reader and destination pixel type.
//!
//! Two concepts:
//! - `SrcReader` — gathers 4 lanes of (r,g,b,a) from a raw byte slice + BufferDesc.
//! - `Pixel` — unified pack/unpack between a concrete type and `[f32;4]`.
//! - `run<R, D>` — the generic inner loop: gather → decode → matrix → encode → pack.

use crate::color::ColorConversion;
use crate::color::TransferFn;
use crate::image::buffer::BufferDesc;
use crate::pixel::{AlphaPolicy, Pixel};
use wide::f32x4;

// ---------------------------------------------------------------------------
// SrcReader — reads 4 lanes of (r,g,b,a) from data + desc
// ---------------------------------------------------------------------------

pub trait SrcReader {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4);
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4];
}

// ---------------------------------------------------------------------------
// Generic inner loop — parameterized by SrcReader + Dst Pixel
// ---------------------------------------------------------------------------

pub fn run<R: SrcReader, D: Pixel>(
    conv: &ColorConversion,
    data: &[u8],
    desc: &BufferDesc,
    y: u32,
    x_start: u32,
    x_end: u32,
    dst: &mut [D],
    mode: AlphaPolicy,
) {
    let mat = conv.matrix();
    let tf = conv.src().transfer();
    let encode = conv.encode_lut();

    let width = x_end - x_start;
    let full = (width / 4) as usize;
    let rem = (width % 4) as usize;
    let mut x = x_start as usize;

    for _ in 0..full {
        let (r_lin, g_lin, b_lin, a_vals) = R::read_x4(data, desc, x as u32, y);

        let (rr, gg, bb) = mat.mul_vec_simd_x4(
            decode_simd(r_lin, tf),
            decode_simd(g_lin, tf),
            decode_simd(b_lin, tf),
        );

        let (rr, gg, bb) = apply_mode(rr, gg, bb, a_vals, mode);

        let r_enc = encode_simd(rr, encode);
        let g_enc = encode_simd(gg, encode);
        let b_enc = encode_simd(bb, encode);

        D::pack_x4(r_enc, g_enc, b_enc, a_vals, mode, &mut dst[x - x_start as usize..]);
        x += 4;
    }

    for i in 0..rem {
        let [rl, gl, bl, a] = R::read_one(data, desc, (x + i) as u32, y);
        let decoded = [tf.decode(rl), tf.decode(gl), tf.decode(bl)];
        let linear = mat.mul_vec(decoded);
        let [r, g, b] = apply_mode_one(linear, a, mode);
        let r_enc = conv.encode_fast(r);
        let g_enc = conv.encode_fast(g);
        let b_enc = conv.encode_fast(b);
        dst[x - x_start as usize + i] = D::pack_one([r_enc, g_enc, b_enc, a], mode);
    }
}

// ---------------------------------------------------------------------------
// SIMD helpers (also exported for space)
// ---------------------------------------------------------------------------

pub fn decode_simd(v: f32x4, tf: TransferFn) -> f32x4 {
    let mut out = [0.0; 4];
    for (i, val) in v.to_array().iter().enumerate() {
        out[i] = tf.decode(*val);
    }
    f32x4::from(out)
}

pub fn encode_simd(v: f32x4, lut: &[f32]) -> f32x4 {
    let mut out = [0.0; 4];
    for (i, val) in v.to_array().iter().enumerate() {
        out[i] = crate::color::lookup_encode(*val, lut);
    }
    f32x4::from(out)
}

pub fn apply_mode(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, mode: AlphaPolicy) -> (f32x4, f32x4, f32x4) {
    match mode {
        AlphaPolicy::PremultiplyOnPack | AlphaPolicy::OpaqueDrop => (rr * aa, gg * aa, bb * aa),
        AlphaPolicy::Straight => (rr, gg, bb),
    }
}

pub fn apply_mode_one(linear: [f32; 3], a: f32, mode: AlphaPolicy) -> [f32; 3] {
    match mode {
        AlphaPolicy::PremultiplyOnPack | AlphaPolicy::OpaqueDrop => [linear[0] * a, linear[1] * a, linear[2] * a],
        AlphaPolicy::Straight => linear,
    }
}

// ---------------------------------------------------------------------------
// SrcReader impls — layout marker ZSTs
// ---------------------------------------------------------------------------

pub struct RgbaU8Interleaved;
impl SrcReader for RgbaU8Interleaved {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let row: &[[u8; 4]] = {
            let plane = &desc.planes[0];
            let bytes = plane.interleaved_row::<u8, 4>(data, y).unwrap_or(&[]);
            bytemuck::cast_slice(bytes)
        };
        let base = x as usize;
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let mut a = [0.0_f32; 4];
        for i in 0..4 {
            let px = row[base + i];
            r[i] = px[0] as f32 / 255.0;
            g[i] = px[1] as f32 / 255.0;
            b[i] = px[2] as f32 / 255.0;
            a[i] = px[3] as f32 / 255.0;
        }
        (f32x4::from(r), f32x4::from(g), f32x4::from(b), f32x4::from(a))
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        [desc.planes[0].read_sample(data, x, y), desc.planes[1].read_sample(data, x, y),
         desc.planes[2].read_sample(data, x, y), desc.planes[3].read_sample(data, x, y)]
    }
}

pub struct RgbU8Interleaved;
impl SrcReader for RgbU8Interleaved {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let row: &[[u8; 3]] = {
            let plane = &desc.planes[0];
            let bytes = plane.interleaved_row::<u8, 3>(data, y).unwrap_or(&[]);
            bytemuck::cast_slice(bytes)
        };
        let base = x as usize;
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let a = [1.0_f32; 4];
        for i in 0..4 {
            let px = row[base + i];
            r[i] = px[0] as f32 / 255.0;
            g[i] = px[1] as f32 / 255.0;
            b[i] = px[2] as f32 / 255.0;
        }
        (f32x4::from(r), f32x4::from(g), f32x4::from(b), f32x4::from(a))
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        [desc.planes[0].read_sample(data, x, y), desc.planes[1].read_sample(data, x, y),
         desc.planes[2].read_sample(data, x, y), 1.0]
    }
}

pub struct GrayU8Interleaved;
impl SrcReader for GrayU8Interleaved {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let row: &[u8] = {
            let plane = &desc.planes[0];
            plane.interleaved_row::<u8, 1>(data, y).unwrap_or(&[])
        };
        let base = x as usize;
        let mut v = [0.0_f32; 4];
        for i in 0..4 {
            v[i] = row[base + i] as f32 / 255.0;
        }
        let one = f32x4::splat(1.0);
        let vv = f32x4::from(v);
        (vv, vv, vv, one)
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        let v = desc.planes[0].read_sample(data, x, y);
        [v, v, v, 1.0]
    }
}

pub struct GrayAlphaU8Interleaved;
impl SrcReader for GrayAlphaU8Interleaved {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let row: &[[u8; 2]] = {
            let plane = &desc.planes[0];
            let bytes = plane.interleaved_row::<u8, 2>(data, y).unwrap_or(&[]);
            bytemuck::cast_slice(bytes)
        };
        let base = x as usize;
        let mut v = [0.0_f32; 4];
        let mut a = [0.0_f32; 4];
        for i in 0..4 {
            let px = row[base + i];
            let vf = px[0] as f32 / 255.0;
            v[i] = vf;
            a[i] = px[1] as f32 / 255.0;
        }
        let vv = f32x4::from(v);
        (vv, vv, vv, f32x4::from(a))
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        let v = desc.planes[0].read_sample(data, x, y);
        [v, v, v, desc.planes[1].read_sample(data, x, y)]
    }
}

pub struct RgbaU16Interleaved;
impl SrcReader for RgbaU16Interleaved {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let row: &[[u16; 4]] = {
            let plane = &desc.planes[0];
            let bytes = plane.interleaved_row::<u16, 4>(data, y).unwrap_or(&[]);
            bytemuck::cast_slice(bytes)
        };
        let base = x as usize;
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let mut a = [0.0_f32; 4];
        for i in 0..4 {
            let px = row[base + i];
            r[i] = px[0] as f32 / 65535.0;
            g[i] = px[1] as f32 / 65535.0;
            b[i] = px[2] as f32 / 65535.0;
            a[i] = px[3] as f32 / 65535.0;
        }
        (f32x4::from(r), f32x4::from(g), f32x4::from(b), f32x4::from(a))
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        [desc.planes[0].read_sample(data, x, y), desc.planes[1].read_sample(data, x, y),
         desc.planes[2].read_sample(data, x, y), desc.planes[3].read_sample(data, x, y)]
    }
}

pub struct GenericReader;
impl SrcReader for GenericReader {
    fn read_x4(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4) {
        let has_alpha = desc.planes.len() >= 4;
        let is_gray = desc.planes.len() <= 2;
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let mut av = [0.0_f32; 4];
        for i in 0..4 {
            let px = x + i as u32;
            let (rv, gv, bv) = if is_gray {
                let v = desc.planes[0].read_sample(data, px, y);
                (v, v, v)
            } else {
                (desc.planes[0].read_sample(data, px, y), desc.planes[1].read_sample(data, px, y), desc.planes[2].read_sample(data, px, y))
            };
            let a = if has_alpha {
                desc.planes[if is_gray { 1 } else { 3 }].read_sample(data, px, y)
            } else {
                1.0
            };
            r[i] = rv;
            g[i] = gv;
            b[i] = bv;
            av[i] = a;
        }
        (f32x4::from(r), f32x4::from(g), f32x4::from(b), f32x4::from(av))
    }
    fn read_one(data: &[u8], desc: &BufferDesc, x: u32, y: u32) -> [f32; 4] {
        let has_alpha = desc.planes.len() >= 4;
        let is_gray = desc.planes.len() <= 2;
        let (r, g, b) = if is_gray {
            let v = desc.planes[0].read_sample(data, x, y);
            (v, v, v)
        } else {
            (desc.planes[0].read_sample(data, x, y), desc.planes[1].read_sample(data, x, y), desc.planes[2].read_sample(data, x, y))
        };
        let a = if has_alpha {
            desc.planes[if is_gray { 1 } else { 3 }].read_sample(data, x, y)
        } else {
            1.0
        };
        [r, g, b, a]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // Tests moved to pixel/mod.rs alongside the Pixel trait
}
