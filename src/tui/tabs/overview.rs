//! Overview tab - system health dashboard with quick actions.

use crate::app::state::AppState;
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{DetailPane, ListWidget, SelectableItem};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Quick action items for overview.
#[derive(Debug, Clone)]
struct OverviewAction {
    id: String,
    label: String,
    description: String,
    key: Option<char>,
}

impl SelectableItem for OverviewAction {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> String {
        self.label.clone()
    }

    fn subtitle(&self) -> Option<String> {
        Some(self.description.clone())
    }

    fn prefix(&self) -> Option<&str> {
        None
    }
}

pub struct OverviewTab {
    actions: ListWidget<OverviewAction>,
    detail: DetailPane,
    show_help: bool,
    requested: Option<ActiveTab>,
}

impl OverviewTab {
    pub fn new() -> Self {
        let actions = vec![
            OverviewAction {
                id: "storage".to_string(),
                label: "Manage Storage".to_string(),
                description: "Create pools, datasets, mount filesystems".to_string(),
                key: Some('s'),
            },
            OverviewAction {
                id: "runtime".to_string(),
                label: "Manage Containers".to_string(),
                description: "Start, stop, inspect Docker/Podman containers".to_string(),
                key: Some('c'),
            },
            OverviewAction {
                id: "catalog".to_string(),
                label: "Browse App Catalog".to_string(),
                description: "Install apps from curated catalog".to_string(),
                key: Some('a'),
            },
            OverviewAction {
                id: "telemetry".to_string(),
                label: "View Telemetry".to_string(),
                description: "Live CPU, memory, disk, and network metrics".to_string(),
                key: Some('t'),
            },
        ];

        let mut list = ListWidget::new("Quick Actions", actions);
        list.show_help = false;

        let detail = DetailPane::new("System Overview");

        Self {
            actions: list,
            detail,
            show_help: false,
            requested: None,
        }
    }

    /// Index of the quick action bound to a hotkey.
    fn action_index_for_key(&self, pressed: char) -> Option<usize> {
        self.actions
            .items
            .iter()
            .position(|action| action.key == Some(pressed))
    }

    /// Tab a quick action navigates to.
    fn action_target(&self, index: usize) -> Option<ActiveTab> {
        self.actions
            .items
            .get(index)
            .and_then(|action| self.action_target_for_id(&action.id))
    }

    fn action_target_for_id(&self, id: &str) -> Option<ActiveTab> {
        match id {
            "storage" => Some(ActiveTab::Storage),
            "runtime" => Some(ActiveTab::Runtime),
            "catalog" => Some(ActiveTab::Catalog),
            "telemetry" => Some(ActiveTab::Telemetry),
            _ => None,
        }
    }

