use crate::discovery::model::Diagnostic;
use crate::runtime::provider::ProviderIntegrationReport;
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

/// One service of the init system.
///
/// The state is the one the init system itself reports, so it is never a
/// guess: systemd calls it `active` or `inactive`, OpenRC calls it `started`
/// or `stopped`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceSummary {
    pub name: String,
    pub state: String,
    /// Whether the service is currently running.
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct InitReport {
    pub kind: String,
    pub detected: bool,
    pub service_manager_status: Option<String>,
    pub service_count: usize,
    /// The services the init system reports.
    pub services: Vec<ServiceSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuntimeInspection {
    pub container_runtimes: Vec<ContainerRuntimeReport>,
    pub init: InitReport,
    pub provider_integration: ProviderIntegrationReport,
    pub diagnostics: Vec<Diagnostic>,
}
