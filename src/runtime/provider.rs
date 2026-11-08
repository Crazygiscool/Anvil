use crate::discovery::model::Diagnostic;
use crate::platform::{CommandRunner, TextReader};
use crate::runtime::inspect::first_output;
use crate::storage::inspect::find_binary;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ProviderIntegration {
    pub kind: String,
    pub available: bool,
    pub host_binary: Option<String>,
    pub status: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ProviderIntegrationReport {
    pub providers: Vec<ProviderIntegration>,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait ProviderIntegrationBackend: Send + Sync {
    fn kind(&self) -> &'static str;
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> ProviderIntegration;
}

pub struct SystemdProvider;
pub struct OpenrcProvider;
pub struct DockerProvider;
pub struct PodmanProvider;

impl ProviderIntegrationBackend for SystemdProvider {
    fn kind(&self) -> &'static str {
        "systemd"
    }

    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> ProviderIntegration {
        let markers = [
            Path::new("/run/systemd/system"),
            Path::new("/usr/lib/systemd"),
        ];
        let available = markers.iter().any(|path| reader.exists(path));
        let host_binary = find_binary(reader, "systemctl");
        let status = host_binary.as_ref().and_then(|program| {
            runner
                .run(program, &["is-system-running"])
                .ok()
                .map(first_output)
        });
        ProviderIntegration {
            kind: self.kind().to_string(),
            available,
            host_binary: host_binary.map(|path| path.display().to_string()),
            status,
            capabilities: if available {
                vec![
                    "inspect".to_string(),
                    "enable".to_string(),
                    "start".to_string(),
                    "restart".to_string(),
                ]
            } else {
                Vec::new()
            },
        }
    }
}

impl ProviderIntegrationBackend for OpenrcProvider {
    fn kind(&self) -> &'static str {
        "openrc"
    }

    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> ProviderIntegration {
        let markers = [
            Path::new("/run/openrc"),
            Path::new("/etc/init.d"),
            Path::new("/sbin/openrc-run"),
        ];
        let available = markers.iter().any(|path| reader.exists(path));
        let host_binary = find_binary(reader, "rc-service");
        let status = host_binary
            .as_ref()
            .and_then(|program| runner.run(program, &["--runlevel"]).ok().map(first_output));
        ProviderIntegration {
            kind: self.kind().to_string(),
            available,
            host_binary: host_binary.map(|path| path.display().to_string()),
            status,
            capabilities: if available {
                vec![
                    "inspect".to_string(),
                    "start".to_string(),
                    "stop".to_string(),
                    "restart".to_string(),
                ]
            } else {
                Vec::new()
            },
        }
    }
}

impl ProviderIntegrationBackend for DockerProvider {
    fn kind(&self) -> &'static str {
        "docker"
    }

    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> ProviderIntegration {
        let sockets = [
            Path::new("/var/run/docker.sock"),
            Path::new("/run/docker.sock"),
        ];
        let available = sockets.iter().any(|socket| reader.exists(socket));
        let host_binary = find_binary(reader, "docker");
        let status = host_binary.as_ref().and_then(|program| {
            runner
                .run(program, &["version", "--format", "{{.Server.Version}}"])
                .ok()
                .map(first_output)
        });
        ProviderIntegration {
            kind: self.kind().to_string(),
            available,
            host_binary: host_binary.map(|path| path.display().to_string()),
            status,
            capabilities: if available {
                vec![
                    "inspect".to_string(),
                    "start".to_string(),
                    "stop".to_string(),
                    "restart".to_string(),
                ]
            } else {
                Vec::new()
            },
        }
    }
}

impl ProviderIntegrationBackend for PodmanProvider {
    fn kind(&self) -> &'static str {
        "podman"
    }

    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> ProviderIntegration {
        let socket = Path::new("/run/podman/podman.sock");
        let available = reader.exists(socket);
        let host_binary = find_binary(reader, "podman");
        let status = host_binary.as_ref().and_then(|program| {
            runner
                .run(program, &["version", "--format", "{{.Server.Version}}"])
                .ok()
                .map(first_output)
        });
        ProviderIntegration {
            kind: self.kind().to_string(),
            available,
            host_binary: host_binary.map(|path| path.display().to_string()),
            status,
            capabilities: if available {
                vec![
                    "inspect".to_string(),
                    "start".to_string(),
                    "stop".to_string(),
                    "restart".to_string(),
                ]
            } else {
                Vec::new()
            },
        }
    }
}

pub fn inspect_provider_integration(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
) -> ProviderIntegrationReport {
    let backends: Vec<Box<dyn ProviderIntegrationBackend>> = vec![
        Box::new(SystemdProvider),
        Box::new(OpenrcProvider),
        Box::new(DockerProvider),
        Box::new(PodmanProvider),
    ];
    let mut providers = backends
        .iter()
        .map(|backend| backend.inspect(reader, runner))
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.kind.cmp(&right.kind));
    ProviderIntegrationReport {
        providers,
        diagnostics: Vec::new(),
    }
}
