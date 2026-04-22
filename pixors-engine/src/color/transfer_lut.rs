//! Lookup tables for gamma transfer functions.
//!
//! LUTs are computed once per process (via `OnceLock`) and reused.
//! - Decode LUTs: 256 entries covering u8 input `[0, 255]`
//! - Encode LUTs: 4096 entries covering linear `[0, 1]` with lerp

use std::sync::OnceLock;

// --- Decode (encoded u8 → linear f32) ---

/// sRGB decode table: index = encoded u8, value = linear f32.
pub fn srgb_decode_u8() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 256];
        for i in 0..256 {
            let x = i as f32 / 255.0;
            arr[i] = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
        }
        arr
    })
}

/// Rec.709 decode table: index = encoded u8, value = linear f32.
pub fn rec709_decode_u8() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 256];
        for i in 0..256 {
            let x = i as f32 / 255.0;
            arr[i] = if x < 0.081 {
                x / 4.5
            } else {
                ((x + 0.099) / 1.099).powf(1.0 / 0.45)
            };
        }
        arr
    })
}

/// Gamma 2.2 decode table: index = encoded u8, value = linear f32.
pub fn gamma22_decode_u8() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 256];
        for i in 0..256 {
            arr[i] = (i as f32 / 255.0).powf(2.2);
        }
        arr
    })
}

/// Gamma 2.4 decode table: index = encoded u8, value = linear f32.
pub fn gamma24_decode_u8() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 256];
        for i in 0..256 {
            arr[i] = (i as f32 / 255.0).powf(2.4);
        }
        arr
    })
}

/// ProPhoto decode table: index = encoded u8, value = linear f32.
pub fn prophoto_decode_u8() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 256];
        for i in 0..256 {
            let x = i as f32 / 255.0;
            arr[i] = if x <= 1.0 / 32.0 { x / 16.0 } else { x.powf(1.8) };
        }
        arr
    })
}

// --- Encode (linear f32 → encoded f32, 4096 entries, lerp) ---

/// sRGB encode LUT: 4096 entries over linear `[0, 1]`.
pub fn srgb_encode_lut() -> &'static [f32; 4096] {
    static LUT: OnceLock<[f32; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 4096];
        for i in 0..4096 {
            let y = i as f32 / 4095.0;
            arr[i] = if y <= 0.0031308 {
                12.92 * y
            } else {
                1.055 * y.powf(1.0 / 2.4) - 0.055
            };
        }
        arr
    })
}

/// Rec.709 encode LUT: 4096 entries over linear `[0, 1]`.
pub fn rec709_encode_lut() -> &'static [f32; 4096] {
    static LUT: OnceLock<[f32; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 4096];
        for i in 0..4096 {
            let y = i as f32 / 4095.0;
            arr[i] = if y < 0.018 {
                4.5 * y
            } else {
                1.099 * y.powf(0.45) - 0.099
            };
        }
        arr
    })
}

/// Gamma 2.2 encode LUT: 4096 entries over linear `[0, 1]`.
pub fn gamma22_encode_lut() -> &'static [f32; 4096] {
    static LUT: OnceLock<[f32; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 4096];
        for i in 0..4096 {
            arr[i] = (i as f32 / 4095.0).max(0.0).powf(1.0 / 2.2);
        }
        arr
    })
}

/// Gamma 2.4 encode LUT: 4096 entries over linear `[0, 1]`.
pub fn gamma24_encode_lut() -> &'static [f32; 4096] {
    static LUT: OnceLock<[f32; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 4096];
        for i in 0..4096 {
            arr[i] = (i as f32 / 4095.0).max(0.0).powf(1.0 / 2.4);
        }
        arr
    })
}

/// ProPhoto encode LUT: 4096 entries over linear `[0, 1]`.
pub fn prophoto_encode_lut() -> &'static [f32; 4096] {
    static LUT: OnceLock<[f32; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 4096];
        for i in 0..4096 {
            let y = i as f32 / 4095.0;
            arr[i] = if y <= 0.001953125 { 16.0 * y } else { y.powf(1.0 / 1.8) };
        }
        arr
    })
}

// --- Helpers ---

/// Look up an encoded value from a 4096-entry LUT using linear interpolation.
/// `y` must be in `[0, 1]`.
#[inline(always)]
pub fn encode_lookup(y: f32, lut: &[f32; 4096]) -> f32 {
    let idx = y * 4095.0;
    let i = idx as usize;
    let frac = idx - i as f32;
    if i >= 4095 {
        lut[4095]
    } else {
        lut[i] + frac * (lut[i + 1] - lut[i])
    }
}

/// Fast sRGB decode for u8 input via LUT.
#[inline(always)]
pub fn srgb_decode_u8_fast(x: u8) -> f32 {
    srgb_decode_u8()[x as usize]
}

