use crate::access;
use crate::config::model::Config;
use crate::error::{AnvilError, Result};
use std::path::PathBuf;

pub fn load(explicit: Option<PathBuf>) -> Result<Config> {
    if let Some(path) = explicit {
        return read_validated(path);
    }

    for path in standard_config_paths() {
        if path.is_file() {
            return read_validated(path);
        }
    }

    Ok(Config::default())
}

fn read_validated(path: PathBuf) -> Result<Config> {
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            // A file the user named explicitly and that is not there is a
            // configuration mistake, not a runtime failure.
            return Err(AnvilError::Config(format!(
                "{}: no such configuration file",
                path.display()
            )));
        }
        Err(source) => return Err(AnvilError::Io { path, source }),
    };
    let config: Config = toml::from_str(&source)
        .map_err(|error| AnvilError::Config(format!("{}: {error}", path.display())))?;
    if config.version != Config::default().version {
        return Err(AnvilError::ManifestVersion {
            path,
            found: config.version,
            expected: Config::default().version,
        });
    }
    Ok(config)
}

fn standard_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if effective_uid() == 0 {
        paths.push(PathBuf::from("/etc/anvil/config.toml"));
    } else {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            paths.push(PathBuf::from(xdg).join("anvil/config.toml"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".config/anvil/config.toml"));
        }
        paths.push(PathBuf::from("/etc/anvil/config.toml"));
    }
    paths
}

fn effective_uid() -> u32 {
    access::effective_uid()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A named configuration file that is not there is a configuration
    /// mistake, so the CLI reports it with the configuration exit code.
    #[test]
    fn a_missing_named_config_file_is_a_config_error() {
        let missing = PathBuf::from("/nonexistent/anvil/config.toml");

        let error = load(Some(missing)).unwrap_err();

        assert!(
            matches!(error, AnvilError::Config(_)),
            "the error is a configuration error: {error:?}"
        );
        assert_eq!(error.exit_code(), 2);
    }

    /// A configuration file with the wrong version is rejected.
    #[test]
    fn a_config_file_with_the_wrong_version_is_rejected() {
        let path = std::env::temp_dir().join(format!("anvil-config-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "version = 99\nrefresh_rate_ms = 1000\nmanifest_path = \"/etc/anvil/manifest.toml\"\n",
        )
        .unwrap();

        let error = load(Some(path.clone())).unwrap_err();

        std::fs::remove_file(&path).ok();
        assert!(matches!(error, AnvilError::ManifestVersion { .. }));
        assert_eq!(error.exit_code(), 2);
    }

    /// A configuration file that cannot be parsed is a configuration error.
    #[test]
    fn a_config_file_that_cannot_be_parsed_is_rejected() {
        let path =
            std::env::temp_dir().join(format!("anvil-config-broken-{}.toml", std::process::id()));
        std::fs::write(&path, "this is not toml = = =\n").unwrap();

        let error = load(Some(path.clone())).unwrap_err();

        std::fs::remove_file(&path).ok();
        assert!(matches!(error, AnvilError::Config(_)));
        assert_eq!(error.exit_code(), 2);
    }
}
