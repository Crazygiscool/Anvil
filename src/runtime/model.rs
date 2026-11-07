use crate::discovery::model::Diagnostic;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerRuntimeReport {
    pub name: String,
    pub socket: String,
    pub binary: Option<String>,
    pub daemon_reachable: bool,
    pub daemon_status: Option<String>,
    pub containers: Vec<ContainerSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct InitReport {
    pub kind: String,
    pub detected: bool,
    pub service_manager_status: Option<String>,
    pub service_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuntimeInspection {
    pub container_runtimes: Vec<ContainerRuntimeReport>,
    pub init: InitReport,
    pub diagnostics: Vec<Diagnostic>,
}
