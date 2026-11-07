use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub refresh_rate_ms: u64,
    pub manifest_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SUPPORTED_VERSION,
            refresh_rate_ms: 1000,
            manifest_path: PathBuf::from("/etc/anvil/manifest.toml"),
        }
    }
}
