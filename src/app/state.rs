use crate::access::AccessContext;
use crate::config::Config;
use crate::discovery::DiscoveryReport;
use crate::plugins::PluginCatalog;
use crate::runtime::RuntimeInspection;
use crate::storage::StorageInspection;
use crate::telemetry::TelemetrySample;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Config,
    pub access: AccessContext,
    pub discovery: Option<DiscoveryReport>,
    pub storage: Option<StorageInspection>,
    pub runtime: Option<RuntimeInspection>,
    pub plugins: Option<PluginCatalog>,
    pub telemetry: Option<TelemetrySample>,
    pub manifest_paths: Vec<PathBuf>,
    pub running: bool,
}

impl AppState {
    pub fn bootstrap(config: Config, access: AccessContext, manifest_paths: Vec<PathBuf>) -> Self {
        Self {
            config,
            access,
            discovery: None,
            storage: None,
            runtime: None,
            plugins: None,
            telemetry: None,
            manifest_paths,
            running: true,
        }
    }
}
