use crate::discovery::model::Diagnostic;
use crate::platform::{CommandRunner, TextReader};
use crate::runtime::container::{ContainerClient, ContainerClients};
use crate::runtime::model::{
    ContainerRuntimeReport, InitReport, RuntimeInspection, ServiceSummary,
};
use crate::runtime::provider::ProviderIntegrationReport;
use crate::runtime::provider::inspect_provider_integration;
use crate::storage::inspect::find_binary;
use std::path::Path;

pub trait RuntimeBackend: Send + Sync {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection;
}

pub struct InitBackend;

impl RuntimeBackend for InitBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
        inspect_init(reader, runner)
    }
}

/// Inspect the host, using the container API clients for the container
/// runtimes.
///
/// The clients return structured data, so the container list does not have to
/// be parsed out of command line output any more. The init system and the
/// provider integration are still read through the command line, because
/// they have no API.
pub fn inspect_runtime_with_clients(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    clients: &ContainerClients,
) -> RuntimeInspection {
    let mut report = RuntimeInspection::default();

    for client in clients.clients() {
        let (runtime_report, diagnostics) = inspect_container_client(client.as_ref());
        report.container_runtimes.push(runtime_report);
        report.diagnostics.extend(diagnostics);
    }

    // A runtime that is not installed at all is worth a diagnostic, so the
    // report does not look like a host with an empty container list.
    for runtime in ["docker", "podman"] {
        if clients.get(runtime).is_some() {
            continue;
        }
        if find_binary(reader, runtime).is_none() {
            push_diagnostic(
                &mut report.diagnostics,
                runtime,
                "binary not found".to_string(),
            );
        }
    }

    let init = InitBackend.inspect(reader, runner);
    report.init = init.init;
    report.diagnostics.extend(init.diagnostics);

    report.provider_integration = inspect_provider_integration(reader, runner, clients);
    report
}

/// Inspect one container runtime through its client.
fn inspect_container_client(
    client: &dyn ContainerClient,
) -> (ContainerRuntimeReport, Vec<Diagnostic>) {
    let mut report = ContainerRuntimeReport {
        name: client.runtime_name().to_string(),
        socket: client.socket_path().unwrap_or_default().to_string(),
        binary: client.binary_path().map(str::to_string),
        daemon_reachable: false,
        daemon_status: None,
        containers: Vec::new(),
    };
    let mut diagnostics = Vec::new();

    match client.list_containers() {
        Ok(containers) => {
            report.daemon_reachable = true;
            report.daemon_status = client.daemon_status();
            report.containers = containers;
        }
        Err(error) => {
            let message = if client.is_reachable() {
                error.to_string()
            } else {
                format!("daemon not reachable: {error}")
            };
            push_diagnostic(&mut diagnostics, client.runtime_name(), message);
        }
    }

    (report, diagnostics)
}

fn inspect_init(reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
    let systemd_markers = [
        Path::new("/run/systemd/system"),
        Path::new("/usr/lib/systemd"),
    ];
    let openrc_markers = [
        Path::new("/run/openrc"),
        Path::new("/etc/init.d"),
        Path::new("/sbin/openrc-run"),
    ];

    if systemd_markers.iter().any(|path| reader.exists(path)) {
        let binary = find_binary(reader, "systemctl");
        let service_manager_status = binary
            .as_ref()
            .and_then(|program| runner.run(program, &["is-system-running"]).ok())
            .map(first_output);
        let services = binary
            .as_ref()
            .and_then(|program| {
                runner
                    .run(
                        program,
                        &[
                            "list-units",
                            "--type=service",
                            "--all",
                            "--plain",
                            "--no-legend",
                        ],
                    )
                    .ok()
            })
            .map(|output| parse_systemd_services(&output.stdout))
            .unwrap_or_default();

        return RuntimeInspection {
            container_runtimes: Vec::new(),
            init: InitReport {
                kind: "systemd".to_string(),
                detected: true,
                service_manager_status,
                service_count: services.len(),
                services,
            },
            diagnostics: Vec::new(),
            provider_integration: ProviderIntegrationReport::default(),
        };
    }

    if openrc_markers.iter().any(|path| reader.exists(path)) {
        let binary = find_binary(reader, "rc-status");
        let service_manager_status = binary
            .as_ref()
            .and_then(|program| runner.run(program, &["--runlevel"]).ok())
            .map(first_output);
        let services = binary
            .as_ref()
            .and_then(|program| runner.run(program, &["--all"]).ok())
            .map(|output| parse_openrc_services(&output.stdout))
            .unwrap_or_default();

        return RuntimeInspection {
            container_runtimes: Vec::new(),
            init: InitReport {
                kind: "openrc".to_string(),
                detected: true,
                service_manager_status,
                service_count: services.len(),
                services,
            },
            diagnostics: Vec::new(),
            provider_integration: ProviderIntegrationReport::default(),
        };
    }

    RuntimeInspection {
        container_runtimes: Vec::new(),
        init: InitReport {
            kind: "unknown".to_string(),
            detected: false,
            service_manager_status: None,
            service_count: 0,
            services: Vec::new(),
        },
        diagnostics: Vec::new(),
        provider_integration: ProviderIntegrationReport::default(),
    }
}

