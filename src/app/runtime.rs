use crate::access::AccessContext;
use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::config::Config;
use crate::error::Result;
use crate::platform::TextReader;
use crate::telemetry::TelemetrySampler;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio::time::Duration;

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
        let mut telemetry_task = tokio::spawn({
            let state = self.state.clone();
            let reader = self.reader.clone();
            let events = self.events.clone();
            async move {
                let mut interval =
                    tokio::time::interval(Duration::from_millis(refresh_rate.max(1)));
                let mut sampler = TelemetrySampler::new();

                let startup_sample = sampler.sample(&*reader);
                {
                    let mut guard = state.write().await;
                    guard.telemetry = Some(startup_sample.clone());
                    guard.running = true;
                }
                let _ = events.send(AppEvent::TelemetrySample(Box::new(startup_sample)));

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            let sample = sampler.sample(&*reader);
                            if let Ok(mut guard) = state.try_write() {
                                guard.telemetry = Some(sample.clone());
                                guard.running = true;
                            }
                            let _ = events.send(AppEvent::TelemetrySample(Box::new(sample)));
                        }
                    }
                }
            }
        });

        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    let _ = self.events.send(AppEvent::Shutdown);
                }
                telemetry_task.abort();
                let _ = telemetry_task.await;
            }
            result = &mut telemetry_task => {
                let _ = result;
            }
        }

        Ok(())
    }
}
