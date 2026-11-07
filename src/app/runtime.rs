use crate::access::AccessContext;
use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::config::Config;
use crate::error::Result;
use crate::platform::TextReader;
use crate::telemetry::TelemetrySampler;
use crate::tui::ActiveTab;
use crate::tui::render_tab;
use crossterm::event::{Event, KeyEventKind, poll as poll_event, read as read_event};
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};
use tokio::time::Duration as TokioDuration;

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

    #[allow(clippy::collapsible_if)]
    pub async fn run_tui(&self, refresh_rate: u64) -> Result<()> {
        if crossterm::terminal::size().is_err() {
            let _ = tokio::signal::ctrl_c().await;
            return Ok(());
        }
        enable_raw_mode().map_err(io_to_result)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(io_to_result)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(io_to_result)?;

        let mut telemetry_task = tokio::spawn({
            let state = self.state.clone();
            let reader = self.reader.clone();
            let events = self.events.clone();
            async move {
                let mut interval =
                    tokio::time::interval(TokioDuration::from_millis(refresh_rate.max(1)));
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

        let mut tab = ActiveTab::Overview;
        let mut show_help = false;
        let mut interval = tokio::time::interval(TokioDuration::from_millis(refresh_rate.max(1)));

        let result = loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    if signal.is_ok() {
                        let _ = self.events.send(AppEvent::Shutdown);
                    }
                    break Ok(());
                }
                result = &mut telemetry_task => {
                    let _ = result;
                    break Err(crate::error::AnvilError::Runtime("telemetry loop exited".to_string()));
                }
                _ = interval.tick() => {
                    if let Ok(true) = poll_event(Duration::ZERO) {
                        if let Ok(Event::Key(key)) = read_event() {
                            if key.kind == KeyEventKind::Press {
                                match key.code {
                                    crossterm::event::KeyCode::Char('q') => break Ok(()),
                                    crossterm::event::KeyCode::Char('h') => show_help = !show_help,
                                    crossterm::event::KeyCode::Tab => tab = tab.next(),
                                    crossterm::event::KeyCode::Left => tab = tab.previous(),
                                    crossterm::event::KeyCode::Right => tab = tab.next(),
                                    _ => {}
                                }
                            }
                        }
                    }

                    let snapshot = self.state.try_read();
                    if let Ok(state) = &snapshot {
                        let _ = terminal.draw(|frame| {
                            render_tab(frame, state, tab, show_help);
                        });
                    }
                }
            }
        };

        telemetry_task.abort();
        let _ = telemetry_task.await;
        let _ = disable_raw_mode();
        let _ = execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        );
        result
    }
}

fn io_to_result(error: io::Error) -> crate::error::AnvilError {
    crate::error::AnvilError::Io {
        path: std::path::PathBuf::from("<terminal>"),
        source: error,
    }
}
