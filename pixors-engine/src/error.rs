use thiserror::Error;
use png::DecodingError;

/// Represents all possible errors in the pixors library.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid image dimensions (zero width or height).
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    /// Unsupported sample type for an operation.
    #[error("unsupported sample type: {0}")]
    UnsupportedSampleType(String),

    /// Unsupported channel layout.
    #[error("unsupported channel layout: {0}")]
    UnsupportedChannelLayout(String),

    /// Unsupported color space.
    #[error("unsupported color space: {0}")]
    UnsupportedColorSpace(String),

    /// Unsupported transfer function.
    #[error("unsupported transfer function: {0}")]
    UnsupportedTransferFunction(String),

    /// Color conversion error (e.g., matrix inversion failed).
    #[error("color conversion error: {0}")]
    ColorConversion(String),

    /// Alpha operation error (division by zero or near-zero).
    #[error("alpha operation error: {0}")]
    AlphaOperation(String),

    /// I/O error (file reading/writing).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// PNG decoding/encoding error.
    #[error("PNG error: {0}")]
    Png(String),

    /// Invalid parameter value.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    /// Generic error for cases not covered above.
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Creates an invalid parameter error.
    pub fn invalid_param(msg: impl Into<String>) -> Self {
        Self::InvalidParameter(msg.into())
    }

    /// Creates an unsupported sample type error.
    pub fn unsupported_sample_type(msg: impl Into<String>) -> Self {
        Self::UnsupportedSampleType(msg.into())
    }

    /// Creates an unsupported channel layout error.
    pub fn unsupported_channel_layout(msg: impl Into<String>) -> Self {
        Self::UnsupportedChannelLayout(msg.into())
    }

    /// Creates an unsupported color space error.
    pub fn unsupported_color_space(msg: impl Into<String>) -> Self {
        Self::UnsupportedColorSpace(msg.into())
    }

    /// Creates an unsupported transfer function error.
    pub fn unsupported_transfer_function(msg: impl Into<String>) -> Self {
        Self::UnsupportedTransferFunction(msg.into())
    }

    /// Creates a color conversion error.
    pub fn color_conversion(msg: impl Into<String>) -> Self {
        Self::ColorConversion(msg.into())
    }

    /// Creates an alpha operation error.
    pub fn alpha_op(msg: impl Into<String>) -> Self {
        Self::AlphaOperation(msg.into())
    }

    /// Creates an internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl From<DecodingError> for Error {
    fn from(err: DecodingError) -> Self {
        Self::Png(err.to_string())
    }
}