    fn build_detail(&mut self, state: &AppState) {
        self.detail.clear();
        self.detail.set_title("System Overview");

        // System info section
        self.detail.add_section(crate::tui::widgets::DetailSection {
            title: "System".to_string(),
            expanded: true,
            fields: vec![
                crate::tui::widgets::DetailField {
                    key: "Hostname".to_string(),
                    value: state.config.manifest_path.display().to_string(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Access Level".to_string(),
                    value: match state.access.tier {
                        crate::access::AccessTier::Root => "Root (mutation allowed)".to_string(),
                        crate::access::AccessTier::ReadOnly => "Read-Only".to_string(),
                    },
                    style: Some(match state.access.tier {
                        crate::access::AccessTier::Root => Style::default().fg(Color::Green),
                        crate::access::AccessTier::ReadOnly => Style::default().fg(Color::Yellow),
                    }),
                },
            ],
        });

        // Discovery summary
        if let Some(discovery) = &state.discovery {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Discovery".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Network Interfaces".to_string(),
                        value: discovery.network.interfaces.len().to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "ZFS Present".to_string(),
                        value: if discovery.storage.zfs.present {
                            "Yes"
                        } else {
                            "No"
                        }
                        .to_string(),
                        style: Some(if discovery.storage.zfs.present {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        }),
                    },
                    crate::tui::widgets::DetailField {
                        key: "Btrfs Present".to_string(),
                        value: if discovery.storage.btrfs.present {
                            "Yes"
                        } else {
                            "No"
                        }
                        .to_string(),
                        style: Some(if discovery.storage.btrfs.present {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        }),
                    },
                    crate::tui::widgets::DetailField {
                        key: "Mount Points".to_string(),
                        value: discovery.storage.mounts.len().to_string(),
                        style: None,
                    },
                ],
            });
        }

        // Storage summary
        if let Some(storage) = &state.storage {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Storage".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "ZFS Pools".to_string(),
                        value: storage.zfs_pools.len().to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "ZFS Datasets".to_string(),
                        value: storage.zfs_datasets.len().to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Btrfs Subvolumes".to_string(),
                        value: storage.btrfs_subvolumes.len().to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "LVM Volumes".to_string(),
                        value: storage.lvm_volumes.len().to_string(),
                        style: None,
                    },
                ],
            });
        }

        // Runtime summary
        if let Some(runtime) = &state.runtime {
            let container_count: usize = runtime
                .container_runtimes
                .iter()
                .map(|r| r.containers.len())
                .sum();
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Runtime".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Init System".to_string(),
                        value: format!(
                            "{} ({})",
                            runtime.init.kind,
                            if runtime.init.detected {
                                "detected"
                            } else {
                                "not found"
                            }
                        ),
                        style: Some(if runtime.init.detected {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Red)
                        }),
                    },
                    crate::tui::widgets::DetailField {
                        key: "Container Runtimes".to_string(),
                        value: runtime.container_runtimes.len().to_string(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Total Containers".to_string(),
                        value: container_count.to_string(),
                        style: None,
                    },
                ],
            });
        }

        // Telemetry summary
        if let Some(telemetry) = &state.telemetry {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Telemetry".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "CPU User".to_string(),
                        value: format!("{}%", telemetry.cpu.user),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "CPU System".to_string(),
                        value: format!("{}%", telemetry.cpu.system),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "CPU Idle".to_string(),
                        value: format!("{}%", telemetry.cpu.idle),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Memory Used".to_string(),
                        value: format_bytes(telemetry.memory.used_bytes),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Memory Total".to_string(),
                        value: format_bytes(telemetry.memory.total_bytes),
                        style: None,
                    },
                ],
            });
        }
    }
}

impl Tab for OverviewTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Overview
    }

    fn title(&self) -> &'static str {
        "Overview"
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut AppState) -> bool {
        // Hotkeys select and run a quick action directly.
        if let crossterm::event::KeyCode::Char(pressed) = key.code
            && let Some(index) = self.action_index_for_key(pressed)
        {
            self.actions.state.list_state.select(Some(index));
            self.requested = self.action_target(index);
            return true;
        }

        // Handle action list keys
        if self.actions.handle_key(key) {
            self.requested = None;
            if let Some(selected) = self.actions.selected() {
                self.requested = self.action_target_for_id(&selected.id);
            }
            return true;
        }

        // Handle detail pane keys
        if self.detail.handle_key(key) {
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

    fn render(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        self.build_detail(state);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 8 } else { 0 }),
            ])
            .split(chunks[0]);

        self.actions.render(frame, left_chunks[0]);
        self.detail.render(frame, chunks[1]);

        if self.show_help {
            let help = Paragraph::new(
                "Overview Help:\n\
                j/k - Navigate actions\n\
                Enter - Select action\n\
                d/s/c/a/p - Quick keys for actions\n\
                Detail pane: j/k scroll, Ctrl+d/u page, Space toggle section\n\
                ? - Toggle this help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[1]);
        }
    }

    fn update(&mut self, state: &AppState) {
        // Keep detail pane in sync
        self.build_detail(state);
    }

    fn requested_tab(&self) -> Option<ActiveTab> {
        self.requested
    }
}

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
