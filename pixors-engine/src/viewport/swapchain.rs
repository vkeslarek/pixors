/// A circular buffer pool for multiple framebuffers.
/// Prevents tearing by separating rendering and presentation.
#[derive(Debug)]
pub struct Swapchain {
    buffers: Vec<Vec<u32>>,
    width: usize,
    height: usize,
    /// Index of the buffer currently being rendered into.
    write_index: usize,
    /// Index of the buffer last presented.
    present_index: usize,
    /// Whether a buffer is currently acquired.
    acquired: bool,
}

impl Swapchain {
    /// Creates a new swapchain with `count` buffers, each sized `width * height`.
    pub fn new(count: usize, width: usize, height: usize) -> Self {
        assert!(count >= 1, "swapchain must have at least one buffer");
        let len = width * height;
        let buffers = (0..count).map(|_| vec![0; len]).collect();
        Self {
            buffers,
            width,
            height,
            write_index: 0,
            present_index: 0,
            acquired: false,
        }
    }

    /// Returns the width of each buffer.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height of each buffer.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the total number of buffers.
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Acquires the next available buffer for rendering.
    /// Returns a mutable slice to the buffer and its index.
    /// # Panics
    /// Panics if a buffer is already acquired (call `present` first).
    pub fn acquire_next_image(&mut self) -> (&mut [u32], usize) {
        assert!(!self.acquired, "buffer already acquired");
        self.acquired = true;
        self.write_index = (self.write_index + 1) % self.buffers.len();
        let buffer = &mut self.buffers[self.write_index];
        (buffer, self.write_index)
    }

    /// Marks the currently acquired buffer as ready for presentation.
    /// # Panics
    /// Panics if no buffer is acquired.
    pub fn present(&mut self) {
        assert!(self.acquired, "no buffer acquired");
        self.acquired = false;
        self.present_index = self.write_index;
    }

    /// Returns a reference to the currently presented buffer.
    pub fn current_buffer(&self) -> &[u32] {
        &self.buffers[self.present_index]
    }

    /// Resizes all buffers to new dimensions.
    /// Existing content is discarded.
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        self.width = new_width;
        self.height = new_height;
        let len = new_width * new_height;
        for buf in &mut self.buffers {
            buf.resize(len, 0);
        }
        // Reset indices to avoid confusion after resize.
        self.write_index = 0;
        self.present_index = 0;
        self.acquired = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapchain_creation() {
        let swap = Swapchain::new(3, 100, 50);
        assert_eq!(swap.width(), 100);
        assert_eq!(swap.height(), 50);
        assert_eq!(swap.buffer_count(), 3);
        assert_eq!(swap.current_buffer().len(), 100 * 50);
    }

    #[test]
    fn swapchain_acquire_and_present() {
        let mut swap = Swapchain::new(2, 10, 10);
        // Initially present_index = 0, write_index = 0
        let (buffer, idx1) = swap.acquire_next_image();
        // write_index becomes 1, returns buffer 1
        assert_eq!(idx1, 1);
        assert_eq!(buffer.len(), 100);
        // Fill buffer
        buffer[0] = 0x12345678;
        swap.present();
        // present_index = 1
        assert_eq!(swap.current_buffer()[0], 0x12345678);
        // Acquire next buffer (should be index 0)
        let (buffer2, idx2) = swap.acquire_next_image();
        assert_eq!(idx2, 0);
        buffer2[0] = 0x87654321;
        swap.present();
        assert_eq!(swap.current_buffer()[0], 0x87654321);
    }

    #[test]
    #[should_panic(expected = "buffer already acquired")]
    fn swapchain_double_acquire_panics() {
        let mut swap = Swapchain::new(2, 5, 5);
        let _ = swap.acquire_next_image();
        let _ = swap.acquire_next_image(); // should panic
    }

    #[test]
    #[should_panic(expected = "no buffer acquired")]
    fn swapchain_present_without_acquire_panics() {
        let mut swap = Swapchain::new(2, 5, 5);
        swap.present(); // should panic
    }

    #[test]
    fn swapchain_resize() {
        let mut swap = Swapchain::new(2, 10, 10);
        swap.resize(20, 30);
        assert_eq!(swap.width(), 20);
        assert_eq!(swap.height(), 30);
        assert_eq!(swap.buffer_count(), 2);
        assert_eq!(swap.current_buffer().len(), 20 * 30);
        // Indices should be reset
        let (_, idx) = swap.acquire_next_image();
        assert_eq!(idx, 1); // because write_index starts at 0, increments to 1
    }
}