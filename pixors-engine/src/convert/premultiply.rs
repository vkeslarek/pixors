//! Alpha premultiplication and unpremultiplication.

use crate::error::Error;
use crate::image::ChannelLayoutKind;

/// Multiplies color channels by alpha (converts straight → premultiplied).
///
/// Generic over any layout with alpha: alpha is always the last channel.
/// Layouts without alpha (Rgb, Gray) are a no-op.
pub fn premultiply(
    data: &mut [f32],
    channel_layout: &ChannelLayoutKind,
) -> Result<(), Error> {
    let n = channel_layout.channel_count();
    if !channel_layout.has_alpha() || n < 2 {
        return Ok(());
    }
    for px in data.chunks_exact_mut(n) {
        let alpha = px[n - 1];
        for c in px.iter_mut().take(n - 1) {
            *c *= alpha;
        }
    }
    Ok(())
}

/// Divides color channels by alpha (converts premultiplied → straight).
///
/// Generic over any layout with alpha: alpha is always the last channel.
/// If alpha ≤ 1e-6, color channels are set to zero.
pub fn unpremultiply(
    data: &mut [f32],
    channel_layout: &ChannelLayoutKind,
) -> Result<(), Error> {
    let n = channel_layout.channel_count();
    if !channel_layout.has_alpha() || n < 2 {
        return Err(Error::invalid_param("cannot unpremultiply layout without alpha"));
    }
    for px in data.chunks_exact_mut(n) {
        let alpha = px[n - 1];
        if alpha.abs() <= 1e-6 {
            for c in px.iter_mut().take(n - 1) {
                *c = 0.0;
            }
        } else {
            let inv = 1.0 / alpha;
            for c in px.iter_mut().take(n - 1) {
                *c *= inv;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_rgba() {
        let mut data = vec![1.0, 0.5, 0.2, 0.5, 0.0, 1.0, 0.0, 1.0];
        premultiply(&mut data, &ChannelLayoutKind::Rgba).unwrap();
        assert_eq!(data[0], 0.5);
        assert_eq!(data[1], 0.25);
        assert_eq!(data[2], 0.1);
        assert_eq!(data[3], 0.5);
        assert_eq!(data[4], 0.0);
        assert_eq!(data[5], 1.0);
        assert_eq!(data[6], 0.0);
        assert_eq!(data[7], 1.0);
    }

    #[test]
    fn unpremultiply_rgba() {
        let mut data = vec![0.5, 0.25, 0.1, 0.5, 0.0, 1.0, 0.0, 1.0];
        unpremultiply(&mut data, &ChannelLayoutKind::Rgba).unwrap();
        assert_eq!(data[0], 1.0);
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.2);
        assert_eq!(data[3], 0.5);
        assert_eq!(data[4], 0.0);
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
        let mut data = vec![0.8, 0.5, 1.0, 0.0];
        premultiply(&mut data, &ChannelLayoutKind::GrayAlpha).unwrap();
        assert_eq!(data[0], 0.4);
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.0);
        assert_eq!(data[3], 0.0);
    }

    #[test]
    fn unpremultiply_gray_alpha() {
        let mut data = vec![0.4, 0.5, 0.0, 0.0];
        unpremultiply(&mut data, &ChannelLayoutKind::GrayAlpha).unwrap();
        assert_eq!(data[0], 0.8);
        assert_eq!(data[1], 0.5);
        assert_eq!(data[2], 0.0);
        assert_eq!(data[3], 0.0);
    }
}
