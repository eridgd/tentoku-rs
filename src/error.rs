use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TentokuError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Database not found at path: {path}")]
    DatabaseNotFound { path: String },

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Build error: {0}")]
    Build(String),
}

pub type Result<T> = std::result::Result<T, TentokuError>;
