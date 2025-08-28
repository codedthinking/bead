use thiserror::Error;

#[derive(Error, Debug)]
pub enum BeadError {
    #[error("Invalid workspace: {0}")]
    InvalidWorkspace(String),
    
    #[error("Box not found: {0}")]
    BoxNotFound(String),
    
    #[error("Bead not found: {0}")]
    BeadNotFound(String),
    
    #[error("Invalid archive: {0}")]
    InvalidArchive(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Invalid bead name: {0}")]
    InvalidBeadName(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("ZIP error: {0}")]
    ZipError(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, BeadError>;