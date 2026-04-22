//! Alpha premultiplication and unpremultiplication.

use crate::error::Error;
use crate::image::{AlphaMode, ChannelLayoutKind};

/// Multiplies color channels by alpha (converts straight → premultiplied).
///
/// `data` is a slice of `f32` samples in interleaved layout.
/// `channel_layout` describes the channel order.
/// Returns `Ok(())` on success, or `Err` if division by near‑zero alpha would produce infinity.
pub fn premultiply(
    data: &mut [f32],
    channel_layout: &ChannelLayoutKind,
) -> Result<(), Error> {
    match channel_layout {
        ChannelLayoutKind::Rgba => {
            for rgba in data.chunks_exact_mut(4) {
                let alpha = rgba[3];
                rgba[0] *= alpha;
                rgba[1] *= alpha;
                rgba[2] *= alpha;
            }
            Ok(())
        }
        ChannelLayoutKind::GrayAlpha => {
            for ga in data.chunks_exact_mut(2) {
                let alpha = ga[1];
                ga[0] *= alpha;
            }
            Ok(())
        }
        ChannelLayoutKind::Rgb | ChannelLayoutKind::Gray => {
            // No alpha channel → nothing to do.
            Ok(())
        }
        ChannelLayoutKind::Yuv | ChannelLayoutKind::YuvA | ChannelLayoutKind::Cmyk | ChannelLayoutKind::Custom(_) => {
            // Unsupported in Phase 1; treat as no‑op.
            Ok(())
        }
    }
}

/// Divides color channels by alpha (converts premultiplied → straight).
///
/// If alpha is zero or extremely small (≤ `1e-6`), the color channels are set to zero.
/// Returns `Ok(())` on success, `Err` if the layout has no alpha channel.
pub fn unpremultiply(
    data: &mut [f32],
    channel_layout: &ChannelLayoutKind,
) -> Result<(), Error> {
    match channel_layout {
        ChannelLayoutKind::Rgba => {
            for rgba in data.chunks_exact_mut(4) {
                let alpha = rgba[3];
                if alpha.abs() <= 1e-6 {
                    rgba[0] = 0.0;
                    rgba[1] = 0.0;
                    rgba[2] = 0.0;
                } else {
                    rgba[0] /= alpha;
                    rgba[1] /= alpha;
                    rgba[2] /= alpha;
                }
            }
            Ok(())
        }
        ChannelLayoutKind::GrayAlpha => {
            for ga in data.chunks_exact_mut(2) {
                let alpha = ga[1];
                if alpha.abs() <= 1e-6 {
                    ga[0] = 0.0;
                } else {
                    ga[0] /= alpha;
                }
            }
            Ok(())
        }
        ChannelLayoutKind::Rgb | ChannelLayoutKind::Gray => {
            Err(Error::invalid_param("cannot unpremultiply layout without alpha"))
        }
        ChannelLayoutKind::Yuv | ChannelLayoutKind::YuvA | ChannelLayoutKind::Cmyk | ChannelLayoutKind::Custom(_) => {
            Err(Error::invalid_param("unpremultiply not supported for this layout"))
        }
    }
}

