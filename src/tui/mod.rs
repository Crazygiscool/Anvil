//! Interactive TUI module with tab management and reusable widgets.

pub mod tabs;
pub mod widgets;

use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::tui::tabs::TabManager;
use crate::tui::tabs::catalog::CatalogTab;
use crate::tui::tabs::discovery::DiscoveryTab;
use crate::tui::tabs::overview::OverviewTab;
use crate::tui::tabs::runtime::RuntimeTab;
use crate::tui::tabs::storage::StorageTab;
use crate::tui::tabs::telemetry::TelemetryTab;
use crossterm::execute;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};

/// Identifies a TUI tab.
///
/// This is the single source of truth for tab identity: [`tabs::TabManager`]
/// resolves the active tab through it, so a tab can never disagree with the
/// tab bar about which view is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Discovery,
    Storage,
    Runtime,
    Telemetry,
    Catalog,
}

impl ActiveTab {
    /// Stable identifier used in status output.
    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Discovery => "discovery",
            Self::Storage => "storage",
            Self::Runtime => "runtime",
            Self::Telemetry => "telemetry",
            Self::Catalog => "catalog",
        }
    }
}

/// Create the tab manager with all tabs.
pub fn create_tab_manager() -> TabManager {
    let tabs: Vec<Box<dyn tabs::Tab>> = vec![
        Box::new(OverviewTab::new()),
        Box::new(DiscoveryTab::new()),
        Box::new(StorageTab::new()),
        Box::new(RuntimeTab::new()),
        Box::new(TelemetryTab::new()),
        Box::new(CatalogTab::new()),
    ];
    TabManager::new(tabs)
}

/// Run the TUI event loop with the tab manager shell.
pub async fn run_tui(
    state: Arc<RwLock<AppState>>,
    refresh_rate: u64,
    host_refresh_rate: u64,
    events: broadcast::Sender<AppEvent>,
    reader: Arc<dyn crate::platform::TextReader + Send + Sync>,
    runner: Arc<dyn crate::platform::CommandRunner + Send + Sync>,
    clients: crate::runtime::container::ContainerClients,
) -> crate::error::Result<()> {
    use crossterm::event::{Event, KeyEventKind, poll as poll_event, read as read_event};
    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };
    use tokio::time::Duration as TokioDuration;

    if crossterm::terminal::size().is_err() {
        let _ = tokio::signal::ctrl_c().await;
        return Ok(());
    }

    enable_raw_mode().map_err(io_to_result)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(io_to_result)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(io_to_result)?;

    let interval_duration = TokioDuration::from_millis(refresh_rate.max(1));

    // Keep a handle on the reader/runner for the host state refresh, which
    // outlives the telemetry task below.
    let refresh_reader = reader.clone();
    let refresh_runner = runner.clone();
    let refresh_clients = clients.clone();

    let mut telemetry_task = tokio::spawn({
        let state = state.clone();
        let events = events.clone();
        async move {
            let mut interval = tokio::time::interval(interval_duration);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut sampler = crate::telemetry::TelemetrySampler::new();

            let startup_sample = sampler.sample(reader.as_ref());
            {
                let mut guard = state.write().await;
                guard.telemetry = Some(startup_sample.clone());
                guard.running = true;
            }
            let _ = events.send(AppEvent::TelemetrySample(Box::new(startup_sample)));

            loop {
                interval.tick().await;
                let sample = sampler.sample(reader.as_ref());
                if let Ok(mut guard) = state.try_write() {
                    guard.telemetry = Some(sample.clone());
                    guard.running = true;
                }
                let _ = events.send(AppEvent::TelemetrySample(Box::new(sample)));
            }
        }
    });

    let mut tab_manager = create_tab_manager();
    let mut show_help = false;
    let event_sender = events.clone();
    let mut events = events.subscribe();
    let mut tick = tokio::time::interval(interval_duration);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Host state (discovery, storage, runtime) is re-collected on its own
    // cadence so the dashboard does not show a stale snapshot.
    let host_refresh_rate = host_refresh_rate.max(refresh_rate.max(1)).max(1_000);
    let mut refresh_tick =
        tokio::time::interval(std::time::Duration::from_millis(host_refresh_rate));
    refresh_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut refresh_task: Option<tokio::task::JoinHandle<()>> = None;

    let result = loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    let _ = event_sender.send(AppEvent::Shutdown);
                }
                break Ok(());
            }
            result = &mut telemetry_task => {
                let _ = result;
                break Err(crate::error::AnvilError::Runtime("telemetry loop exited".to_string()));
            }
            event = events.recv() => {
                match event {
                    Ok(AppEvent::Shutdown) => break Ok(()),
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Closed) => break Ok(()),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
            _ = refresh_tick.tick(), if refresh_task.is_none() => {
                let state = state.clone();
                let reader = refresh_reader.clone();
                let runner = refresh_runner.clone();
                let clients = refresh_clients.clone();
                refresh_task = Some(tokio::spawn(async move {
                    let collected = tokio::task::spawn_blocking(move || {
                        crate::app::runtime::collect_host_state(
                            reader.as_ref(),
                            runner.as_ref(),
                            &clients,
                        )
                    })
                    .await;

                    let Ok((discovery, storage, runtime)) = collected else {
                        return;
                    };
                    let mut guard = state.write().await;
                    guard.discovery = Some(discovery);
                    guard.storage = Some(storage);
                    guard.runtime = Some(runtime);
                }));
            }
            _ = tick.tick() => {
                if let Some(handle) = &mut refresh_task
                    && handle.is_finished()
                {
                    refresh_task = None;
                }

                if let Ok(true) = poll_event(Duration::ZERO)
                    && let Ok(Event::Key(key)) = read_event()
                        && key.kind == KeyEventKind::Press {
                            let mut state_guard = state.write().await;
                            let handled = tab_manager.handle_key(key, &mut state_guard);
                            if !handled {
                                match key.code {
                                    crossterm::event::KeyCode::Char('q') => break Ok(()),
                                    crossterm::event::KeyCode::Char('h') => show_help = !show_help,
                                    _ => {}
                                }
                            }
                        }
            }
        }

        let snapshot = state.try_read();
        if let Ok(state_guard) = snapshot {
            let _ = terminal.draw(|frame| {
                render_tui(frame, &mut tab_manager, &state_guard, show_help);
            });
        }
    };

    if let Some(handle) = refresh_task {
        handle.abort();
    }
    telemetry_task.abort();
    let _ = telemetry_task.await;
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    result
}

