use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("color conversion error: {0}")]
    ColorConversion(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

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

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
