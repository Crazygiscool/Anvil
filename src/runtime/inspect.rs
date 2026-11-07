use crate::discovery::model::Diagnostic;
use crate::platform::{CommandRunner, TextReader};
use crate::runtime::model::{
    ContainerRuntimeReport, ContainerSummary, InitReport, RuntimeInspection,
};
use crate::storage::inspect::find_binary;
use std::path::{Path, PathBuf};

pub trait RuntimeBackend: Send + Sync {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection;
}

pub struct DockerBackend;

impl RuntimeBackend for DockerBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
        inspect_container_runtime(
            reader,
            runner,
            "docker",
            &["/var/run/docker.sock", "/run/docker.sock"],
            &["version", "--format", "{{.Server.Version}}"],
        )
    }
}

pub struct PodmanBackend;

impl RuntimeBackend for PodmanBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
        inspect_container_runtime(
            reader,
            runner,
            "podman",
            &["/run/podman/podman.sock"],
            &["version", "--format", "{{.Server.Version}}"],
        )
    }
}

pub struct InitBackend;

impl RuntimeBackend for InitBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
        inspect_init(reader, runner)
    }
}

pub struct RuntimeCoordinator {
    pub backends: Vec<Box<dyn RuntimeBackend>>,
}

impl RuntimeCoordinator {
    pub fn new() -> Self {
        Self {
            backends: vec![
                Box::new(DockerBackend),
                Box::new(PodmanBackend),
                Box::new(InitBackend),
            ],
        }
    }
}

impl RuntimeBackend for RuntimeCoordinator {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
        let mut report = RuntimeInspection::default();

        for backend in &self.backends {
            let partial = backend.inspect(reader, runner);
            report.container_runtimes.extend(partial.container_runtimes);
            report.init = if report.init.detected {
                report.init.clone()
            } else {
                partial.init
            };
            report.diagnostics.extend(partial.diagnostics);
        }

        report
    }
}

pub fn inspect_runtime(reader: &dyn TextReader, runner: &dyn CommandRunner) -> RuntimeInspection {
    RuntimeCoordinator::new().inspect(reader, runner)
}

fn inspect_container_runtime(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    name: &str,
    socket_candidates: &[&str],
    version_args: &[&str],
) -> RuntimeInspection {
    let mut report = RuntimeInspection::default();
    let binary = find_binary(reader, name);

    for socket in socket_candidates {
        let socket_path = PathBuf::from(socket);
        if !reader.exists(&socket_path) {
            continue;
        }

        let mut runtime = ContainerRuntimeReport {
            name: name.to_string(),
            socket: socket.to_string(),
            binary: binary.as_ref().map(|path| path.display().to_string()),
            daemon_reachable: false,
            daemon_status: None,
            containers: Vec::new(),
        };

        if let Some(program) = binary.as_ref() {
            if let Ok(output) = runner.run(program, version_args) {
                runtime.daemon_reachable = true;
                runtime.daemon_status = Some(first_output(output));
            } else {
                push_diagnostic(
                    &mut report.diagnostics,
                    name,
                    "daemon not reachable".to_string(),
                );
                continue;
            }

            if let Ok(output) = runner.run(
                program,
                &[
                    "ps",
                    "--format",
                    "{{.ID}} {{.Names}} {{.Image}} {{.Status}}",
                ],
            ) && output.status.unwrap_or(1) == 0
            {
                for line in output.stdout.lines() {
                    if let Some(container) = parse_container_line(line) {
                        runtime.containers.push(container);
                    }
                }
            }
        }

        report.container_runtimes.push(runtime);
    }

    if binary.is_none() {
        push_diagnostic(
            &mut report.diagnostics,
            name,
            "binary not found".to_string(),
        );
    }

    report
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
        let service_count = binary
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
            .map(|output| count_service_lines(&output.stdout))
            .unwrap_or(0);

        return RuntimeInspection {
            container_runtimes: Vec::new(),
            init: InitReport {
                kind: "systemd".to_string(),
                detected: true,
                service_manager_status,
                service_count,
            },
            diagnostics: Vec::new(),
        };
    }

    if openrc_markers.iter().any(|path| reader.exists(path)) {
        let binary = find_binary(reader, "rc-status");
        let service_manager_status = binary
            .as_ref()
            .and_then(|program| runner.run(program, &["--runlevel"]).ok())
            .map(first_output);
        let service_count = binary
            .as_ref()
            .and_then(|program| runner.run(program, &["--all"]).ok())
            .map(|output| count_service_lines(&output.stdout))
            .unwrap_or(0);

        return RuntimeInspection {
            container_runtimes: Vec::new(),
            init: InitReport {
                kind: "openrc".to_string(),
                detected: true,
                service_manager_status,
                service_count,
            },
            diagnostics: Vec::new(),
        };
    }

    RuntimeInspection {
        container_runtimes: Vec::new(),
        init: InitReport {
            kind: "unknown".to_string(),
            detected: false,
            service_manager_status: None,
            service_count: 0,
        },
        diagnostics: Vec::new(),
    }
}

fn parse_container_line(line: &str) -> Option<ContainerSummary> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }

    let status = fields[3..].join(" ");
    Some(ContainerSummary {
        id: fields[0].to_string(),
        name: fields[1].to_string(),
        image: fields[2].to_string(),
        status,
    })
}

fn count_service_lines(output: &str) -> usize {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("UNIT") && !line.starts_with("Swap:"))
        .count()
}

fn first_output(output: crate::platform::CommandOutput) -> String {
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
