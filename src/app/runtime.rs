use crate::access::AccessContext;
use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::config::Config;
use crate::error::Result;
use crate::platform::TextReader;
use crate::tui::run_tui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

pub struct AppRuntime {
    pub state: Arc<RwLock<AppState>>,
    pub reader: Arc<dyn TextReader + Send + Sync>,
    events: broadcast::Sender<AppEvent>,
}

impl AppRuntime {
    pub fn bootstrap(
        config: Config,
        access: AccessContext,
        manifest_paths: Vec<PathBuf>,
        reader: Arc<dyn TextReader + Send + Sync>,
    ) -> Self {
        let (sender, _) = broadcast::channel(1024);
        let state = AppState::bootstrap(config, access, manifest_paths);
        Self {
            state: Arc::new(RwLock::new(state)),
            reader,
            events: sender,
        }
    }

    #[allow(dead_code)]
    pub fn events(&self) -> broadcast::Sender<AppEvent> {
        self.events.clone()
    }

    pub async fn run_tui(&self, refresh_rate: u64) -> Result<()> {
        run_tui(
            self.state.clone(),
            refresh_rate,
            self.events.clone(),
            self.reader.clone(),
        )
        .await
    }
}
