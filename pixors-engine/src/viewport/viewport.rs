use crate::viewport::{ImageView, Swapchain, ViewRect};
use crate::viewport::sampling::nearest_neighbor_sample;

/// Main pixors-viewport orchestrator.
/// Manages swapchain, view rectangle, and rendering state.
#[derive(Debug)]
pub struct Viewport {
    swapchain: Swapchain,
    view_rect: ViewRect,
    dirty: bool,
}

impl Viewport {
    /// Creates a new pixors-viewport with a swapchain of `buffer_count` buffers.
    ///
    /// # Panics
    /// Panics if `width == 0` or `height == 0`.
    pub fn new(buffer_count: usize, width: usize, height: usize) -> Self {
        assert!(width > 0 && height > 0, "viewport dimensions must be positive");
        let swapchain = Swapchain::new(buffer_count, width, height);
        let view_rect = ViewRect::centered(1, 1, width, height); // dummy, will be set later
        Self {
            swapchain,
            view_rect,
            dirty: true,
        }
    }

    /// Returns a reference to the view rectangle.
    pub fn view_rect(&self) -> &ViewRect {
        &self.view_rect
    }

    /// Returns a mutable reference to the view rectangle.
    /// Marks the pixors-viewport as dirty.
    pub fn view_rect_mut(&mut self) -> &mut ViewRect {
        self.dirty = true;
        &mut self.view_rect
    }

    /// Sets the view rectangle and marks dirty.
    pub fn set_view_rect(&mut self, view_rect: ViewRect) {
        self.view_rect = view_rect;
        self.dirty = true;
    }

    /// Adjusts the view rectangle to fit the given image dimensions.
    /// Centers and scales to fit.
    pub fn fit_image(&mut self, image_width: usize, image_height: usize) {
        let width = self.swapchain.width();
        let height = self.swapchain.height();
        self.view_rect = ViewRect::centered(image_width, image_height, width, height);
        self.dirty = true;
    }

    /// Returns whether the pixors-viewport needs redrawing.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the pixors-viewport as clean (no need to redraw).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Renders the given image into the swapchain if dirty.
    /// Returns `true` if rendering occurred.
    pub fn render_if_dirty(&mut self, image: &ImageView) -> bool {
        if !self.dirty {
            return false;
        }
        self.render(image);
        true
    }

    /// Renders the image into the swapchain (forces redraw).
    pub fn render(&mut self, image: &ImageView) {
        let width = self.swapchain.width();
        let height = self.swapchain.height();
        let view_rect = &self.view_rect;
        let (buffer, _) = self.swapchain.acquire_next_image();

        // We simulate a software Render Pass here. We iterate through each 
        // pixel of the screen, mapping it back to the continuous image space
        // by applying the view rectangle transformations (handling pan & zoom),
        // and then we sample the image to acquire the final color.
        for y in 0..height {
            let row_start = y * width;
            for x in 0..width {
                let (ix, iy) = view_rect.view_to_image(x as f64, y as f64);
                let pixel = nearest_neighbor_sample(image, ix, iy);
                buffer[row_start + x] = pixel;
            }
        }

        self.swapchain.present();
        self.dirty = false;
    }

    /// Copies the current presented buffer into a target slice.
    /// The target slice must have length at least `width * height`.
    pub fn flush(&self, target: &mut [u32]) {
        let current = self.swapchain.current_buffer();
        target.copy_from_slice(current);
    }

    /// Handles a resize of the display surface.
    pub fn handle_resize(&mut self, new_width: usize, new_height: usize) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        if new_width != self.swapchain.width() || new_height != self.swapchain.height() {
            self.swapchain.resize(new_width, new_height);
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_creation() {
        let viewport = Viewport::new(2, 100, 50);
        assert_eq!(viewport.swapchain.width(), 100);
        assert_eq!(viewport.swapchain.height(), 50);
        assert!(viewport.dirty());
    }

    #[test]
    fn viewport_fit_image() {
        let mut viewport = Viewport::new(2, 200, 200);
        viewport.fit_image(100, 50);
        let rect = viewport.view_rect();
        // Image 100x50 fits into 200x200 with scale = 4.0? Wait compute: view width/height 200.
        // scale_x = 200/100 = 2.0, scale_y = 200/50 = 4.0, min = 2.0.
        // So scale should be 2.0.
        assert_eq!(rect.scale(), 2.0);
        assert!(viewport.dirty());
    }

    #[test]
    fn viewport_render_if_dirty() {
        let mut viewport = Viewport::new(2, 10, 10);
        let pixels = vec![0xFF0000FF; 4]; // 2x2 image
        let image = ImageView::new(&pixels, 2, 2);
        // Initially dirty
        assert!(viewport.render_if_dirty(&image));
        assert!(!viewport.dirty());
        // Second call should not render
        assert!(!viewport.render_if_dirty(&image));
    }

    #[test]
    fn viewport_flush() {
        let mut viewport = Viewport::new(2, 2, 2);
        let pixels = vec![0xFF0000FF; 4]; // 2x2 red
        let image = ImageView::new(&pixels, 2, 2);
        viewport.fit_image(2, 2);
        viewport.render(&image);
        let mut target = vec![0u32; 4];
        viewport.flush(&mut target);
        // After rendering, the pixors-viewport should have red pixels (nearest neighbor)
        // Since image fits exactly, each pixel maps to same pixel.
        assert_eq!(target, vec![0xFF0000FF; 4]);
    }

    #[test]
    fn viewport_handle_resize() {
        let mut viewport = Viewport::new(2, 100, 100);
        viewport.mark_clean();
        assert!(!viewport.dirty());
        // Resize to different dimensions
        viewport.handle_resize(150, 150);
        assert!(viewport.dirty());
        assert_eq!(viewport.swapchain.width(), 150);
        assert_eq!(viewport.swapchain.height(), 150);
        // Resize to same dimensions does nothing
        viewport.mark_clean();
        viewport.handle_resize(150, 150);
        assert!(!viewport.dirty());
    }
}