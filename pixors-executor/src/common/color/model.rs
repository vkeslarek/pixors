//! Color model decode — converts non-RGB pixel layouts (CMYK, YCbCr) to
//! sRGB-encoded [0,255] float4, after which the standard transfer → matrix →
//! encode pipeline takes over. The matrix step goes through CIE XYZ as the
//! universal intermediate, so model decode only needs to produce sRGB values.

use wide::f32x4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ColorModelTransform {
    None = 0,
    CmykToRgb = 1,
    CmykAToRgb = 2,
    YCbCrToRgb = 3,
}

impl ColorModelTransform {
    /// Apply model decode to 4 pixels at once (SIMD-friendly).
    /// Input: channel vectors from `unpack_x4`, each element is a float in
    /// the source encoding's native range (0–255 for u8, 0–65535 for u16).
    /// Output: sRGB-encoded R, G, B in the same numeric range as input.
    /// Alpha channel passes through unchanged.
    pub fn decode_4(
        &self,
        c0: f32x4,
        c1: f32x4,
        c2: f32x4,
        c3: f32x4,
    ) -> (f32x4, f32x4, f32x4) {
        match self {
            Self::None => (c0, c1, c2),
            Self::CmykToRgb => {
                let one = f32x4::splat(1.0);
                let scale = one - c3;
                let r = (one - c0) * scale;
                let g = (one - c1) * scale;
                let b = (one - c2) * scale;
                (r, g, b)
            }
            Self::CmykAToRgb => {
                // CMYK→RGB, preserve alpha in c3 (K channel becomes K, a stays)
                // Actually: c0=C, c1=M, c2=Y, c3=K. Alpha is NOT in unpack — it's the 5th byte.
                // For CmykA<u8>, unpack gives [C,M,Y,K] (4 values, normalized).
                // The 5th channel (alpha) is not in f32x4. We handle it differently.
                // Same as CmykToRgb since alpha is separate.
                let one = f32x4::splat(1.0);
                let scale = one - c3;
                let r = (one - c0) * scale;
                let g = (one - c1) * scale;
                let b = (one - c2) * scale;
                (r, g, b)
            }
            Self::YCbCrToRgb => {
                // Y=c0, Cb=c1, Cr=c2 in [0,1] → R, G, B (ITU-R BT.601)
                let cb = c1 - f32x4::splat(0.5);
                let cr = c2 - f32x4::splat(0.5);
                let r = c0 + cr * f32x4::splat(1.402);
                let g = c0 - cb * f32x4::splat(0.344136) - cr * f32x4::splat(0.714136);
                let b = c0 + cb * f32x4::splat(1.772);
                (r, g, b)
            }
        }
    }

    /// Apply model decode to a single pixel.
    /// Input: [c0, c1, c2, c3] from `unpack()`.
    /// Output: [r, g, b] in the same numeric range.
    pub fn decode_1(&self, ch: &[f32; 4]) -> [f32; 3] {
        match self {
            Self::None => [ch[0], ch[1], ch[2]],
            Self::CmykToRgb => {
                let scale = 1.0 - ch[3];
                [
                    (1.0 - ch[0]) * scale,
                    (1.0 - ch[1]) * scale,
                    (1.0 - ch[2]) * scale,
                ]
            }
            Self::CmykAToRgb => {
                let scale = 1.0 - ch[3];
                [
                    (1.0 - ch[0]) * scale,
                    (1.0 - ch[1]) * scale,
                    (1.0 - ch[2]) * scale,
                ]
            }
            Self::YCbCrToRgb => {
                let cb = ch[1] - 0.5;
                let cr = ch[2] - 0.5;
                [
                    (ch[0] + 1.402 * cr).clamp(0.0, 1.0),
                    (ch[0] - 0.344136 * cb - 0.714136 * cr).clamp(0.0, 1.0),
                    (ch[0] + 1.772 * cb).clamp(0.0, 1.0),
                ]
            }
        }
    }
}
