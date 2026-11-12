use crate::discovery::model::Diagnostic;
use crate::platform::{CommandRunner, TextReader};
use crate::runtime::container::{
    ContainerClient, ContainerClients, docker_sockets, podman_sockets,
};
use crate::runtime::inspect::first_output;
use crate::storage::inspect::find_binary;
use serde::Serialize;
use std::path::{Path, PathBuf};

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
    fn inspect(
        &self,
        reader: &dyn TextReader,
        runner: &dyn CommandRunner,
        clients: &ContainerClients,
    ) -> ProviderIntegration;
}

pub struct SystemdProvider;
pub struct OpenrcProvider;
pub struct DockerProvider;
pub struct PodmanProvider;

impl ProviderIntegrationBackend for SystemdProvider {
    fn kind(&self) -> &'static str {
        "systemd"
    }

    fn inspect(
        &self,
        reader: &dyn TextReader,
        runner: &dyn CommandRunner,
        _clients: &ContainerClients,
    ) -> ProviderIntegration {
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

    fn inspect(
        &self,
        reader: &dyn TextReader,
        runner: &dyn CommandRunner,
        _clients: &ContainerClients,
    ) -> ProviderIntegration {
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

    /// The Docker provider reads its status from the cached container
    /// client.
    ///
    /// The client already knows the daemon version, because it asked once
    /// when it was built. Probing `docker version` again on every refresh
    /// would start a subprocess per tick for no new information.
    fn inspect(
        &self,
        reader: &dyn TextReader,
        _runner: &dyn CommandRunner,
        clients: &ContainerClients,
    ) -> ProviderIntegration {
        container_provider("docker", reader, clients.get("docker").map(|c| c.as_ref()))
    }
}

impl ProviderIntegrationBackend for PodmanProvider {
    fn kind(&self) -> &'static str {
        "podman"
    }

    /// The Podman provider reads its status from the cached container
    /// client, for the same reason as the Docker provider.
    fn inspect(
        &self,
        reader: &dyn TextReader,
        _runner: &dyn CommandRunner,
        clients: &ContainerClients,
    ) -> ProviderIntegration {
        container_provider("podman", reader, clients.get("podman").map(|c| c.as_ref()))
    }
}

/// The provider entry for a container runtime.
///
/// `available` says whether the runtime is present on the host at all; the
/// status is the daemon version the client reported, or nothing when the
/// daemon did not answer.
fn container_provider(
    kind: &str,
    reader: &dyn TextReader,
    client: Option<&dyn ContainerClient>,
) -> ProviderIntegration {
    let sockets: Vec<PathBuf> = if kind == "docker" {
        docker_sockets()
    } else {
        podman_sockets()
    };

    let host_binary = find_binary(reader, kind);
    let socket_present = sockets.iter().any(|socket| reader.exists(socket));
    let available = socket_present || host_binary.is_some();

    let status = client.and_then(|client| client.daemon_status());

    ProviderIntegration {
        kind: kind.to_string(),
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

pub fn inspect_provider_integration(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    clients: &ContainerClients,
) -> ProviderIntegrationReport {
    let backends: Vec<Box<dyn ProviderIntegrationBackend>> = vec![
        Box::new(SystemdProvider),
        Box::new(OpenrcProvider),
        Box::new(DockerProvider),
        Box::new(PodmanProvider),
    ];
    let mut providers = backends
        .iter()
        .map(|backend| backend.inspect(reader, runner, clients))
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.kind.cmp(&right.kind));
    ProviderIntegrationReport {
        providers,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::container::{ContainerClients, MockContainerClient};
    use std::sync::{Arc, Mutex};

    /// A reader that finds nothing on the host.
    struct EmptyReader;

    impl TextReader for EmptyReader {
        fn read_to_string(&self, _path: &Path) -> Result<String, std::io::Error> {
            Err(std::io::Error::other("missing fixture"))
        }

        fn read_dir_names(&self, _path: &Path) -> Result<Vec<String>, std::io::Error> {
            Ok(Vec::new())
        }

        fn exists(&self, _path: &Path) -> bool {
            false
        }
    }

    /// A runner that records every command it is asked to run.
    struct RecordingRunner {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(
            &self,
            program: &Path,
            args: &[&str],
        ) -> Result<crate::platform::CommandOutput, std::io::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{} {}", program.display(), args.join(" ")));
            Ok(crate::platform::CommandOutput {
                status: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn provider<'a>(report: &'a ProviderIntegrationReport, kind: &str) -> &'a ProviderIntegration {
        report
            .providers
            .iter()
            .find(|provider| provider.kind == kind)
            .expect("the provider is in the report")
    }

    /// The daemon version comes from the cached client, so a refresh does
    /// not run `docker version` again.
    #[test]
    fn the_container_provider_does_not_probe_the_daemon_version() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = RecordingRunner {
            calls: Arc::clone(&calls),
        };
        let mock = MockContainerClient::new("docker");
        let clients = ContainerClients::new(vec![Arc::new(mock)]);

        let report = inspect_provider_integration(&EmptyReader, &runner, &clients);

        let docker = provider(&report, "docker");
        assert_eq!(docker.status.as_deref(), Some("mock"));
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.contains("version")),
            "the daemon version is not probed again: {:#?}",
            calls.lock().unwrap()
        );
    }

    /// A runtime that is not installed at all is reported as unavailable.
    #[test]
    fn a_runtime_without_a_client_is_not_available() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = RecordingRunner {
            calls: Arc::clone(&calls),
        };

        let report =
            inspect_provider_integration(&EmptyReader, &runner, &ContainerClients::empty());

        assert!(!provider(&report, "docker").available);
        assert!(provider(&report, "docker").status.is_none());
        assert!(!provider(&report, "podman").available);
    }

    /// The init providers still report through the command line.
    #[test]
    fn the_init_providers_are_reported() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = RecordingRunner {
            calls: Arc::clone(&calls),
        };

        let report =
            inspect_provider_integration(&EmptyReader, &runner, &ContainerClients::empty());

        assert!(report.providers.iter().any(|p| p.kind == "systemd"));
        assert!(report.providers.iter().any(|p| p.kind == "openrc"));
    }
}
