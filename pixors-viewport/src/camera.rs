//! UV-space camera: pan, zoom, and aspect-ratio-correct fit.
//!
//! ## Coordinate systems
//!
//! | Space        | Range    | Description                              |
//! |--------------|----------|------------------------------------------|
//! | Screen UV    | [0,1]²   | Top-left origin, matches canvas pixels   |
//! | Texture UV   | [0,1]²   | Top-left origin, maps into image texels  |
//!
//! The fragment shader samples the image at:
//! ```text
//! tex_uv = uv_offset + screen_uv * uv_scale
//! ```
//! Pixels where `tex_uv` falls outside `[0,1]²` are clipped to the background colour
//! in the shader — this prevents colour bleed from `ClampToEdge` at the image border.
//!
//! ## Zoom model
//!
//! `zoom = 1.0` → image fills the viewport (letterboxed / pillarboxed to preserve
//! aspect ratio).  Higher zoom = more magnified (smaller visible UV region).

use glam::Vec2;
use bytemuck::{Pod, Zeroable};

// ── GPU uniform ───────────────────────────────────────────────────────────────

/// Matches the `CameraUniform` struct in `shader.wgsl` (16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    /// Texture UV at the top-left corner of the screen.
    pub uv_offset: [f32; 2],
    /// How much texture UV space the full screen covers.
    pub uv_scale: [f32; 2],
}

// ── Camera ────────────────────────────────────────────────────────────────────

/// Stateful camera that maps screen UV → texture UV.
#[derive(Clone, Copy)]
pub(crate) struct Camera {
    /// Texture UV coordinate at the centre of the screen.
    pub center: Vec2,
    /// Magnification level. `1.0` = fit-to-viewport.
    pub zoom: f32,
    /// Loaded image dimensions in pixels.
    pub image_size: Vec2,
    /// Viewport (canvas) dimensions in pixels.
    pub viewport_size: Vec2,
}

impl Camera {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            center: Vec2::splat(0.5),
            zoom: 1.0,
            // Assume square image until update_texture sets the real size.
            image_size: Vec2::new(viewport_w, viewport_h),
            viewport_size: Vec2::new(viewport_w, viewport_h),
        }
    }

    /// Reset to fit the image into the viewport at `zoom = 1`.
    pub fn fit(&mut self) {
        self.center = Vec2::splat(0.5);
        self.zoom = 1.0;
    }

    /// Translate by `delta_px` screen pixels (positive X → pan right).
    pub fn pan(&mut self, delta_px: Vec2) {
        // Convert pixel delta to UV delta then apply.
        let scale = self.uv_scale();
        self.center -= delta_px / self.viewport_size * scale;
    }

    /// Zoom by `factor` keeping `anchor_screen` (screen UV) fixed in image space.
    pub fn zoom_at(&mut self, factor: f32, anchor_screen: Vec2) {
        // Compute the texture UV under the anchor before the zoom.
        let scale = self.uv_scale();
        let anchor_uv = (self.center - scale * 0.5) + anchor_screen * scale;

        self.zoom = (self.zoom * factor).clamp(0.05, 100.0);

        // Shift center so that anchor_uv stays under anchor_screen.
        let new_scale = self.uv_scale();
        let new_offset = anchor_uv - anchor_screen * new_scale;
        self.center = new_offset + new_scale * 0.5;
    }

    /// Build the GPU uniform for the current camera state.
    pub fn to_uniform(&self) -> CameraUniform {
        let scale = self.uv_scale();
        let offset = self.center - scale * 0.5;
        CameraUniform {
            uv_offset: offset.to_array(),
            uv_scale: scale.to_array(),
        }
    }

    // ── private helpers ───────────────────────────────────────────────────────

    /// Fraction of texture UV space visible on screen, accounting for aspect ratio.
    ///
    /// At `zoom = 1` the image is letterboxed / pillarboxed so it fits exactly
    /// inside the viewport without distortion.
    fn uv_scale(&self) -> Vec2 {
        let img_ar = self.image_size.x / self.image_size.y;
        let vp_ar = self.viewport_size.x / self.viewport_size.y;

        // Fit along the constraining axis; the other axis gets padding.
        let (bw, bh) = if img_ar >= vp_ar {
            (1.0f32, vp_ar / img_ar) // image wider → fit width, pad height
        } else {
            (img_ar / vp_ar, 1.0f32) // image taller → fit height, pad width
        };

        Vec2::new(bw / self.zoom, bh / self.zoom)
    }
}
