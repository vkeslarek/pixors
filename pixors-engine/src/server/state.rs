use crate::convert::{convert_acescg_premul_to_srgb_u8, convert_raw_to_typed};
use crate::error::Error;
use crate::image::TypedImage;
use crate::io::png::load_png;
use crate::pixel::Rgba;
use half::f16;
use std::path::Path;
use tokio::sync::RwLock;

/// Shared application state.
#[derive(Default)]
pub struct AppState {
    /// Current loaded image (ACEScg premul f16).
    current_image: RwLock<Option<TypedImage<Rgba<f16>>>>,
}

impl AppState {
    /// Load an image from a path.
    pub async fn load_image(&self, path: &str) -> Result<(u32, u32), Error> {
        let raw = load_png(Path::new(path))?;
        let typed = convert_raw_to_typed(raw)?;
        let width = typed.width;
        let height = typed.height;
        *self.current_image.write().await = Some(typed);
        Ok((width, height))
    }

    /// Get current image dimensions.
    pub async fn image_info(&self) -> Option<(u32, u32)> {
        self.current_image
            .read()
            .await
            .as_ref()
            .map(|img| (img.width, img.height))
    }

    /// Convert current image to RGBA8 bytes.
    pub async fn to_rgba8(&self) -> Option<Vec<u8>> {
        self.current_image
            .read()
            .await
            .as_ref()
            .map(convert_acescg_premul_to_srgb_u8)
    }
}
