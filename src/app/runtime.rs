use crate::access::AccessContext;
use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::config::Config;
use crate::error::Result;
use crate::platform::{CommandRunner, TextReader};
use crate::tui::run_tui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

pub struct AppRuntime {
    pub state: Arc<RwLock<AppState>>,
    pub reader: Arc<dyn TextReader + Send + Sync>,
    pub runner: Arc<dyn CommandRunner + Send + Sync>,
    events: broadcast::Sender<AppEvent>,
}

impl AppRuntime {
    pub fn bootstrap(
        config: Config,
        access: AccessContext,
        manifest_paths: Vec<PathBuf>,
        reader: Arc<dyn TextReader + Send + Sync>,
        runner: Arc<dyn CommandRunner + Send + Sync>,
    ) -> Self {
        let (sender, _) = broadcast::channel(1024);
        let state = AppState::bootstrap(config, access, manifest_paths);
        Self {
            state: Arc::new(RwLock::new(state)),
            reader,
            runner,
            events: sender,
        }
    }

    #[allow(dead_code)]
    pub fn events(&self) -> broadcast::Sender<AppEvent> {
        self.events.clone()
    }

    pub async fn run_tui(&self, refresh_rate: u64) -> Result<()> {
        let host_refresh_rate = self.state.read().await.config.host_refresh_rate_ms;
        run_tui(
            self.state.clone(),
            refresh_rate,
            host_refresh_rate,
            self.events.clone(),
            self.reader.clone(),
            self.runner.clone(),
        )
        .await
    }
}

/// Collect the host state that the dashboard renders.
pub fn collect_host_state(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
) -> (
    crate::discovery::DiscoveryReport,
    crate::storage::StorageInspection,
    crate::runtime::RuntimeInspection,
) {
    (
        crate::discovery::all(reader, runner),
        crate::storage::inspect_storage(reader, runner),
        crate::runtime::inspect_runtime(reader, runner),
    )
}
