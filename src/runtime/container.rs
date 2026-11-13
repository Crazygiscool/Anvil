//! A container runtime client.
//!
//! Anvil talks to container daemons through the [`ContainerClient`] trait.
//! The Docker API is the primary transport; the command line client is the
//! fallback for daemons that do not expose a socket, such as rootless
//! Podman without a service socket.
//!
//! The trait is synchronous on purpose: the rest of Anvil's integration
//! layer ([`crate::platform::CommandRunner`], the mutation controller, the
//! TUI) is synchronous as well, and a synchronous trait keeps the daemon
//! call on the same code path everywhere. The Docker API client is
//! asynchronous, so its implementation owns a small runtime on a dedicated
//! thread and hands requests to it.

// The client API is a complete interface for the container runtimes; not
// every operation is wired into a command or a tab yet.

use crate::error::Result;
use crate::platform::{CommandRunner, TextReader};
use crate::runtime::model::ContainerSummary;
use crate::runtime::mutation::{PortMapping, VolumeMount};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

pub mod bollard;
pub mod cli;
/// The in-memory client the tests use instead of a daemon.
#[cfg(test)]
pub mod mock;

#[allow(unused_imports)]
pub use bollard::BollardContainerClient;
#[allow(unused_imports)]
pub use cli::CliContainerClient;
#[cfg(test)]
pub use mock::MockContainerClient;

/// What the daemon knows about a single container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerDetails {
    pub id: String,
    pub name: String,
    pub image: String,
    /// The human readable status, e.g. `Up 3 minutes`.
    pub status: String,
    /// Whether the container is currently running.
    pub running: bool,
    /// The published ports, formatted as `host:container/protocol`.
    pub ports: Vec<String>,
}

/// A container that should be created and started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCreateRequest {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub env: Vec<(String, String)>,
    pub restart_policy: String,
}

/// A client for one container runtime.
pub trait ContainerClient: Send + Sync {
    /// The name of the runtime, `docker` or `podman`.
    fn runtime_name(&self) -> &str;

    /// Whether the daemon answered the last request.
    ///
    /// A client that has never reached its daemon reports `false`; the
    /// diagnostics then say the daemon is not reachable instead of claiming
    /// an empty host.
    fn is_reachable(&self) -> bool;

    /// The status string the daemon reported, e.g. the server version.
    fn daemon_status(&self) -> Option<String>;

    /// The socket the client talks to, when it talks over a socket.
    fn socket_path(&self) -> Option<&str> {
        None
    }

    /// The binary the client drives, when it drives one.
    fn binary_path(&self) -> Option<&str> {
        None
    }

    /// List every container the daemon knows about, running or not.
    fn list_containers(&self) -> Result<Vec<ContainerSummary>>;

    /// Inspect a container by name.
    fn inspect(&self, name: &str) -> Result<ContainerDetails>;

    /// Start a container.
    fn start(&self, name: &str) -> Result<()>;

    /// Stop a container.
    fn stop(&self, name: &str) -> Result<()>;

    /// Restart a container.
    fn restart(&self, name: &str) -> Result<()>;

    /// Remove a container.
    fn remove(&self, name: &str) -> Result<()>;

    /// Pull an image.
    fn pull(&self, image: &str) -> Result<()>;

    /// Create a container and start it.
    ///
    /// Returns the id of the new container.
    fn create(&self, request: &ContainerCreateRequest) -> Result<String>;
}

/// The container clients of the runtimes on this host.
///
/// The clients are built once and then reused, so the daemon is not probed
/// for its version on every refresh.
#[derive(Clone, Default)]
pub struct ContainerClients {
    clients: Vec<Arc<dyn ContainerClient>>,
}

impl ContainerClients {
    /// An empty registry: no runtime has a client.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a registry from the clients.
    ///
    /// The registry that the application uses is built by
    /// [`discover_clients`]; this constructor is for the tests.
    #[cfg(test)]
    pub fn new(clients: Vec<Arc<dyn ContainerClient>>) -> Self {
        Self { clients }
    }

    /// The client for a runtime, if there is one.
    pub fn get(&self, runtime: &str) -> Option<&Arc<dyn ContainerClient>> {
        self.clients
            .iter()
            .find(|client| client.runtime_name() == runtime)
    }

    /// Every client in the registry.
    pub fn clients(&self) -> impl Iterator<Item = &Arc<dyn ContainerClient>> {
        self.clients.iter()
    }

    /// The names of the runtimes that have a client.
    ///
    /// Only used by the tests.
    #[cfg(test)]
    pub fn runtimes(&self) -> Vec<&str> {
        self.clients
            .iter()
            .map(|client| client.runtime_name())
            .collect()
    }

    /// Whether the registry holds any client.
    ///
    /// Only used by the tests.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    /// Add a client to the registry.
    pub fn push(&mut self, client: Arc<dyn ContainerClient>) {
        self.clients.push(client);
    }
}

