use half::f16;
use crate::pixel::Rgba;

/// A pixel type that supports accumulate + subtract + average.
/// Useful for convolution, box blur, downsampling, etc.
pub trait PixelAccumulator: Copy + Clone + Send + Sync + 'static {
    type Sum: Copy + Default;

    fn accumulate(&self, sum: &mut Self::Sum);

    fn subtract(sum: &mut Self::Sum, pixel: &Self);

    fn from_sum(sum: Self::Sum, count: u32) -> Self;
}

// ── impls ──────────────────────────────────────────────────────────────────

impl PixelAccumulator for [u8; 4] {
    type Sum = [u32; 4];

    fn accumulate(&self, sum: &mut [u32; 4]) {
        sum[0] += self[0] as u32;
        sum[1] += self[1] as u32;
        sum[2] += self[2] as u32;
        sum[3] += self[3] as u32;
    }

    fn subtract(sum: &mut [u32; 4], pixel: &Self) {
        sum[0] = sum[0].saturating_sub(pixel[0] as u32);
        sum[1] = sum[1].saturating_sub(pixel[1] as u32);
        sum[2] = sum[2].saturating_sub(pixel[2] as u32);
        sum[3] = sum[3].saturating_sub(pixel[3] as u32);
    }

    fn from_sum(sum: [u32; 4], count: u32) -> Self {
        let c = count.max(1);
        [(sum[0] / c) as u8, (sum[1] / c) as u8, (sum[2] / c) as u8, (sum[3] / c) as u8]
    }
}

impl PixelAccumulator for Rgba<f16> {
    type Sum = [f32; 4];

    fn accumulate(&self, sum: &mut [f32; 4]) {
        sum[0] += self.r.to_f32();
        sum[1] += self.g.to_f32();
        sum[2] += self.b.to_f32();
        sum[3] += self.a.to_f32();
    }

    fn subtract(sum: &mut [f32; 4], pixel: &Self) {
        sum[0] -= pixel.r.to_f32();
        sum[1] -= pixel.g.to_f32();
        sum[2] -= pixel.b.to_f32();
        sum[3] -= pixel.a.to_f32();
    }

    fn from_sum(sum: [f32; 4], count: u32) -> Self {
        let c = count.max(1) as f32;
        Rgba {
            r: f16::from_f32(sum[0] / c),
            g: f16::from_f32(sum[1] / c),
            b: f16::from_f32(sum[2] / c),
            a: f16::from_f32(sum[3] / c),
        }
    }
}
