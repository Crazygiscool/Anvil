//! A container client that speaks the Docker API.
//!
//! The Docker API is asynchronous, so this client owns a small runtime on a
//! dedicated thread and hands requests to it. That keeps the
//! [`ContainerClient`](crate::runtime::container::ContainerClient) trait
//! synchronous without hiding the daemon behind a fake result: every call
//! goes to the daemon and reports what the daemon said.

use crate::error::{AnvilError, Result};
use crate::runtime::container::{ContainerClient, ContainerCreateRequest, ContainerDetails};
use crate::runtime::model::ContainerSummary;
use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, ContainerStateStatusEnum, ContainerSummaryStateEnum, HostConfig, Mount,
    MountType, PortBinding, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{CreateContainerOptions, ListContainersOptions};
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

/// How long the daemon may take to answer a request, in seconds.
const REQUEST_TIMEOUT_SECONDS: u64 = 30;

/// A job for the worker thread.
type Job = Box<dyn FnOnce(&Docker, &tokio::runtime::Runtime) + Send>;

/// What the client knows about the daemon.
#[derive(Debug, Clone, Default)]
struct DaemonState {
    reachable: bool,
    status: Option<String>,
}

/// A container client that talks to a Docker-compatible daemon.
pub struct BollardContainerClient {
    runtime: String,
    socket: String,
    sender: Sender<Job>,
    state: Arc<Mutex<DaemonState>>,
}

impl BollardContainerClient {
    /// Connect to a daemon over a Unix socket.
    ///
    /// The client is created once and reused; the daemon is not probed for
    /// its version on every call.
    pub fn connect(runtime: &str, socket: &str) -> Result<Self> {
        let docker = Docker::connect_with_unix(
            socket,
            REQUEST_TIMEOUT_SECONDS,
            bollard::API_DEFAULT_VERSION,
        )
        .map_err(|error| {
            AnvilError::Runtime(format!(
                "{runtime}: cannot connect to the daemon socket {socket}: {error}"
            ))
        })?;

        let (sender, receiver) = mpsc::channel::<Job>();
        let state = Arc::new(Mutex::new(DaemonState::default()));

        let worker_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name(format!("anvil-{runtime}-api"))
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };

                // Learn the daemon version once, so the status line does not
                // have to be re-probed on every refresh.
                let version = runtime.block_on(async { docker.version().await });
                if let Ok(version) = version
                    && let Ok(mut guard) = worker_state.lock()
                {
                    guard.reachable = true;
                    guard.status = version.version.clone();
                }

                while let Ok(job) = receiver.recv() {
                    job(&docker, &runtime);
                }
            })
            .map_err(|error| {
                AnvilError::Runtime(format!(
                    "{runtime}: the container API worker thread did not start: {error}"
                ))
            })?;

        Ok(Self {
            runtime: runtime.to_string(),
            socket: socket.to_string(),
            sender,
            state,
        })
    }

    /// The socket the client talks to.
    pub fn socket(&self) -> &str {
        &self.socket
    }

    /// Run a request on the worker thread and wait for its result.
    fn call<T: Send + 'static>(
        &self,
        request: impl FnOnce(&Docker, &tokio::runtime::Runtime) -> T + Send + 'static,
    ) -> Result<T> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.sender
            .send(Box::new(move |docker, runtime| {
                let _ = sender.send(request(docker, runtime));
            }))
            .map_err(|_| {
                AnvilError::Runtime(format!(
                    "{}: the container API worker thread stopped",
                    self.runtime
                ))
            })?;

        receiver.recv().map_err(|_| {
            AnvilError::Runtime(format!(
                "{}: the container API worker thread stopped",
                self.runtime
            ))
        })
    }

    /// Record whether the daemon answered.
    fn record(&self, reachable: bool) {
        if let Ok(mut guard) = self.state.lock() {
            guard.reachable = reachable;
        }
    }

    /// Run a request and translate a daemon error.
    fn request<T: Send + 'static>(
        &self,
        request: impl FnOnce(
            &Docker,
            &tokio::runtime::Runtime,
        ) -> std::result::Result<T, bollard::errors::Error>
        + Send
        + 'static,
    ) -> Result<T> {
        let outcome = self.call(request)?;
        match outcome {
            Ok(value) => {
                self.record(true);
                Ok(value)
            }
            Err(error) => {
                self.record(false);
                Err(AnvilError::Runtime(format!(
                    "{}: the daemon returned an error: {error}",
                    self.runtime
                )))
            }
        }
    }
}

impl ContainerClient for BollardContainerClient {
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

    fn socket_path(&self) -> Option<&str> {
        Some(&self.socket)
    }

    fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
        let containers = self.request(|docker, runtime| {
            runtime.block_on(async move {
                let options = ListContainersOptions {
                    all: true,
                    ..Default::default()
                };
                docker.list_containers(Some(options)).await
            })
        })?;

