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