/// Converts between alpha modes in‑place.
///
/// If `from` and `to` are the same, does nothing.
/// Returns `Err` if conversion is impossible (e.g., unpremultiply on opaque).
#[allow(dead_code)]
pub fn convert_alpha_mode(
    data: &mut [f32],
    channel_layout: &ChannelLayoutKind,
    from: AlphaMode,
    to: AlphaMode,
) -> Result<(), Error> {
    if from == to {
        return Ok(());
    }

    match (from, to) {
        (AlphaMode::Straight, AlphaMode::Premultiplied) => premultiply(data, channel_layout),
        (AlphaMode::Premultiplied, AlphaMode::Straight) => unpremultiply(data, channel_layout),
        (AlphaMode::Opaque, AlphaMode::Straight) | (AlphaMode::Opaque, AlphaMode::Premultiplied) => {
            // Opaque means α = 1 everywhere; we need to add an alpha channel.
            // This function expects the layout already has alpha (should be validated earlier).
            Ok(())
        }
        (AlphaMode::Straight, AlphaMode::Opaque) | (AlphaMode::Premultiplied, AlphaMode::Opaque) => {
            // Removing alpha is a lossy operation; not supported in Phase 1.
            Err(Error::invalid_param("cannot convert to opaque without background compositing"))
        }
        _ => Err(Error::invalid_param("unsupported alpha mode conversion")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_rgba() {
        let mut data = vec![1.0, 0.5, 0.2, 0.5,  // r,g,b,a
                            0.0, 1.0, 0.0, 1.0];
        premultiply(&mut data, &ChannelLayoutKind::Rgba).unwrap();
        assert_eq!(data[0], 0.5);  // 1.0 * 0.5
        assert_eq!(data[1], 0.25); // 0.5 * 0.5
        assert_eq!(data[2], 0.1);  // 0.2 * 0.5
        assert_eq!(data[3], 0.5);  // alpha unchanged
        assert_eq!(data[4], 0.0);  // 0.0 * 1.0
        assert_eq!(data[5], 1.0);  // 1.0 * 1.0
        assert_eq!(data[6], 0.0);  // 0.0 * 1.0
        assert_eq!(data[7], 1.0);
    }

    #[test]
    fn unpremultiply_rgba() {
        let mut data = vec![0.5, 0.25, 0.1, 0.5,
                            0.0, 1.0, 0.0, 1.0];
        unpremultiply(&mut data, &ChannelLayoutKind::Rgba).unwrap();
        assert_eq!(data[0], 1.0);   // 0.5 / 0.5
        assert_eq!(data[1], 0.5);   // 0.25 / 0.5
        assert_eq!(data[2], 0.2);   // 0.1 / 0.5
        assert_eq!(data[3], 0.5);
        assert_eq!(data[4], 0.0);   // 0.0 / 1.0
        assert_eq!(data[5], 1.0);
        assert_eq!(data[6], 0.0);
        assert_eq!(data[7], 1.0);
    }

    #[test]
    fn unpremultiply_zero_alpha() {
        let mut data = vec![0.3, 0.4, 0.5, 0.0];
        unpremultiply(&mut data, &ChannelLayoutKind::Rgba).unwrap();
        assert_eq!(data[0], 0.0);
        assert_eq!(data[1], 0.0);
        assert_eq!(data[2], 0.0);
        assert_eq!(data[3], 0.0);
    }

    #[test]
    fn premultiply_gray_alpha() {
        let mut data = vec![0.8, 0.5,  // v, a
                            1.0, 0.0];
        premultiply(&mut data, &ChannelLayoutKind::GrayAlpha).unwrap();
        assert_eq!(data[0], 0.4);  // 0.8 * 0.5
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.0);  // 1.0 * 0.0
        assert_eq!(data[3], 0.0);
    }

    #[test]
    fn unpremultiply_gray_alpha() {
        let mut data = vec![0.4, 0.5,
                            0.0, 0.0];
        unpremultiply(&mut data, &ChannelLayoutKind::GrayAlpha).unwrap();
        assert_eq!(data[0], 0.8);  // 0.4 / 0.5
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.0);  // zero alpha → zero value
        assert_eq!(data[3], 0.0);
    }

    #[test]
    fn convert_alpha_mode_straight_to_premul() {
        let mut data = vec![1.0, 0.5, 0.2, 0.5];
        convert_alpha_mode(
            &mut data,
            &ChannelLayoutKind::Rgba,
            AlphaMode::Straight,
            AlphaMode::Premultiplied,
        ).unwrap();
        assert_eq!(data[0], 0.5);
        assert_eq!(data[1], 0.25);
        assert_eq!(data[2], 0.1);
        assert_eq!(data[3], 0.5);
    }

    #[test]
    fn convert_alpha_mode_premul_to_straight() {
        let mut data = vec![0.5, 0.25, 0.1, 0.5];
        convert_alpha_mode(
            &mut data,
            &ChannelLayoutKind::Rgba,
            AlphaMode::Premultiplied,
            AlphaMode::Straight,
        ).unwrap();
        assert_eq!(data[0], 1.0);
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.2);
        assert_eq!(data[3], 0.5);
    }
}