/// The sockets the Docker daemon listens on.
pub fn docker_sockets() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/var/run/docker.sock"),
        PathBuf::from("/run/docker.sock"),
    ]
}

/// The sockets the Podman daemon listens on.
///
/// The rootless socket comes first: a rootless daemon is the one that runs
/// without a system service.
pub fn podman_sockets() -> Vec<PathBuf> {
    let mut sockets = Vec::new();
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        sockets.push(PathBuf::from(runtime_dir).join("podman/podman.sock"));
    }
    sockets.push(PathBuf::from(format!(
        "/run/user/{}/podman/podman.sock",
        crate::access::effective_uid()
    )));
    sockets.push(PathBuf::from("/run/podman/podman.sock"));
    sockets
}

/// Build the clients for the container runtimes on this host.
///
/// A runtime that exposes a socket gets an API client; a runtime without one
/// falls back to its command line client. The clients are built once and
/// then reused, so the daemon is not probed on every refresh.
pub fn discover_clients(
    reader: &dyn TextReader,
    runner: Arc<dyn CommandRunner + Send + Sync>,
) -> ContainerClients {
    let mut clients = ContainerClients::empty();

    for (runtime, sockets) in [("docker", docker_sockets()), ("podman", podman_sockets())] {
        if let Some(client) = connect_api_client(runtime, &sockets) {
            clients.push(Arc::new(client));
            continue;
        }

        if let Some(client) = command_line_client(reader, runtime, Arc::clone(&runner)) {
            clients.push(Arc::new(client));
        }
    }

    clients
}

/// Connect to the first socket that answers.
fn connect_api_client(runtime: &str, sockets: &[PathBuf]) -> Option<BollardContainerClient> {
    for socket in sockets {
        if !socket.exists() {
            continue;
        }
        let path = socket.to_string_lossy().to_string();
        match BollardContainerClient::connect(runtime, &path) {
            Ok(client) => return Some(client),
            Err(_) => continue,
        }
    }
    None
}

/// Build a command line client for a runtime whose binary is installed.
fn command_line_client(
    reader: &dyn TextReader,
    runtime: &str,
    runner: Arc<dyn CommandRunner + Send + Sync>,
) -> Option<CliContainerClient> {
    let binary = crate::storage::inspect::find_binary(reader, runtime)?;
    let client = CliContainerClient::new(runtime, &binary.to_string_lossy(), runner);
    // Learn the daemon version once; the probe is cached by the client.
    let _ = client.probe();
    Some(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A client that records the calls it receives.
    #[derive(Default)]
    struct RecordingClient {
        runtime: String,
        calls: Mutex<Vec<String>>,
    }

    impl ContainerClient for RecordingClient {
        fn runtime_name(&self) -> &str {
            &self.runtime
        }

        fn is_reachable(&self) -> bool {
            true
        }

        fn daemon_status(&self) -> Option<String> {
            Some("test".to_string())
        }

        fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
            self.calls.lock().unwrap().push("list".to_string());
            Ok(Vec::new())
        }

        fn inspect(&self, _name: &str) -> Result<ContainerDetails> {
            unimplemented!("the registry tests do not inspect")
        }

        fn start(&self, _name: &str) -> Result<()> {
            unimplemented!("the registry tests do not start")
        }

        fn stop(&self, _name: &str) -> Result<()> {
            unimplemented!("the registry tests do not stop")
        }

        fn restart(&self, _name: &str) -> Result<()> {
            unimplemented!("the registry tests do not restart")
        }

        fn remove(&self, _name: &str) -> Result<()> {
            unimplemented!("the registry tests do not remove")
        }

        fn pull(&self, _image: &str) -> Result<()> {
            unimplemented!("the registry tests do not pull")
        }

        fn create(&self, _request: &ContainerCreateRequest) -> Result<String> {
            unimplemented!("the registry tests do not create")
        }
    }

    fn client(runtime: &str) -> Arc<dyn ContainerClient> {
        Arc::new(RecordingClient {
            runtime: runtime.to_string(),
            calls: Mutex::new(Vec::new()),
        })
    }

    #[test]
    fn the_registry_finds_a_client_by_runtime() {
        let clients = ContainerClients::new(vec![client("docker"), client("podman")]);

        assert_eq!(clients.runtimes(), vec!["docker", "podman"]);
        assert!(clients.get("docker").is_some());
        assert!(clients.get("containerd").is_none());
    }

    #[test]
    fn an_empty_registry_has_no_runtimes() {
        let clients = ContainerClients::empty();

        assert!(clients.is_empty());
        assert!(clients.runtimes().is_empty());
        assert!(clients.get("docker").is_none());
    }

    #[test]
    fn clients_can_be_added_after_the_registry_is_built() {
        let mut clients = ContainerClients::empty();
        clients.push(client("docker"));

        assert!(!clients.is_empty());
        assert!(clients.get("docker").is_some());
    }
}
