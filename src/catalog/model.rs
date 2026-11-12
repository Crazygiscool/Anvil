//! Data model for the application catalog.
//!
//! The catalog is a plain data file so that users can extend or override the
//! built-in entries without rebuilding Anvil.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the catalog index format.
pub const SUPPORTED_INDEX_VERSION: u32 = 1;

fn default_runtime() -> String {
    "docker".to_string()
}

fn default_restart_policy() -> String {
    "unless-stopped".to_string()
}

fn default_protocol() -> String {
    "tcp".to_string()
}

fn default_volume_kind() -> String {
    "volume".to_string()
}

/// A catalog index: a versioned list of installable applications.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogIndex {
    pub version: u32,
    #[serde(default)]
    pub apps: Vec<CatalogEntry>,
}

impl CatalogIndex {
    pub fn new(apps: Vec<CatalogEntry>) -> Self {
        Self {
            version: SUPPORTED_INDEX_VERSION,
            apps,
        }
    }

    /// Find an entry by its identifier.
    pub fn find(&self, id: &str) -> Option<&CatalogEntry> {
        self.apps.iter().find(|entry| entry.id == id)
    }

    /// The distinct categories present in the index, in sorted order.
    pub fn categories(&self) -> Vec<String> {
        let mut categories: Vec<String> = self
            .apps
            .iter()
            .map(|entry| entry.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }
}

/// One installable application.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub category: String,
    pub image: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    #[serde(default = "default_restart_policy")]
    pub restart_policy: String,
    #[serde(default)]
    pub ports: Vec<CatalogPort>,
    #[serde(default)]
    pub volumes: Vec<CatalogVolume>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// A published port on the host.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogPort {
    pub host: u16,
    pub container: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

/// A volume attached to the container.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogVolume {
    pub source: String,
    pub target: String,
    /// `volume` for a named volume, `bind` for a host path.
    #[serde(default = "default_volume_kind")]
    pub kind: String,
}

impl CatalogVolume {
    pub fn is_bind(&self) -> bool {
        self.kind == "bind"
    }
}
