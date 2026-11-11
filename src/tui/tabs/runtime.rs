//! Runtime tab - interactive container and service management.

use crate::app::state::AppState;
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{
    Action, ActionMenu, ConfirmDialog, DetailPane, ListWidget, SelectableItem,
};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[derive(Debug, Clone)]
struct RuntimeItem {
    id: String,
    title: String,
    subtitle: Option<String>,
    kind: RuntimeItemKind,
    runtime_name: String, // docker/podman
    status_style: Option<Style>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeItemKind {
    Runtime,
    Container,
}

impl SelectableItem for RuntimeItem {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn subtitle(&self) -> Option<String> {
        self.subtitle.clone()
    }

    fn prefix(&self) -> Option<&str> {
        match self.kind {
            RuntimeItemKind::Runtime => Some("🐳"),
            RuntimeItemKind::Container => Some("📦"),
        }
    }

    fn style(&self) -> Option<Style> {
        if let Some(status_style) = self.status_style {
            return Some(status_style);
        }
        match self.kind {
            RuntimeItemKind::Runtime => Some(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan),
            ),
            RuntimeItemKind::Container => Some(Style::default().fg(Color::Green)),
        }
    }
}

pub struct RuntimeTab {
    items: ListWidget<RuntimeItem>,
    detail: DetailPane,
    action_menu: ActionMenu,
    confirm_dialog: ConfirmDialog,
    show_help: bool,
}

impl RuntimeTab {
    pub fn new() -> Self {
        let items = ListWidget::new("Runtime", Vec::new())
            .with_empty_message("No runtime data. Run discovery to populate.");
        let detail = DetailPane::new("Runtime Details");
        let action_menu = ActionMenu::new("Container Actions", Vec::new());
        let confirm_dialog =
            ConfirmDialog::new("Confirm", "Are you sure?").with_labels("Yes", "No");

        Self {
            items,
            detail,
            action_menu,
            confirm_dialog,
            show_help: false,
        }
    }

    fn rebuild_items(&mut self, state: &AppState) {
        let mut all_items = Vec::new();

        if let Some(runtime) = &state.runtime {
            for cr in &runtime.container_runtimes {
                // Runtime header
                all_items.push(RuntimeItem {
                    id: format!("runtime-{}", cr.name),
                    title: format!(
                        "{} ({})",
                        cr.name,
                        cr.daemon_status.as_deref().unwrap_or("unknown")
                    ),
                    subtitle: Some(format!(
                        "{} containers, socket: {}",
                        cr.containers.len(),
                        cr.socket
                    )),
                    kind: RuntimeItemKind::Runtime,
                    runtime_name: cr.name.clone(),
                    status_style: None,
                });

                // Containers
                for container in &cr.containers {
                    let status_style = if container.status.to_lowercase().contains("up") {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    };
                    all_items.push(RuntimeItem {
                        id: format!("container-{}-{}", cr.name, container.id),
                        title: format!("{} ({})", container.name, container.image),
                        subtitle: Some(format!(
                            "ID: {} | Status: {}",
                            &container.id[..12.min(container.id.len())],
                            container.status
                        )),
                        kind: RuntimeItemKind::Container,
                        runtime_name: cr.name.clone(),
                        status_style: Some(status_style),
                    });
                }
            }

            // Init services would go here if we had them
        }

        self.items.set_items(all_items);
    }

