use crossterm::execute;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use std::io::{self, Stdout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Discovery,
    Storage,
    Runtime,
    Telemetry,
}

impl ActiveTab {
    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Discovery => "Discovery",
            Self::Storage => "Storage",
            Self::Runtime => "Runtime",
            Self::Telemetry => "Telemetry",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Discovery,
            Self::Discovery => Self::Storage,
            Self::Storage => Self::Runtime,
            Self::Runtime => Self::Telemetry,
            Self::Telemetry => Self::Overview,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Telemetry,
            Self::Discovery => Self::Overview,
            Self::Storage => Self::Discovery,
            Self::Runtime => Self::Storage,
            Self::Telemetry => Self::Runtime,
        }
    }
}

#[allow(dead_code)]
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

#[allow(dead_code)]
impl TerminalSession {
    pub fn startup() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
        })
    }

    pub fn restore(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        );
    }
}

pub fn render_tab(
    frame: &mut Frame,
    state: &crate::app::state::AppState,
    tab: ActiveTab,
    show_help: bool,
) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);
    let title_area = layout[0];
    let main_area = layout[1];
    let footer_area = layout[2];

    let tabs = Tabs::new(vec![
        ActiveTab::Overview.title(),
        ActiveTab::Discovery.title(),
        ActiveTab::Storage.title(),
        ActiveTab::Runtime.title(),
        ActiveTab::Telemetry.title(),
    ])
    .select(tab_index(tab))
    .block(Block::default().borders(Borders::ALL).title(format!(
        "Anvil ({} access)",
        access_label(&state.access.tier)
    )));
    frame.render_widget(tabs, title_area);

    let content = if show_help {
        String::from("Controls: q exit, Left/Right tab, h help, j/k scroll (read-only state).")
    } else {
        match tab {
            ActiveTab::Overview => format!(
                "Overview\nConfig manifest: {}\nDiscovery interfaces: {}\nStorage pools: {}\nRuntime containers: {}\nTelemetry load: {:?}",
                state.config.manifest_path.display(),
                state.discovery.as_ref().map(|report| report.network.interfaces.len()).unwrap_or(0),
                state.storage.as_ref().map(|report| report.zfs_pools.len()).unwrap_or(0),
                state.runtime.as_ref().map(|report| report.container_runtimes.len()).unwrap_or(0),
                state.telemetry.as_ref().and_then(|sample| sample.load_average.as_ref().map(|load| load.one)),
            ),
            ActiveTab::Discovery => state
                .discovery
                .as_ref()
                .map(|report| format!(
                    "Storage: zfs={}, btrfs={}, mounts={}\nNetwork: {} interfaces\nDiagnostics: {}",
                    report.storage.zfs.present,
                    report.storage.btrfs.present,
                    report.storage.mounts.len(),
                    report.network.interfaces.len(),
                    report.diagnostics.len()
                ))
                .unwrap_or_else(|| "Discovery not yet available".to_string()),
            ActiveTab::Storage => state
                .storage
                .as_ref()
                .map(|report| {
                    format!(
                        "ZFS pools: {}\nZFS datasets: {}\nBtrfs subvolumes: {}\nLVM volumes: {}\nDiagnostics: {}",
                        report.zfs_pools.len(),
                        report.zfs_datasets.len(),
                        report.btrfs_subvolumes.len(),
                        report.lvm_volumes.len(),
                        report.diagnostics.len()
                    )
                })
                .unwrap_or_else(|| "Storage not yet inspected".to_string()),
            ActiveTab::Runtime => state
                .runtime
                .as_ref()
                .map(|report| {
                    format!(
                        "Init: {} ({})\nContainer runtimes: {}\n",
                        report.init.kind,
                        if report.init.detected { "detected" } else { "absent" },
                        report.container_runtimes.len()
                    )
                })
                .unwrap_or_else(|| "Runtime not yet inspected".to_string()),
            ActiveTab::Telemetry => state
                .telemetry
                .as_ref()
                .map(|sample| {
                    format!(
                        "CPU totals: user={}, system={}, idle={}\nMemory used/total: {} / {}\nDisk read/write: {} / {}\nNetwork rx/tx: {} / {}",
                        sample.cpu.user,
                        sample.cpu.system,
                        sample.cpu.idle,
                        sample.memory.used_bytes,
                        sample.memory.total_bytes,
                        sample.disk.read_bytes,
                        sample.disk.write_bytes,
                        sample.network.rx_bytes,
                        sample.network.tx_bytes,
                    )
                })
                .unwrap_or_else(|| "Telemetry not yet sampled".to_string()),
        }
    };
    let body = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("View"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(body, main_area);

    let status = Paragraph::new("q quit | left/right switch tabs | h help")
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(status, footer_area);
}

pub fn tab_index(tab: ActiveTab) -> usize {
    match tab {
        ActiveTab::Overview => 0,
        ActiveTab::Discovery => 1,
        ActiveTab::Storage => 2,
        ActiveTab::Runtime => 3,
        ActiveTab::Telemetry => 4,
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_overview_without_panic() {
        let state = crate::app::state::AppState::bootstrap(
            crate::config::Config::default(),
            crate::access::AccessContext::detect(false),
            Vec::new(),
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab(frame, &state, ActiveTab::Overview, false))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Overview"));
    }
}
