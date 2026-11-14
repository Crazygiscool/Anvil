//! Telemetry tab - live system telemetry dashboard.

use crate::app::state::AppState;
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{BuildCache, DetailPane};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Wrap},
};

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.1} {}", size, UNITS[unit_idx])
}

fn format_rate(bytes_per_second: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_second.max(0.0) as u64))
}

pub struct TelemetryTab {
    system_detail: DetailPane,
    show_help: bool,
    /// What has already been built, so a keystroke does not rebuild the
    /// whole pane.
    cache: BuildCache<()>,
}

impl TelemetryTab {
    pub fn new() -> Self {
        let system_detail = DetailPane::new("System Telemetry");

        Self {
            system_detail,
            show_help: false,
            cache: BuildCache::default(),
        }
    }

    fn build_system_detail(&mut self, state: &AppState) {
        self.system_detail.clear();
        self.system_detail.set_title("System Telemetry");

        let Some(telemetry) = &state.telemetry else {
            self.system_detail
                .push_field("No telemetry data", "Waiting for first sample...");
            return;
        };

        // CPU
        self.system_detail
            .add_section(crate::tui::widgets::DetailSection {
                title: "CPU".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "User".to_string(),
                        value: telemetry.cpu.user.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "System".to_string(),
                        value: telemetry.cpu.system.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Idle".to_string(),
                        value: telemetry.cpu.idle.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Nice".to_string(),
                        value: telemetry.cpu.nice.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "IO Wait".to_string(),
                        value: telemetry.cpu.iowait.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Steal".to_string(),
                        value: telemetry.cpu.steal.to_string(),
                        style: None,
                    },
                ],
            });

        // Memory
        self.system_detail
            .add_section(crate::tui::widgets::DetailSection {
                title: "Memory".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Total".to_string(),
                        value: format_bytes(telemetry.memory.total_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Used".to_string(),
                        value: format_bytes(telemetry.memory.used_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Free".to_string(),
                        value: format_bytes(telemetry.memory.free_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Available".to_string(),
                        value: format_bytes(telemetry.memory.available_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Swap Total".to_string(),
                        value: format_bytes(telemetry.memory.swap_total_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Swap Used".to_string(),
                        value: format_bytes(telemetry.memory.swap_used_bytes),
                        style: None,
                    },
                ],
            });

        // Load average
        if let Some(load) = &telemetry.load_average {
            self.system_detail
                .add_section(crate::tui::widgets::DetailSection {
                    title: "Load Average".to_string(),
                    expanded: true,
                    fields: vec![
                        crate::tui::widgets::DetailField {
                            key: "1 min".to_string(),
                            value: format!("{:.2}", load.one),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "5 min".to_string(),
                            value: format!("{:.2}", load.five),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "15 min".to_string(),
                            value: format!("{:.2}", load.fifteen),
                            style: None,
                        },
                    ],
                });
        }

        // Disk I/O
        self.system_detail
            .add_section(crate::tui::widgets::DetailSection {
                title: "Disk I/O".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Read".to_string(),
                        value: format_bytes(telemetry.disk.read_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Write".to_string(),
                        value: format_bytes(telemetry.disk.write_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Read Ops".to_string(),
                        value: telemetry.disk.read_ops.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Write Ops".to_string(),
                        value: telemetry.disk.write_ops.to_string(),
                        style: None,
                    },
                ],
            });

        // Network
        self.system_detail
            .add_section(crate::tui::widgets::DetailSection {
                title: "Network".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "RX".to_string(),
                        value: format_bytes(telemetry.network.rx_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "TX".to_string(),
                        value: format_bytes(telemetry.network.tx_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "RX Packets".to_string(),
                        value: telemetry.network.rx_packets.to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "TX Packets".to_string(),
                        value: telemetry.network.tx_packets.to_string(),
                        style: None,
                    },
                ],
            });

        // Rates derived from the previous sample
        if let Some(rates) = &telemetry.rates {
            self.system_detail
                .add_section(crate::tui::widgets::DetailSection {
                    title: "Rates".to_string(),
                    expanded: true,
                    fields: vec![
                        crate::tui::widgets::DetailField {
                            key: "CPU Usage".to_string(),
                            value: rates
                                .cpu_usage
                                .map(|value| format!("{:.1}%", value * 100.0))
                                .unwrap_or_else(|| "n/a".to_string()),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "Disk Read".to_string(),
                            value: rates
                                .disk_read_bytes_per_second
                                .map(format_rate)
                                .unwrap_or_else(|| "n/a".to_string()),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "Disk Write".to_string(),
                            value: rates
                                .disk_write_bytes_per_second
                                .map(format_rate)
                                .unwrap_or_else(|| "n/a".to_string()),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "Network RX".to_string(),
                            value: rates
                                .network_rx_bytes_per_second
                                .map(format_rate)
                                .unwrap_or_else(|| "n/a".to_string()),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "Network TX".to_string(),
                            value: rates
                                .network_tx_bytes_per_second
                                .map(format_rate)
                                .unwrap_or_else(|| "n/a".to_string()),
                            style: None,
                        },
                    ],
                });
        }
    }
}

impl Tab for TelemetryTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Telemetry
    }

    fn title(&self) -> &'static str {
        "Telemetry"
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut AppState) -> bool {
        if self.system_detail.handle_key(key) {
            return true;
        }

        match key.code {
            crossterm::event::KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 8 } else { 0 }),
            ])
            .split(area);

        self.system_detail.render(frame, chunks[0]);

        if self.show_help {
            let help = Paragraph::new(
                "Telemetry Help:\n\
                j/k - Scroll telemetry\n\
                Ctrl+d/u - Page telemetry\n\
                g - Jump to top\n\
                Space - Toggle section\n\
                ? - Toggle help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, chunks[1]);
        }
    }

    fn update(&mut self, state: &AppState) {
        let _ = self
            .cache
            .rows_changed(None, Some(state.telemetry_generation), None);
        if self.cache.detail_changed(None) {
            self.build_system_detail(state);
        }
    }
}
