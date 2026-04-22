//! Viewport, swapchain, and interactive rendering for Phase 2.
//!
//! This module provides the core abstractions for displaying images with pan and zoom:
//! - [`ImageView`]: a non‑owning view into ARGB pixel data.
//! - [`ViewRect`]: a camera defining which region of the image is visible and at what scale.
//! - [`Swapchain`]: a pool of framebuffers enabling tear‑free rendering.
//! - [`Viewport`]: the orchestrator that manages the swapchain, view rectangle, and rendering.
//!
//! The rendering pipeline samples the source image using either nearest‑neighbor or
//! bicubic interpolation (Catmull‑Rom kernel) and writes the result into the swapchain.
//! Interactive pan and zoom are implemented by modifying the [`ViewRect`] in response to
//! mouse events.

mod image_view;
mod view_rect;
mod swapchain;
mod viewport;
mod sampling;

pub use image_view::ImageView;
pub use view_rect::ViewRect;
pub use swapchain::Swapchain;
pub use viewport::Viewport;
pub use sampling::{nearest_neighbor_sample, bicubic_sample};