fn render_tui(frame: &mut Frame, tab_manager: &mut TabManager, state: &AppState, show_help: bool) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(if show_help { 3 } else { 0 }),
        ])
        .split(area);

    let title_area = layout[0];
    let main_area = layout[1];
    let footer_area = layout.get(2).copied().unwrap_or_default();

    let tab_titles: Vec<String> = tab_manager
        .tabs
        .iter()
        .map(|tab| tab.title().to_string())
        .collect();
    let tabs = Tabs::new(tab_titles)
        .select(tab_manager.active_index())
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Anvil ({} access)",
            access_label(&state.access.tier)
        )));
    frame.render_widget(tabs, title_area);

    tab_manager.render(frame, main_area, state);

    let status = Paragraph::new(format!(
        "q quit | Tab/Shift+Tab switch tabs | h help | a actions | / filter | ? tab help | view: {}",
        tab_manager.active().name()
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(status, footer_area);
}

fn io_to_result(error: io::Error) -> crate::error::AnvilError {
    crate::error::AnvilError::Io {
        path: std::path::PathBuf::from("<terminal>"),
        source: error,
    }
}

fn access_label(tier: &crate::access::AccessTier) -> &'static str {
    match tier {
        crate::access::AccessTier::Root => "root",
        crate::access::AccessTier::ReadOnly => "read-only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_tab_manager_with_unique_tabs() {
        let manager = create_tab_manager();
        assert_eq!(manager.tabs.len(), 6);

        let mut names: Vec<&'static str> = manager.tabs.iter().map(|tab| tab.id().name()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "tab identifiers must be unique");
    }

    #[test]
    fn active_tab_names_are_stable() {
        assert_eq!(ActiveTab::Overview.name(), "overview");
        assert_eq!(ActiveTab::Catalog.name(), "catalog");
    }
}