/// Read the services out of `systemctl list-units`.
///
/// The plain output is `UNIT LOAD ACTIVE SUB DESCRIPTION`, so the unit name
/// is the first field and the active state the third. A unit that systemd
/// reports as `active` is running, even when its process has exited — that
/// is what systemd means by the state.
fn parse_systemd_services(output: &str) -> Vec<ServiceSummary> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("UNIT") && !line.starts_with("Swap:"))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.to_string();
            let _load = fields.next()?;
            let active = fields.next()?;
            let state = fields.next().unwrap_or(active);
            Some(ServiceSummary {
                name,
                state: state.to_string(),
                running: active == "active",
            })
        })
        .collect()
}

/// Read the services out of `rc-status --all`.
///
/// The output is one service per line, as `name | state`, where the state
/// is `started`, `stopped` or `crashed`.
fn parse_openrc_services(output: &str) -> Vec<ServiceSummary> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, state) = line.split_once('|')?;
            let name = name.trim();
            let state = state.trim();
            if name.is_empty() || state.is_empty() {
                return None;
            }
            Some(ServiceSummary {
                name: name.to_string(),
                state: state.to_string(),
                running: state == "started",
            })
        })
        .collect()
}

pub fn first_output(output: crate::platform::CommandOutput) -> String {
    if !output.stdout.is_empty() {
        output.stdout
    } else {
        output.stderr
    }
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, source: &str, error: String) {
    diagnostics.push(Diagnostic {
        source: source.to_string(),
        error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::container::{ContainerClients, MockContainerClient};
    use crate::runtime::model::ContainerSummary;
    use std::path::Path;
    use std::sync::Arc;

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

    /// A runner that answers nothing.
    struct EmptyRunner;

    impl CommandRunner for EmptyRunner {
        fn run(
            &self,
            _program: &Path,
            _args: &[&str],
        ) -> Result<crate::platform::CommandOutput, std::io::Error> {
            Err(std::io::Error::other("no command"))
        }
    }

    fn container(name: &str) -> ContainerSummary {
        ContainerSummary {
            id: "abc123".to_string(),
            name: name.to_string(),
            image: "nginx:latest".to_string(),
            status: "running".to_string(),
        }
    }

    /// The systemd output is read as `UNIT LOAD ACTIVE SUB DESCRIPTION`.
    #[test]
    fn the_systemd_service_list_is_read_from_the_daemon() {
        let output = "\
sshd.service loaded active running OpenBSD Secure Shell server
cron.service loaded inactive dead Regular background program processing daemon
user@1000.service loaded active exited User Manager for UID 1000
";

        let services = parse_systemd_services(output);

        assert_eq!(services.len(), 3);
        assert_eq!(services[0].name, "sshd.service");
        assert_eq!(services[0].state, "running");
        assert!(services[0].running);
        assert_eq!(services[1].state, "dead");
        assert!(!services[1].running);
        assert!(
            services[2].running,
            "a service that is active but has exited is still active"
        );
    }

    /// The OpenRC output is read as `name | state`.
    #[test]
    fn the_openrc_service_list_is_read_from_the_daemon() {
        let output = "\
OpenRC 0.42.2 is running
 sshd                | started
 mount-ro-root       | stopped
 crond               | crashed
";

        let services = parse_openrc_services(output);

        assert_eq!(services.len(), 3);
        assert_eq!(services[0].name, "sshd");
        assert_eq!(services[0].state, "started");
        assert!(services[0].running);
        assert!(!services[1].running);
        assert_eq!(services[2].state, "crashed");
        assert!(!services[2].running);
    }

    /// Output that is not a service list yields no services.
    #[test]
    fn an_empty_service_list_yields_no_services() {
        assert!(parse_systemd_services("").is_empty());
        assert!(parse_systemd_services("UNIT LOAD ACTIVE SUB DESCRIPTION\n").is_empty());
        assert!(parse_openrc_services("").is_empty());
        assert!(parse_openrc_services("no services here\n").is_empty());
    }

    #[test]
    fn the_inspection_reports_the_containers_of_the_client() {
        let mock = MockContainerClient::new("docker");
        mock.set_containers(vec![container("web"), container("db")]);
        let clients = ContainerClients::new(vec![Arc::new(mock)]);

        let report = inspect_runtime_with_clients(&EmptyReader, &EmptyRunner, &clients);

        assert_eq!(report.container_runtimes.len(), 1);
        let runtime = &report.container_runtimes[0];
        assert_eq!(runtime.name, "docker");
        assert!(runtime.daemon_reachable);
        assert_eq!(runtime.daemon_status.as_deref(), Some("mock"));
        assert_eq!(runtime.containers.len(), 2);
        assert_eq!(runtime.containers[0].name, "web");
    }

    #[test]
    fn an_unreachable_daemon_is_reported_as_a_diagnostic() {
        let mock = MockContainerClient::new("podman");
        mock.set_unreachable();
        let clients = ContainerClients::new(vec![Arc::new(mock)]);

        let report = inspect_runtime_with_clients(&EmptyReader, &EmptyRunner, &clients);

        assert_eq!(report.container_runtimes.len(), 1);
        assert!(!report.container_runtimes[0].daemon_reachable);
        assert!(report.container_runtimes[0].containers.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.source == "podman"
                    && diagnostic.error.contains("not reachable")),
            "the unreachable daemon should be reported: {:#?}",
            report.diagnostics
        );
    }
}
