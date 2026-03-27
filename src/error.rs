use thiserror::Error;

#[derive(Debug, Error)]
pub enum XlsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid BIFF8 format: {0}")]
    InvalidFormat(String),

    #[error("Unsupported BIFF version: expected BIFF8")]
    UnsupportedVersion,

    #[error("Sheet not found: {0}")]
    SheetNotFound(String),

    #[error("Invalid record at offset {offset}: {message}")]
    InvalidRecord { offset: usize, message: String },
}

pub type Result<T> = std::result::Result<T, XlsError>;
