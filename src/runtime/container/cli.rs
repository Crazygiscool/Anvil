//! A container client that shells out to the runtime's command line.
//!
//! This is the fallback for daemons that do not expose a socket, such as
//! rootless Podman without a service socket. It reuses the same command
//! line the mutation stack builds, so the two transports agree on what a
//! start, a stop or a create means.

use crate::error::{AnvilError, Result};
use crate::platform::CommandRunner;
use crate::runtime::container::{ContainerClient, ContainerCreateRequest, ContainerDetails};
use crate::runtime::model::ContainerSummary;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A client that drives the `docker` or `podman` command line.
pub struct CliContainerClient {
    runtime: String,
    binary: String,
    runner: Arc<dyn CommandRunner + Send + Sync>,
    state: Mutex<DaemonState>,
}

/// What the client knows about the daemon.
#[derive(Debug, Clone, Default)]
struct DaemonState {
    reachable: bool,
    status: Option<String>,
}

impl CliContainerClient {
    /// Build a client for a runtime binary.
    pub fn new(runtime: &str, binary: &str, runner: Arc<dyn CommandRunner + Send + Sync>) -> Self {
        Self {
            runtime: runtime.to_string(),
            binary: binary.to_string(),
            runner,
            state: Mutex::new(DaemonState::default()),
        }
    }

    /// Run a command line against the runtime.
    fn run(&self, args: &[&str]) -> Result<crate::platform::CommandOutput> {
        let output = self
            .runner
            .run(Path::new(&self.binary), args)
            .map_err(|error| AnvilError::Io {
                path: PathBuf::from(&self.binary),
                source: error,
            })?;

        if output.status == Some(0) {
            if let Ok(mut guard) = self.state.lock() {
                guard.reachable = true;
            }
            return Ok(output);
        }

        if let Ok(mut guard) = self.state.lock() {
            guard.reachable = false;
        }
        Err(AnvilError::CommandExecution {
            program: self.binary.clone(),
            status: output.status.unwrap_or(-1),
            stderr: if output.stderr.is_empty() {
                output.stdout
            } else {
                output.stderr
            },
        })
    }

    /// Probe the daemon once and remember what it said.
    ///
    /// The probe is cached, so a refresh does not ask the daemon for its
    /// version over and over.
    pub fn probe(&self) -> Result<()> {
        if self.daemon_status().is_some() {
            return Ok(());
        }

        let output = self.run(&["version", "--format", "{{.Server.Version}}"])?;
        let status = output.stdout.trim().to_string();
        if let Ok(mut guard) = self.state.lock() {
            guard.status = Some(status);
        }
        Ok(())
    }
}

impl ContainerClient for CliContainerClient {
    fn runtime_name(&self) -> &str {
        &self.runtime
    }

    fn is_reachable(&self) -> bool {
        self.state
            .lock()
            .map(|guard| guard.reachable)
            .unwrap_or(false)
    }

    fn daemon_status(&self) -> Option<String> {
        self.state
            .lock()
            .ok()
            .and_then(|guard| guard.status.clone())
    }

    fn binary_path(&self) -> Option<&str> {
        Some(&self.binary)
    }

    fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
        let output = self.run(&[
            "ps",
            "--all",
            "--format",
            "{{.ID}} {{.Names}} {{.Image}} {{.Status}}",
        ])?;

