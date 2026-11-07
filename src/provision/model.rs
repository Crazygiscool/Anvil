use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProvisioningManifest {
    pub version: u32,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub storage: Vec<StorageResource>,
    #[serde(default)]
    pub services: Vec<ServiceResource>,
    #[serde(default)]
    pub containers: Vec<ContainerResource>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StorageResource {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_storage_kind")]
    pub kind: String,
    #[serde(default)]
    pub pool: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_storage_kind() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ServiceResource {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ContainerResource {
    pub name: String,
    pub image: String,
    #[serde(default = "default_container_runtime")]
    pub runtime: String,
}

fn default_container_runtime() -> String {
    "docker".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestJob {
    pub id: String,
    pub path: PathBuf,
    pub manifest: ProvisioningManifest,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionAction {
    pub domain: &'static str,
    pub resource: String,
    pub summary: String,
    pub command: Option<CommandPlan>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProvisionPlan {
    pub actions: Vec<ProvisionAction>,
    pub error_count: usize,
}
