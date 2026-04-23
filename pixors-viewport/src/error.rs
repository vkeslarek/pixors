use thiserror::Error;
use wasm_bindgen::JsValue;

/// Errors that can occur within the Pixors WASM Viewport.
#[derive(Error, Debug)]
pub enum ViewportError {
    #[error("Canvas element not found: {0}")]
    CanvasNotFound(String),
    
    #[error("WebGL is not supported or could not be initialized")]
    NoWebGlSupport,
    
    #[error("Failed to request WebGPU device")]
    DeviceRequestFailed,
    
    #[error("Texture dimension {width}x{height} exceeds GPU max texture dimension 2D ({max_dim})")]
    TextureDimensionExceeded {
        width: u32,
        height: u32,
        max_dim: u32,
    },
    
    #[error("Tile data size mismatch: expected {expected} bytes, got {got}")]
    TileDataSizeMismatch {
        expected: usize,
        got: usize,
    },
    
    #[error("No texture has been created yet. Call create_empty_texture or update_texture first.")]
    NoTextureCreated,
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Allows returning `Result<T, ViewportError>` directly from functions
/// exported via `#[wasm_bindgen]`.
impl Into<JsValue> for ViewportError {
    fn into(self) -> JsValue {
        JsValue::from_str(&self.to_string())
    }
}
