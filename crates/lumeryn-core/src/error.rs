use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unsupported capability: {0}")]
    Unsupported(String),
    #[error("revision conflict: expected {expected}, current {current}")]
    Conflict { expected: u64, current: u64 },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("path already exists: {0}")]
    Exists(String),
    #[error("media worker failure: {0}")]
    Media(String),
    #[error("media worker timed out")]
    Timeout,
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("project storage: {0}")]
    Storage(#[from] sqlx::Error),
    #[error("invalid serialized data: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Json(_) => "invalid_input",
            Self::Unsupported(_) => "unsupported",
            Self::Conflict { .. } => "revision_conflict",
            Self::NotFound(_) => "not_found",
            Self::Exists(_) => "already_exists",
            Self::Media(_) => "media_failure",
            Self::Timeout => "timeout",
            Self::Io(_) => "io_failure",
            Self::Storage(_) => "storage_failure",
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Invalid(_) | Self::Json(_) => 2,
            Self::Conflict { .. } => 3,
            Self::Unsupported(_) => 4,
            Self::Media(_) | Self::Timeout => 5,
            _ => 1,
        }
    }
}
