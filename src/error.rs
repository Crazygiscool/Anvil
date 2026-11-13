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
    #[error("command {program} failed with status {status}: {stderr}")]
    CommandExecution {
        program: String,
        status: i32,
        stderr: String,
    },
    #[error("runtime error: {0}")]
    Runtime(String),
}

pub type Result<T> = std::result::Result<T, AnvilError>;

impl AnvilError {
    /// The exit code the CLI reports for this error.
    ///
    /// The codes say what has to change: a configuration error (2) and a
    /// permission error (3) need different fixes than a runtime failure
    /// (1).
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::ManifestVersion { .. } => 2,
            Self::Permission(_) => 3,
            Self::Io { .. } | Self::Runtime(_) | Self::CommandExecution { .. } => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exit code tells a script what kind of problem it has to fix.
    #[test]
    fn the_exit_code_says_what_has_to_change() {
        let config = AnvilError::Config("broken".to_string());
        assert_eq!(config.exit_code(), 2);

        let version = AnvilError::ManifestVersion {
            path: PathBuf::from("/etc/anvil/manifest.toml"),
            found: 2,
            expected: 1,
        };
        assert_eq!(version.exit_code(), 2);

        let permission = AnvilError::Permission("read-only".to_string());
        assert_eq!(permission.exit_code(), 3);

        let io = AnvilError::Io {
            path: PathBuf::from("/etc/anvil/config.toml"),
            source: std::io::Error::other("boom"),
        };
        assert_eq!(io.exit_code(), 1);

        let runtime = AnvilError::Runtime("boom".to_string());
        assert_eq!(runtime.exit_code(), 1);

        let command = AnvilError::CommandExecution {
            program: "docker".to_string(),
            status: 1,
            stderr: "no such image".to_string(),
        };
        assert_eq!(command.exit_code(), 1);
    }
}
