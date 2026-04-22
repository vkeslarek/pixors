/// A view into image pixel data (ARGB u32 format).
/// Does not own the data; acts as a reference/slice.
#[derive(Debug, Clone, Copy)]
pub struct ImageView<'a> {
    /// ARGB pixels in row-major order.
    data: &'a [u32],
    width: usize,
    height: usize,
    /// Number of pixels to skip to move to the next row.
    stride: usize,
}

impl<'a> ImageView<'a> {
    /// Creates a new image view from a slice of ARGB pixels.
    /// # Panics
    /// Panics if `data.len() < width * height`.
    pub fn new(data: &'a [u32], width: usize, height: usize) -> Self {
        assert!(data.len() >= width * height, "data too small for dimensions");
        Self {
            data,
            width,
            height,
            stride: width,
        }
    }

    /// Creates a new image view with a custom stride (pixels per row).
    /// # Panics
    /// Panics if `data.len() < stride * height`.
    pub fn with_stride(data: &'a [u32], width: usize, height: usize, stride: usize) -> Self {
        assert!(data.len() >= stride * height, "data too small for stride");
        Self {
            data,
            width,
            height,
            stride,
        }
    }

    /// Returns the width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the stride (pixels per row).
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Returns a reference to the underlying pixel slice.
    pub fn data(&self) -> &'a [u32] {
        self.data
    }

    /// Gets the pixel at (x, y) without bounds checking.
    /// # Safety
    /// Caller must ensure `x < width` and `y < height`.
    pub unsafe fn pixel_unchecked(&self, x: usize, y: usize) -> u32 {
        // SAFETY: caller guarantees x < width and y < height, and stride * height <= data.len()
        unsafe { *self.data.get_unchecked(y * self.stride + x) }
    }

    /// Gets the pixel at (x, y) or returns `None` if out of bounds.
    pub fn pixel(&self, x: usize, y: usize) -> Option<u32> {
        if x < self.width && y < self.height {
            Some(unsafe { self.pixel_unchecked(x, y) })
        } else {
            None
        }
    }

    /// Samples the image at continuous coordinates (x, y) using nearest neighbor.
    /// Coordinates are in image space (0..width, 0..height), where (0,0) is the top-left corner.
    /// The pixel (i,j) covers the region [i, i+1) × [j, j+1).
    /// Returns `0` (transparent black) if out of bounds.
    pub fn sample_nearest(&self, x: f64, y: f64) -> u32 {
        let ix = x.floor() as isize;
        let iy = y.floor() as isize;
        if ix >= 0 && iy >= 0 {
            let ux = ix as usize;
            let uy = iy as usize;
            if ux < self.width && uy < self.height {
                return unsafe { self.pixel_unchecked(ux, uy) };
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_view_basic() {
        // ARGB: alpha=0xFF, red=0xFF, green=0xFF, blue=0xFF
        let pixels = vec![0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFFFF];
        let view = ImageView::new(&pixels, 2, 2);
        assert_eq!(view.width(), 2);
        assert_eq!(view.height(), 2);
        assert_eq!(view.stride(), 2);
        assert_eq!(view.data().len(), 4);
    }

    #[test]
    fn image_view_pixel_access() {
        let pixels = vec![0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFFFF];
        let view = ImageView::new(&pixels, 2, 2);
        assert_eq!(view.pixel(0, 0), Some(0xFFFF0000));
        assert_eq!(view.pixel(1, 0), Some(0xFF00FF00));
        assert_eq!(view.pixel(0, 1), Some(0xFF0000FF));
        assert_eq!(view.pixel(1, 1), Some(0xFFFFFFFF));
        assert_eq!(view.pixel(2, 0), None);
        assert_eq!(view.pixel(0, 2), None);
    }

    #[test]
    fn image_view_sample_nearest() {
        let pixels = vec![0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFFFF];
        let view = ImageView::new(&pixels, 2, 2);
        // Exact coordinates
        assert_eq!(view.sample_nearest(0.0, 0.0), 0xFFFF0000);
        assert_eq!(view.sample_nearest(1.0, 0.0), 0xFF00FF00);
        assert_eq!(view.sample_nearest(0.0, 1.0), 0xFF0000FF);
        assert_eq!(view.sample_nearest(1.0, 1.0), 0xFFFFFFFF);
        // Fractional (floor)
        assert_eq!(view.sample_nearest(0.4, 0.4), 0xFFFF0000);
        assert_eq!(view.sample_nearest(0.6, 0.6), 0xFFFF0000); // still pixel (0,0)
        assert_eq!(view.sample_nearest(1.4, 1.4), 0xFFFFFFFF); // pixel (1,1)
        // Out of bounds
        assert_eq!(view.sample_nearest(-0.5, -0.5), 0);
        assert_eq!(view.sample_nearest(2.5, 2.5), 0);
    }

    #[test]
    fn image_view_with_stride() {
        // 4x2 image with stride 5 (extra column at end of each row)
        let pixels = vec![
            0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFFFF, 0x00000000,
            0x12345678, 0x87654321, 0xAAAAAAAA, 0xBBBBBBBB, 0x00000000,
        ];
        let view = ImageView::with_stride(&pixels, 4, 2, 5);
        assert_eq!(view.width(), 4);
        assert_eq!(view.height(), 2);
        assert_eq!(view.stride(), 5);
        assert_eq!(view.pixel(0, 0), Some(0xFFFF0000));
        assert_eq!(view.pixel(3, 0), Some(0xFFFFFFFF));
        assert_eq!(view.pixel(0, 1), Some(0x12345678));
        assert_eq!(view.pixel(3, 1), Some(0xBBBBBBBB));
    }

    #[test]
    #[should_panic(expected = "data too small for dimensions")]
    fn image_view_panics_on_insufficient_data() {
        let pixels = vec![0xFF0000FF];
        let _ = ImageView::new(&pixels, 2, 2);
    }
}