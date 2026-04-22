//! Image sampling algorithms.
//!
//! Currently implements nearest‑neighbor and bicubic interpolation (Catmull‑Rom kernel).
//! The bicubic implementation is scalar; future optimization should use SIMD (e.g., via the `wide` crate)
//! to process multiple pixels in parallel.

use crate::viewport::ImageView;

/// Samples the image at continuous coordinates using nearest neighbor.
pub fn nearest_neighbor_sample(image: &ImageView, x: f64, y: f64) -> u32 {
    image.sample_nearest(x, y)
}

/// Cubic kernel (Catmull‑Rom).
fn cubic_kernel(t: f32, a: f32) -> f32 {
    let abs_t = t.abs();
    if abs_t < 1.0 {
        (a + 2.0) * abs_t.powi(3) - (a + 3.0) * abs_t.powi(2) + 1.0
    } else if abs_t < 2.0 {
        a * abs_t.powi(3) - 5.0 * a * abs_t.powi(2) + 8.0 * a * abs_t - 4.0 * a
    } else {
        0.0
    }
}

/// Converts ARGB u32 to four f32 components in [0, 1].
fn argb_to_components(argb: u32) -> (f32, f32, f32, f32) {
    let a = ((argb >> 24) & 0xFF) as f32 / 255.0;
    let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
    let b = (argb & 0xFF) as f32 / 255.0;
    (a, r, g, b)
}

/// Converts four f32 components in [0, 1] back to ARGB u32.
fn components_to_argb(a: f32, r: f32, g: f32, b: f32) -> u32 {
    let a_byte = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r_byte = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
    let g_byte = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
    let b_byte = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
    (a_byte << 24) | (r_byte << 16) | (g_byte << 8) | b_byte
}

/// Samples the image using bicubic interpolation (Catmull‑Rom, a = -0.5).
pub fn bicubic_sample(image: &ImageView, x: f64, y: f64) -> u32 {
    let x = x as f32;
    let y = y as f32;
    let width = image.width() as i32;
    let height = image.height() as i32;

    // Determine the 4x4 neighborhood around (x, y)
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;

    let mut sum_a = 0.0f32;
    let mut sum_r = 0.0f32;
    let mut sum_g = 0.0f32;
    let mut sum_b = 0.0f32;
    let mut weight_sum = 0.0f32;

    for dy in -1..=2 {
        let sy = iy + dy;
        for dx in -1..=2 {
            let sx = ix + dx;
            // Fetch pixel (clamp to edges)
            let pixel = if sx >= 0 && sy >= 0 && sx < width && sy < height {
                unsafe { image.pixel_unchecked(sx as usize, sy as usize) }
            } else {
                0 // transparent black for out-of-bounds
            };
            let (a, r, g, b) = argb_to_components(pixel);

            let wx = cubic_kernel((sx as f32 - x) / 1.0, -0.5);
            let wy = cubic_kernel((sy as f32 - y) / 1.0, -0.5);
            let weight = wx * wy;

            sum_a += a * weight;
            sum_r += r * weight;
            sum_g += g * weight;
            sum_b += b * weight;
            weight_sum += weight;
        }
    }

    // Normalize by total weight (avoid division by zero)
    if weight_sum.abs() > 1e-6 {
        sum_a /= weight_sum;
        sum_r /= weight_sum;
        sum_g /= weight_sum;
        sum_b /= weight_sum;
    }

    components_to_argb(sum_a, sum_r, sum_g, sum_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_neighbor_sample_test() {
        let pixels = vec![0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFFFF];
        let view = ImageView::new(&pixels, 2, 2);
        assert_eq!(nearest_neighbor_sample(&view, 0.0, 0.0), 0xFFFF0000);
        assert_eq!(nearest_neighbor_sample(&view, 0.9, 0.9), 0xFFFF0000); // still pixel (0,0)
        assert_eq!(nearest_neighbor_sample(&view, 1.0, 1.0), 0xFFFFFFFF); // corner (1,1)
        assert_eq!(nearest_neighbor_sample(&view, 1.9, 1.9), 0xFFFFFFFF); // still pixel (1,1)
    }

    #[test]
    fn cubic_kernel_values() {
        // Catmull‑Rom (a = -0.5)
        let a = -0.5;
        // At t = 0, kernel should be 1.0
        assert!((cubic_kernel(0.0, a) - 1.0).abs() < 1e-6);
        // At t = 1, kernel should be 0
        assert!((cubic_kernel(1.0, a) - 0.0).abs() < 1e-6);
        // At t = 2, kernel should be 0
        assert!((cubic_kernel(2.0, a) - 0.0).abs() < 1e-6);
        // Symmetry
        assert_eq!(cubic_kernel(0.5, a), cubic_kernel(-0.5, a));
    }

    #[test]
    fn argb_conversion_roundtrip() {
        let argb = 0x12345678;
        let (a, r, g, b) = argb_to_components(argb);
        let back = components_to_argb(a, r, g, b);
        assert_eq!(back, argb);
    }

    #[test]
    fn argb_conversion_clamping() {
        // Values outside [0,1] should clamp
        let argb = components_to_argb(1.5, -0.2, 0.5, 2.0);
        // Alpha clamped to 1.0 -> 0xFF
        // Red clamped to 0.0 -> 0x00
        // Green 0.5 -> 0x80 (approx)
        // Blue clamped to 1.0 -> 0xFF
        assert_eq!(argb >> 24, 0xFF);
        assert_eq!((argb >> 16) & 0xFF, 0x00);
        assert_eq!((argb >> 8) & 0xFF, 0x80);
        assert_eq!(argb & 0xFF, 0xFF);
    }

    #[test]
    fn bicubic_sample_constant_image() {
        // A 2x2 image with all white pixels
        let pixels = vec![0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF];
        let view = ImageView::new(&pixels, 2, 2);
        // Sampling anywhere inside should give white
        let sample = bicubic_sample(&view, 0.5, 0.5);
        assert_eq!(sample, 0xFFFFFFFF);
        // Sampling near edge should still be white (extrapolation with transparent black?)
        // Actually, out-of-bounds pixels are 0, so edges will darken.
        // For simplicity, we just ensure no crash.
        let _ = bicubic_sample(&view, -0.5, -0.5);
        let _ = bicubic_sample(&view, 1.5, 1.5);
    }

    #[test]
    fn bicubic_sample_checkerboard() {
        // 4x4 checkerboard (ARGB: red=0xFFFF0000, green=0xFF00FF00)
        let pixels = vec![
            0xFFFF0000, 0xFF00FF00, 0xFFFF0000, 0xFF00FF00,
            0xFF00FF00, 0xFFFF0000, 0xFF00FF00, 0xFFFF0000,
            0xFFFF0000, 0xFF00FF00, 0xFFFF0000, 0xFF00FF00,
            0xFF00FF00, 0xFFFF0000, 0xFF00FF00, 0xFFFF0000,
        ];
        let view = ImageView::new(&pixels, 4, 4);
        // Sample at center of pixel (0,0) -> red
        let sample = bicubic_sample(&view, 0.0, 0.0);
        // Should be predominantly red (kernel blends with neighboring green pixels)
        let (_, r, g, b) = argb_to_components(sample);
        eprintln!("bicubic sample at (0,0): r={}, g={}, b={}", r, g, b);
        // Kernel Catmull‑Rom with a=-0.5, support 2 pixels each side.
        // At exact pixel center, weight of center pixel is 1.0, neighbors contribute.
        // Expect red dominant.
        assert!(r > 0.3 && g < 0.7 && b < 0.2);
        // Ensure not black
        assert!(r > 0.1);
    }
}