/// Fast Rec.709 decode for u8 input via LUT.
#[inline(always)]
pub fn rec709_decode_u8_fast(x: u8) -> f32 {
    rec709_decode_u8()[x as usize]
}

/// Fast Gamma 2.2 decode for u8 input via LUT.
#[inline(always)]
pub fn gamma22_decode_u8_fast(x: u8) -> f32 {
    gamma22_decode_u8()[x as usize]
}

/// Fast Gamma 2.4 decode for u8 input via LUT.
#[inline(always)]
pub fn gamma24_decode_u8_fast(x: u8) -> f32 {
    gamma24_decode_u8()[x as usize]
}

/// Fast ProPhoto decode for u8 input via LUT.
#[inline(always)]
pub fn prophoto_decode_u8_fast(x: u8) -> f32 {
    prophoto_decode_u8()[x as usize]
}

// -----------------------------------------------------------------------------
// 16‑bit LUTs (65 536 entries)
// -----------------------------------------------------------------------------

/// sRGB decode table for 16‑bit input (65536 entries).
pub fn srgb_decode_u16() -> &'static [f32; 65536] {
    static LUT: OnceLock<[f32; 65536]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 65536];
        // Compute in chunks to avoid stack overflow
        for chunk_start in (0..65536).step_by(4096) {
            let chunk_end = (chunk_start + 4096).min(65536);
            for i in chunk_start..chunk_end {
                let x = i as f32 / 65535.0;
                arr[i] = if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                };
            }
        }
        arr
    })
}

/// Fast sRGB decode for u16 input via LUT.
#[inline(always)]
pub fn srgb_decode_u16_fast(x: u16) -> f32 {
    srgb_decode_u16()[x as usize]
}

/// Rec.709 decode table for 16‑bit input (65536 entries).
pub fn rec709_decode_u16() -> &'static [f32; 65536] {
    static LUT: OnceLock<[f32; 65536]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 65536];
        for chunk_start in (0..65536).step_by(4096) {
            let chunk_end = (chunk_start + 4096).min(65536);
            for i in chunk_start..chunk_end {
                let x = i as f32 / 65535.0;
                arr[i] = if x < 0.081 {
                    x / 4.5
                } else {
                    ((x + 0.099) / 1.099).powf(1.0 / 0.45)
                };
            }
        }
        arr
    })
}

/// Fast Rec.709 decode for u16 input via LUT.
#[inline(always)]
pub fn rec709_decode_u16_fast(x: u16) -> f32 {
    rec709_decode_u16()[x as usize]
}

/// Gamma 2.2 decode table for 16‑bit input (65536 entries).
pub fn gamma22_decode_u16() -> &'static [f32; 65536] {
    static LUT: OnceLock<[f32; 65536]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 65536];
        for chunk_start in (0..65536).step_by(4096) {
            let chunk_end = (chunk_start + 4096).min(65536);
            for i in chunk_start..chunk_end {
                let x = i as f32 / 65535.0;
                arr[i] = x.powf(2.2);
            }
        }
        arr
    })
}

/// Fast Gamma 2.2 decode for u16 input via LUT.
#[inline(always)]
pub fn gamma22_decode_u16_fast(x: u16) -> f32 {
    gamma22_decode_u16()[x as usize]
}

/// Gamma 2.4 decode table for 16‑bit input (65536 entries).
pub fn gamma24_decode_u16() -> &'static [f32; 65536] {
    static LUT: OnceLock<[f32; 65536]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 65536];
        for chunk_start in (0..65536).step_by(4096) {
            let chunk_end = (chunk_start + 4096).min(65536);
            for i in chunk_start..chunk_end {
                let x = i as f32 / 65535.0;
                arr[i] = x.powf(2.4);
            }
        }
        arr
    })
}

/// Fast Gamma 2.4 decode for u16 input via LUT.
#[inline(always)]
pub fn gamma24_decode_u16_fast(x: u16) -> f32 {
    gamma24_decode_u16()[x as usize]
}

/// ProPhoto decode table for 16‑bit input (65536 entries).
pub fn prophoto_decode_u16() -> &'static [f32; 65536] {
    static LUT: OnceLock<[f32; 65536]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut arr = [0.0; 65536];
        for chunk_start in (0..65536).step_by(4096) {
            let chunk_end = (chunk_start + 4096).min(65536);
            for i in chunk_start..chunk_end {
                let x = i as f32 / 65535.0;
                arr[i] = if x <= 1.0 / 32.0 {
                    x / 16.0
                } else {
                    x.powf(1.8)
                };
            }
        }
        arr
    })
}

/// Fast ProPhoto decode for u16 input via LUT.
#[inline(always)]
pub fn prophoto_decode_u16_fast(x: u16) -> f32 {
    prophoto_decode_u16()[x as usize]
}