        Ok(containers
            .into_iter()
            .map(|container| {
                let name = container
                    .names
                    .as_ref()
                    .and_then(|names| names.first())
                    .map(|name| name.trim_start_matches('/').to_string())
                    .unwrap_or_default();
                let state = container.state.unwrap_or(ContainerSummaryStateEnum::EMPTY);
                let status = match state {
                    ContainerSummaryStateEnum::RUNNING => "running".to_string(),
                    ContainerSummaryStateEnum::EXITED => "exited".to_string(),
                    ContainerSummaryStateEnum::PAUSED => "paused".to_string(),
                    ContainerSummaryStateEnum::CREATED => "created".to_string(),
                    ContainerSummaryStateEnum::RESTARTING => "restarting".to_string(),
                    ContainerSummaryStateEnum::REMOVING => "removing".to_string(),
                    ContainerSummaryStateEnum::DEAD => "dead".to_string(),
                    ContainerSummaryStateEnum::STOPPING => "stopping".to_string(),
                    ContainerSummaryStateEnum::EMPTY => "unknown".to_string(),
                };

                ContainerSummary {
                    id: container.id.unwrap_or_default(),
                    name,
                    image: container.image.unwrap_or_default(),
                    status,
                }
            })
            .collect())
    }

    fn inspect(&self, name: &str) -> Result<ContainerDetails> {
        let requested = name.to_string();
        let details = self.request(move |docker, runtime| {
            runtime.block_on(async move { docker.inspect_container(&requested, None).await })
        })?;

        let state = details.state;
        let running = state
            .as_ref()
            .and_then(|state| state.running)
            .unwrap_or(false);
        let status = state
            .as_ref()
            .and_then(|state| state.status)
            .map(|status| match status {
                ContainerStateStatusEnum::RUNNING => "running".to_string(),
                ContainerStateStatusEnum::EXITED => "exited".to_string(),
                ContainerStateStatusEnum::PAUSED => "paused".to_string(),
                ContainerStateStatusEnum::CREATED => "created".to_string(),
                ContainerStateStatusEnum::RESTARTING => "restarting".to_string(),
                ContainerStateStatusEnum::REMOVING => "removing".to_string(),
                ContainerStateStatusEnum::DEAD => "dead".to_string(),
                ContainerStateStatusEnum::STOPPING => "stopping".to_string(),
                ContainerStateStatusEnum::EMPTY => "unknown".to_string(),
            })
            .unwrap_or_else(|| "unknown".to_string());

        Ok(ContainerDetails {
            id: details.id.unwrap_or_default(),
            name: details
                .name
                .map(|name| name.trim_start_matches('/').to_string())
                .unwrap_or_else(|| name.to_string()),
            image: details.image.unwrap_or_default(),
            status,
            running,
            ports: Vec::new(),
        })
    }

    fn start(&self, name: &str) -> Result<()> {
        let requested = name.to_string();
        self.request(move |docker, runtime| {
            runtime.block_on(async move { docker.start_container(&requested, None).await })
        })
    }

    fn stop(&self, name: &str) -> Result<()> {
        let requested = name.to_string();
        self.request(move |docker, runtime| {
            runtime.block_on(async move { docker.stop_container(&requested, None).await })
        })
    }

    fn restart(&self, name: &str) -> Result<()> {
        let requested = name.to_string();
        self.request(move |docker, runtime| {
            runtime.block_on(async move { docker.restart_container(&requested, None).await })
        })
    }

    fn remove(&self, name: &str) -> Result<()> {
        let requested = name.to_string();
        self.request(move |docker, runtime| {
            runtime.block_on(async move {
                let options = bollard::query_parameters::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                };
                docker.remove_container(&requested, Some(options)).await
            })
        })
    }

    fn pull(&self, image: &str) -> Result<()> {
        let requested = image.to_string();
        self.request(move |docker, runtime| {
            runtime.block_on(async move {
                let options = bollard::query_parameters::CreateImageOptions {
                    from_image: Some(requested.clone()),
                    ..Default::default()
                };
                let mut stream = docker.create_image(Some(options), None, None);
                // The daemon reports progress as a stream of JSON frames; the
                // pull is only finished when the stream ends.
                while let Some(message) = stream.next().await {
                    message?;
                }
                Ok(())
            })
        })
    }

    fn create(&self, request: &ContainerCreateRequest) -> Result<String> {
        let request = request.clone();
        let created = self.request(|docker, runtime| {
            runtime.block_on(async move {
                let options = CreateContainerOptions {
                    name: Some(request.name.clone()),
                    platform: String::new(),
                };

                let exposed_ports: Vec<String> = request
                    .ports
                    .iter()
                    .map(|port| format!("{}/{}", port.container, port.protocol))
                    .collect();

                let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
                for port in &request.ports {
                    port_bindings.insert(
                        format!("{}/{}", port.container, port.protocol),
                        Some(vec![PortBinding {
                            host_ip: None,
                            host_port: Some(port.host.to_string()),
                        }]),
                    );
                }

                let mounts: Vec<Mount> = request
                    .volumes
                    .iter()
                    .map(|volume| Mount {
                        target: Some(volume.target.clone()),
                        source: Some(volume.source.clone()),
                        typ: Some(match volume.kind.as_str() {
                            "bind" => MountType::BIND,
                            _ => MountType::VOLUME,
                        }),
                        read_only: None,
                        consistency: None,
                        bind_options: None,
                        volume_options: None,
                        image_options: None,
                        tmpfs_options: None,
                    })
                    .collect();

                let restart_policy = RestartPolicy {
                    name: Some(restart_policy_name(&request.restart_policy)),
                    maximum_retry_count: None,
                };

                let host_config = HostConfig {
                    port_bindings: Some(port_bindings),
                    mounts: Some(mounts),
                    restart_policy: Some(restart_policy),
                    ..Default::default()
                };

                let body = ContainerCreateBody {
                    image: Some(request.image.clone()),
                    env: Some(
                        request
                            .env
                            .iter()
                            .map(|(key, value)| format!("{key}={value}"))
                            .collect(),
                    ),
                    exposed_ports: if exposed_ports.is_empty() {
                        None
                    } else {
                        Some(exposed_ports)
                    },
                    host_config: Some(host_config),
                    ..Default::default()
                };

                let response = docker.create_container(Some(options), body).await?;
                // `docker run` starts the container; creating it alone would
                // leave it in the `created` state.
                docker.start_container(&response.id, None).await?;
                Ok(response.id)
            })
        })?;

        Ok(created)
    }
}

