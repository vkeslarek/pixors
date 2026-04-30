use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("unsupported sample type: {0}")]
    UnsupportedSampleType(String),

    #[error("unsupported channel layout: {0}")]
    UnsupportedChannelLayout(String),

    #[error("unsupported color space: {0}")]
    UnsupportedColorSpace(String),

    #[error("unsupported transfer function: {0}")]
    UnsupportedTransferFunction(String),

    #[error("color conversion error: {0}")]
    ColorConversion(String),

    #[error("alpha operation error: {0}")]
    AlphaOperation(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("PNG error: {0}")]
    Png(String),

    #[error("TIFF error: {0}")]
    Tiff(String),

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    pub fn invalid_param(msg: impl Into<String>) -> Self {
        Self::InvalidParameter(msg.into())
    }

    pub fn unsupported_sample_type(msg: impl Into<String>) -> Self {
        Self::UnsupportedSampleType(msg.into())
    }

    pub fn unsupported_channel_layout(msg: impl Into<String>) -> Self {
        Self::UnsupportedChannelLayout(msg.into())
    }

    pub fn unsupported_color_space(msg: impl Into<String>) -> Self {
        Self::UnsupportedColorSpace(msg.into())
    }

    pub fn unsupported_transfer_function(msg: impl Into<String>) -> Self {
        Self::UnsupportedTransferFunction(msg.into())
    }

    pub fn color_conversion(msg: impl Into<String>) -> Self {
        Self::ColorConversion(msg.into())
    }

    pub fn alpha_op(msg: impl Into<String>) -> Self {
        Self::AlphaOperation(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
