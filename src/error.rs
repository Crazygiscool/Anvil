use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnvilError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("manifest {path} has version {found}, expected {expected}")]
    ManifestVersion {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    #[error("I/O error while handling {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

pub type Result<T> = std::result::Result<T, AnvilError>;

impl AnvilError {
    #[allow(dead_code)]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) | Self::ManifestVersion { .. } => 2,
            Self::Permission(_) => 3,
            Self::Io { .. } | Self::Runtime(_) => 1,
        }
    }
}
