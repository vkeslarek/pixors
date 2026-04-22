use serde::{Deserialize, Serialize};

/// Commands sent from frontend to engine.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientCommand {
    /// Load an image from a path.
    LoadImage { path: String },
    /// Apply an operation to the current image.
    ApplyOperation { op: String, params: serde_json::Value },
    /// Request current image dimensions.
    GetImageInfo,
    /// Close connection.
    Close,
}

/// Events sent from engine to frontend.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Image loaded successfully.
    ImageLoaded {
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    /// Operation applied successfully.
    OperationApplied,
    /// Image info response.
    ImageInfo {
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    /// Error occurred.
    Error { message: String },
    /// Binary data follows (separate WebSocket message).
    BinaryData { size: usize },
}

/// Pixel format for binary transmission.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    /// RGBA8, 4 bytes per pixel.
    Rgba8,
    /// ARGB32, 4 bytes per pixel (u32).
    Argb32,
}

impl PixelFormat {
    /// Returns bytes per pixel.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgba8 => 4,
            PixelFormat::Argb32 => 4,
        }
    }
}