/// The restart policy name the daemon understands.
fn restart_policy_name(policy: &str) -> RestartPolicyNameEnum {
    match policy {
        "always" => RestartPolicyNameEnum::ALWAYS,
        "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
        "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
        _ => RestartPolicyNameEnum::EMPTY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::container::ContainerCreateRequest;
    use std::path::PathBuf;

    /// The marker the CI job greps for, so a skipped test cannot pass
    /// unnoticed.
    pub const RAN_MARKER: &str = "container api test ran against";

    /// The Docker socket on this host, if there is one.
    fn docker_socket() -> Option<PathBuf> {
        [
            PathBuf::from("/var/run/docker.sock"),
            PathBuf::from("/run/docker.sock"),
        ]
        .into_iter()
        .find(|socket| socket.exists())
    }

    /// A real round trip against a real daemon.
    ///
    /// The test never fakes a daemon. It only runs when
    /// `ANVIL_CONTAINER_API_TEST=1` is set, because it pulls an image and
    /// creates a container on the host it runs on.
    #[test]
    fn the_container_api_talks_to_a_real_daemon() {
        if std::env::var("ANVIL_CONTAINER_API_TEST").as_deref() != Ok("1") {
            eprintln!(
                "SKIPPED: set ANVIL_CONTAINER_API_TEST=1 to run the test against a real daemon"
            );
            return;
        }

        let socket = docker_socket().expect("ANVIL_CONTAINER_API_TEST=1 but no docker socket");
        let client = BollardContainerClient::connect("docker", &socket.to_string_lossy())
            .expect("the daemon socket is accepted");

        println!("{RAN_MARKER} {socket:?}");

        // The daemon answers.
        assert!(client.is_reachable(), "the daemon did not answer");
        assert!(
            client.daemon_status().is_some(),
            "the daemon did not report a version"
        );

        // The container list comes back from the daemon.
        let before = client
            .list_containers()
            .expect("the daemon lists its containers");
        assert!(before.iter().all(|container| !container.name.is_empty()));

        // A real image can be pulled.
        client
            .pull("hello-world:latest")
            .expect("the image is pulled from the registry");

        // A real container can be created and started.
        let name = format!("anvil-api-test-{}", std::process::id());
        let request = ContainerCreateRequest {
            name: name.clone(),
            image: "hello-world:latest".to_string(),
            ports: Vec::new(),
            volumes: Vec::new(),
            env: Vec::new(),
            restart_policy: "no".to_string(),
        };
        let id = client
            .create(&request)
            .expect("the container is created and started");
        assert!(!id.is_empty());

        // The daemon knows about it.
        let details = client
            .inspect(&name)
            .expect("the container can be inspected");
        assert_eq!(details.name, name);
        assert!(details.id.starts_with(&id[..12.min(id.len())]));

        // And it can be removed again.
        client.remove(&name).expect("the container is removed");
        let after = client
            .list_containers()
            .expect("the daemon lists its containers");
        assert!(
            after.iter().all(|container| container.name != name),
            "the removed container is still listed"
        );
    }
}
