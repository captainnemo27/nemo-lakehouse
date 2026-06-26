use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, NemoError>;

#[derive(Debug, Error)]
pub enum NemoError {
    #[error("metadata error: {0}")]
    Metadata(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("graph error: {0}")]
    Graph(String),

    #[error("commit error: {0}")]
    Commit(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("table already exists: {0}")]
    TableAlreadyExists(PathBuf),

    #[error("table metadata not found: {0}")]
    TableNotFound(PathBuf),

    #[error("validation error: {0}")]
    Validation(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