    fn build_detail(&mut self, state: &AppState) {
        self.detail.clear();

        if let Some(selected) = self.items.selected() {
            self.detail
                .set_title(format!("Details: {}", selected.title));
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Properties".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Type".to_string(),
                        value: format!("{:?}", selected.kind),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Runtime".to_string(),
                        value: selected.runtime_name.clone(),
                        style: None,
                    },
                ],
            });

            if let Some(sub) = &selected.subtitle {
                self.detail.push_field("Info", sub.clone());
            }

            // Get detailed info from state
            if let Some(runtime) = &state.runtime {
                match selected.kind {
                    RuntimeItemKind::Runtime => {
                        if let Some(cr) = runtime
                            .container_runtimes
                            .iter()
                            .find(|r| r.name == selected.runtime_name)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "Runtime Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "Name".to_string(),
                                        value: cr.name.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Socket".to_string(),
                                        value: cr.socket.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Binary".to_string(),
                                        value: cr
                                            .binary
                                            .as_deref()
                                            .unwrap_or("unknown")
                                            .to_string(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Daemon Reachable".to_string(),
                                        value: if cr.daemon_reachable { "Yes" } else { "No" }
                                            .to_string(),
                                        style: Some(if cr.daemon_reachable {
                                            Style::default().fg(Color::Green)
                                        } else {
                                            Style::default().fg(Color::Red)
                                        }),
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Daemon Status".to_string(),
                                        value: cr
                                            .daemon_status
                                            .as_deref()
                                            .unwrap_or("unknown")
                                            .to_string(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Containers".to_string(),
                                        value: cr.containers.len().to_string(),
                                        style: None,
                                    },
                                ],
                            });
                        }
                    }
                    RuntimeItemKind::Container => {
                        if let Some(cr) = runtime
                            .container_runtimes
                            .iter()
                            .find(|r| r.name == selected.runtime_name)
                            && let Some(container) = cr
                                .containers
                                .iter()
                                .find(|c| format!("{} ({})", c.name, c.image) == selected.title)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "Container Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "ID".to_string(),
                                        value: container.id.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Name".to_string(),
                                        value: container.name.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Image".to_string(),
                                        value: container.image.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Status".to_string(),
                                        value: container.status.clone(),
                                        style: Some(
                                            if container.status.to_lowercase().contains("up") {
                                                Style::default().fg(Color::Green)
                                            } else {
                                                Style::default().fg(Color::Red)
                                            },
                                        ),
                                    },
                                ],
                            });
                        }
                    }
                }
            }
        } else {
            self.detail.set_title("Runtime Details");
            self.detail.push_field("Select an item", "to view details");
        }
    }

    fn build_action_menu(&mut self) {
        if let Some(selected) = self.items.selected() {
            let mut actions = Vec::new();

            match selected.kind {
                RuntimeItemKind::Container => {
                    // Parse status to determine available actions
                    let is_running = selected
                        .subtitle
                        .as_ref()
                        .map(|s| s.to_lowercase().contains("up"))
                        .unwrap_or(false);

                    if is_running {
                        actions.push(Action {
                            label: "Stop".to_string(),
                            description: "Stop the container".to_string(),
                            key: Some('s'),
                            enabled: true,
                            dangerous: false,
                        });
                        actions.push(Action {
                            label: "Restart".to_string(),
                            description: "Restart the container".to_string(),
                            key: Some('r'),
                            enabled: true,
                            dangerous: false,
                        });
                        actions.push(Action {
                            label: "View Logs".to_string(),
                            description: "Show container logs".to_string(),
                            key: Some('l'),
                            enabled: true,
                            dangerous: false,
                        });
                        actions.push(Action {
                            label: "Inspect".to_string(),
                            description: "Show detailed container info (JSON)".to_string(),
                            key: Some('i'),
                            enabled: true,
                            dangerous: false,
                        });
                        actions.push(Action {
                            label: "Exec Shell".to_string(),
                            description: "Open shell in container".to_string(),
                            key: Some('e'),
                            enabled: true,
                            dangerous: false,
                        });
                    } else {
                        actions.push(Action {
                            label: "Start".to_string(),
                            description: "Start the container".to_string(),
                            key: Some('s'),
                            enabled: true,
                            dangerous: false,
                        });
                        actions.push(Action {
                            label: "Remove".to_string(),
                            description: "Remove stopped container".to_string(),
                            key: Some('D'),
                            enabled: true,
                            dangerous: true,
                        });
                    }
                }
                RuntimeItemKind::Runtime => {
                    actions.push(Action {
                        label: "Pull Image".to_string(),
                        description: "Pull a new container image".to_string(),
                        key: Some('p'),
                        enabled: true,
                        dangerous: false,
                    });
                    actions.push(Action {
                        label: "List Images".to_string(),
                        description: "Show all local images".to_string(),
                        key: Some('i'),
                        enabled: true,
                        dangerous: false,
                    });
                }
            }

            if !actions.is_empty() {
                self.action_menu = ActionMenu::new(format!("Actions: {}", selected.title), actions);
            }
        }
    }
}

impl Tab for RuntimeTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Runtime
    }

    fn title(&self) -> &'static str {
        "Runtime"
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        // Check action menu
        if self.action_menu.is_visible()
            && let Some(result) = self.action_menu.handle_key(key)
        {
            self.handle_action_result(result, state);
            return true;
        }

        // Check confirm dialog
        if self.confirm_dialog.is_visible()
            && let Some(result) = self.confirm_dialog.handle_key(key)
        {
            self.handle_confirm_result(result, state);
            return true;
        }

        // Handle list keys
        if self.items.handle_key(key) {
            self.build_detail(state);
            self.build_action_menu();
            return true;
        }

        // Handle detail keys
        if self.detail.handle_key(key) {
            return true;
        }

        match key.code {
            crossterm::event::KeyCode::Char('a') => {
                if self.items.selected().is_some() {
                    self.action_menu.show();
                }
                true
            }
            crossterm::event::KeyCode::Char('r') => {
                self.rebuild_items(state);
                true
            }
            crossterm::event::KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        self.rebuild_items(state);
        self.build_detail(state);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 10 } else { 0 }),
            ])
            .split(chunks[0]);

        self.items.render(frame, left_chunks[0]);
        self.detail.render(frame, chunks[1]);

        if self.show_help {
            let help = Paragraph::new(
                "Runtime Help:\n\
                j/k - Navigate items\n\
                Enter - Select item\n\
                a - Show actions for selected container/runtime\n\
                r - Refresh\n\
                / - Filter\n\
                Ctrl+s - Sort\n\
                Detail: j/k scroll, Ctrl+d/u page\n\
                ? - Toggle help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[1]);
        }
    }

    fn action_menu(&mut self) -> Option<&mut ActionMenu> {
        Some(&mut self.action_menu)
    }

    fn confirm_dialog(&mut self) -> Option<&mut ConfirmDialog> {
        Some(&mut self.confirm_dialog)
    }

    fn update(&mut self, state: &AppState) {
        self.rebuild_items(state);
        self.build_detail(state);
    }
}

impl RuntimeTab {
    fn handle_action_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        _state: &mut AppState,
    ) {
        if let crate::tui::widgets::DialogResult::Selected(idx) = result
            && self.items.selected().is_some()
            && idx < self.action_menu.actions.len()
        {
            let action = &self.action_menu.actions[idx];
            if action.dangerous {
                self.confirm_dialog = ConfirmDialog::new(
                    "Confirm Action",
                    format!("{} - This action cannot be undone!", action.label),
                )
                .dangerous(true)
                .with_labels("Yes, proceed", "Cancel");
                self.confirm_dialog.show();
            } else {
                // Execute action - would call mutation backend
            }
        }
    }

    fn handle_confirm_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        _state: &mut AppState,
    ) {
        if let crate::tui::widgets::DialogResult::Confirmed = result {
            // Execute the confirmed action
        }
    }
}
