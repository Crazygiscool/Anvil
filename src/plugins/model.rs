use crate::discovery::model::Diagnostic;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub api_version: u32,
    pub library: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
}

fn default_entrypoint() -> String {
    "anvil_entry".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginReport {
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub path: PathBuf,
    pub library: PathBuf,
    pub exports: Vec<String>,
    pub capabilities: Vec<String>,
    pub invoked: bool,
    pub entry_result: Option<i64>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PluginCatalog {
    pub api_version: u32,
    pub plugins: Vec<PluginReport>,
    pub diagnostics: Vec<Diagnostic>,
}
