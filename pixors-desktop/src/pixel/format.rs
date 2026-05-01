use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Rgba8,
    Argb32,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgba8 => 4,
            PixelFormat::Argb32 => 4,
        }
    }
}
