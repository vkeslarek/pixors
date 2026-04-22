/// Defines a pixors-viewport rectangle (camera) over an image.
/// Coordinates are in image space.
#[derive(Debug, Clone, Copy)]
pub struct ViewRect {
    /// X coordinate of the pixors-viewport's top-left corner in image space.
    x: f64,
    /// Y coordinate of the pixors-viewport's top-left corner in image space.
    y: f64,
    /// Scale factor: pixors-viewport pixels per image pixel.
    scale: f64,
}

impl ViewRect {
    /// Creates a new ViewRect centered on the image with scale 1.0.
    pub fn centered(width: usize, height: usize, view_width: usize, view_height: usize) -> Self {
        let scale_x = view_width as f64 / width as f64;
        let scale_y = view_height as f64 / height as f64;
        let scale = scale_x.min(scale_y).max(0.01);
        let scaled_width = width as f64 * scale;
        let scaled_height = height as f64 * scale;
        let x = (view_width as f64 - scaled_width) / 2.0 / scale;
        let y = (view_height as f64 - scaled_height) / 2.0 / scale;
        Self { x, y, scale }
    }

    /// Creates a new ViewRect with explicit parameters.
    pub fn new(x: f64, y: f64, scale: f64) -> Self {
        Self { x, y, scale }
    }

    /// Returns the X coordinate.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Returns the Y coordinate.
    pub fn y(&self) -> f64 {
        self.y
    }

    /// Returns the scale factor.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Translates the view by (dx, dy) in image pixels.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Zooms in/out by the given factor, keeping the point (anchor_x, anchor_y) fixed.
    /// Anchor coordinates are in pixors-viewport pixels (screen space).
    pub fn zoom(&mut self, factor: f64, anchor_x: f64, anchor_y: f64) {
        let old_scale = self.scale;
        let new_scale = (self.scale * factor).max(0.01).min(1000.0);
        // Convert anchor from pixors-viewport pixels to image coordinates at old scale
        let image_x = self.x + anchor_x / old_scale;
        let image_y = self.y + anchor_y / old_scale;
        // Adjust x,y so that the same image point lies under the anchor at new scale
        self.x = image_x - anchor_x / new_scale;
        self.y = image_y - anchor_y / new_scale;
        self.scale = new_scale;
    }

    /// Maps a pixors-viewport pixel coordinate (screen space) to continuous image coordinates.
    pub fn view_to_image(&self, vx: f64, vy: f64) -> (f64, f64) {
        let ix = self.x + vx / self.scale;
        let iy = self.y + vy / self.scale;
        (ix, iy)
    }

    /// Maps continuous image coordinates to pixors-viewport pixel coordinates.
    pub fn image_to_view(&self, ix: f64, iy: f64) -> (f64, f64) {
        let vx = (ix - self.x) * self.scale;
        let vy = (iy - self.y) * self.scale;
        (vx, vy)
    }

    /// Clamps the view rectangle to stay within image bounds (0..width, 0..height).
    pub fn clamp(&mut self, width: usize, height: usize) {
        let view_width = width as f64 / self.scale;
        let view_height = height as f64 / self.scale;
        self.x = self.x.clamp(-view_width * 0.1, width as f64 * 0.9);
        self.y = self.y.clamp(-view_height * 0.1, height as f64 * 0.9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_rect_new() {
        let rect = ViewRect::new(10.0, 20.0, 2.0);
        assert_eq!(rect.x(), 10.0);
        assert_eq!(rect.y(), 20.0);
        assert_eq!(rect.scale(), 2.0);
    }

    #[test]
    fn view_rect_centered() {
        // Image 100x100, view 200x200 -> scale = 2.0 (fit to smaller dimension)
        let rect = ViewRect::centered(100, 100, 200, 200);
        assert_eq!(rect.scale(), 2.0);
        // Should be centered: image scaled to 200x200 fits exactly, x = 0, y = 0
        assert_eq!(rect.x(), 0.0);
        assert_eq!(rect.y(), 0.0);
    }

    #[test]
    fn view_rect_centered_non_square() {
        // Image 200x100, view 300x300 -> scale = 1.5 (fit to height: 300/100=3, width 300/200=1.5, min=1.5)
        let rect = ViewRect::centered(200, 100, 300, 300);
        assert_eq!(rect.scale(), 1.5);
        // Scaled width = 200 * 1.5 = 300, height = 100 * 1.5 = 150
        // Horizontal centered: (300 - 300) / 2 / 1.5 = 0
        // Vertical centered: (300 - 150) / 2 / 1.5 = 75 / 1.5 = 50
        assert_eq!(rect.x(), 0.0);
        assert_eq!(rect.y(), 50.0);
    }

    #[test]
    fn view_rect_pan() {
        let mut rect = ViewRect::new(0.0, 0.0, 1.0);
        rect.pan(10.0, -5.0);
        assert_eq!(rect.x(), 10.0);
        assert_eq!(rect.y(), -5.0);
    }

    #[test]
    fn view_rect_zoom_with_anchor() {
        let mut rect = ViewRect::new(100.0, 100.0, 1.0);
        // Zoom in 2x with anchor at pixors-viewport (50, 50)
        rect.zoom(2.0, 50.0, 50.0);
        assert_eq!(rect.scale(), 2.0);
        // Verify that the image point under anchor stays the same
        // Old image coord at anchor: 100 + 50/1 = 150
        // New image coord at same anchor: x + 50/2 = 150 => x = 150 - 25 = 125
        assert_eq!(rect.x(), 125.0);
        assert_eq!(rect.y(), 125.0);
    }

    #[test]
    fn view_rect_view_to_image() {
        let rect = ViewRect::new(10.0, 20.0, 2.0);
        let (ix, iy) = rect.view_to_image(30.0, 40.0);
        // ix = 10 + 30/2 = 25
        // iy = 20 + 40/2 = 40
        assert_eq!(ix, 25.0);
        assert_eq!(iy, 40.0);
    }

    #[test]
    fn view_rect_image_to_view() {
        let rect = ViewRect::new(10.0, 20.0, 2.0);
        let (vx, vy) = rect.image_to_view(25.0, 40.0);
        // vx = (25 - 10) * 2 = 30
        // vy = (40 - 20) * 2 = 40
        assert_eq!(vx, 30.0);
        assert_eq!(vy, 40.0);
    }

    #[test]
    fn view_rect_clamp() {
        let mut rect = ViewRect::new(-50.0, -50.0, 1.0);
        rect.clamp(100, 100);
        // view_width = 100, view_height = 100
        // x clamped between -10 and 90
        // y clamped between -10 and 90
        assert!(rect.x() >= -10.0 && rect.x() <= 90.0);
        assert!(rect.y() >= -10.0 && rect.y() <= 90.0);
    }
}