        Ok(output
            .stdout
            .lines()
            .filter_map(parse_container_line)
            .collect())
    }

    fn inspect(&self, name: &str) -> Result<ContainerDetails> {
        let output = self.run(&[
            "inspect",
            "--format",
            "{{.Id}} {{.Name}} {{.Config.Image}} {{.State.Status}} {{.State.Running}}",
            name,
        ])?;

        let line = output.stdout.lines().next().unwrap_or_default();
        let mut fields = line.split_whitespace();
        let id = fields.next().unwrap_or_default().to_string();
        let raw_name = fields.next().unwrap_or_default().to_string();
        let image = fields.next().unwrap_or_default().to_string();
        let status = fields.next().unwrap_or("unknown").to_string();
        let running = fields.next().unwrap_or("false") == "true";

        Ok(ContainerDetails {
            id,
            name: raw_name.trim_start_matches('/').to_string(),
            image,
            status,
            running,
            ports: Vec::new(),
        })
    }

    fn start(&self, name: &str) -> Result<()> {
        self.run(&["start", name]).map(|_| ())
    }

    fn stop(&self, name: &str) -> Result<()> {
        self.run(&["stop", name]).map(|_| ())
    }

    fn restart(&self, name: &str) -> Result<()> {
        self.run(&["restart", name]).map(|_| ())
    }

    fn remove(&self, name: &str) -> Result<()> {
        self.run(&["rm", "-f", name]).map(|_| ())
    }

    fn pull(&self, image: &str) -> Result<()> {
        self.run(&["pull", image]).map(|_| ())
    }

    fn create(&self, request: &ContainerCreateRequest) -> Result<String> {
        let mut args: Vec<String> = vec!["run".to_string(), "-d".to_string()];
        args.push("--name".to_string());
        args.push(request.name.clone());

        let policy = request.restart_policy.trim();
        if !policy.is_empty() && policy != "no" {
            args.push("--restart".to_string());
            args.push(policy.to_string());
        }

        for port in &request.ports {
            args.push("-p".to_string());
            args.push(format!(
                "{}:{}/{}",
                port.host, port.container, port.protocol
            ));
        }

        for volume in &request.volumes {
            args.push("-v".to_string());
            args.push(format!("{}:{}", volume.source, volume.target));
        }

        for (key, value) in &request.env {
            args.push("-e".to_string());
            args.push(format!("{key}={value}"));
        }

        args.push(request.image.clone());

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self.run(&borrowed)?;

        Ok(output.stdout.trim().to_string())
    }
}

/// Parse one line of `docker ps` output.
fn parse_container_line(line: &str) -> Option<ContainerSummary> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }

    let status = fields[3..].join(" ");
    Some(ContainerSummary {
        id: fields[0].to_string(),
        name: fields[1].trim_start_matches('/').to_string(),
        image: fields[2].to_string(),
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CommandOutput;
    use std::sync::Mutex;

    /// Records the command lines the client runs.
    struct RecordingRunner {
        calls: Arc<Mutex<Vec<String>>>,
        output: CommandOutput,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&self, program: &std::path::Path, args: &[&str]) -> std::io::Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{} {}", program.display(), args.join(" ")));
            Ok(self.output.clone())
        }
    }

    fn client(runner: Arc<RecordingRunner>) -> CliContainerClient {
        CliContainerClient::new("docker", "/usr/bin/docker", runner)
    }

    fn output(stdout: &str) -> CommandOutput {
        CommandOutput {
            status: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    #[test]
    fn listing_containers_parses_the_command_line_output() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
            output: output("abc123 web nginx:latest Up 2 minutes\n"),
        });
        let client = client(runner);

        let containers = client.list_containers().unwrap();

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].id, "abc123");
        assert_eq!(containers[0].name, "web");
        assert_eq!(containers[0].status, "Up 2 minutes");
        assert!(client.is_reachable());
    }

    #[test]
    fn a_failing_command_marks_the_daemon_unreachable() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
            output: CommandOutput {
                status: Some(1),
                stdout: String::new(),
                stderr: "daemon is not running".to_string(),
            },
        });
        let client = client(runner);

        assert!(client.list_containers().is_err());
        assert!(!client.is_reachable());
    }

    #[test]
    fn creating_a_container_builds_the_run_command() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
            output: output("container-id\n"),
        });
        let client = client(runner);

        let id = client
            .create(&ContainerCreateRequest {
                name: "web".to_string(),
                image: "nginx:alpine".to_string(),
                ports: vec![crate::runtime::mutation::PortMapping {
                    host: 8080,
                    container: 80,
                    protocol: "tcp".to_string(),
                }],
                volumes: Vec::new(),
                env: vec![("TZ".to_string(), "UTC".to_string())],
                restart_policy: "unless-stopped".to_string(),
            })
            .unwrap();

        assert_eq!(id, "container-id");
        let recorded = calls.lock().unwrap().clone();
        assert_eq!(
            recorded[0],
            "/usr/bin/docker run -d --name web --restart unless-stopped -p 8080:80/tcp -e TZ=UTC nginx:alpine"
        );
    }

    #[test]
    fn the_daemon_probe_is_cached() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
            output: output("27.1.1\n"),
        });
        let client = client(runner);

        client.probe().unwrap();
        client.probe().unwrap();

        assert_eq!(client.daemon_status().as_deref(), Some("27.1.1"));
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "the daemon version is only probed once"
        );
    }
}
