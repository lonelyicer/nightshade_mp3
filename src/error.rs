use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("invalid socket address: {0}")]
    Address(#[from] std::net::AddrParseError),

    #[error("{0}")]
    Message(String),
}

pub type AppResult<T> = Result<T, AppError>;
