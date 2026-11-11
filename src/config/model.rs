use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SUPPORTED_VERSION: u32 = 1;

fn default_host_refresh_rate_ms() -> u64 {
    10_000
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub refresh_rate_ms: u64,
    /// How often host state (discovery, storage, runtime) is re-collected.
    #[serde(default = "default_host_refresh_rate_ms")]
    pub host_refresh_rate_ms: u64,
    pub manifest_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SUPPORTED_VERSION,
            refresh_rate_ms: 1000,
            host_refresh_rate_ms: default_host_refresh_rate_ms(),
            manifest_path: PathBuf::from("/etc/anvil/manifest.toml"),
        }
    }
}
