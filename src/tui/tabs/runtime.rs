//! Runtime tab - interactive container and service management.

use crate::app::state::AppState;
use crate::runtime::controller::ControllerReceipt;
use crate::runtime::mutation::{
    DockerBackend, MutationBackend, MutationTarget, MutationVerb, PodmanBackend, RuntimeKind,
};
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{
    Action, ActionMenu, ConfirmDialog, DetailPane, ListWidget, ProgressOverlay, SelectableItem,
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
    container_name: Option<String>,
    running: bool,
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
    progress: ProgressOverlay,
    pending: Option<PendingMutation>,
    last_receipt: Option<ControllerReceipt>,
    show_help: bool,
}

/// A mutation waiting for user confirmation.
#[derive(Debug, Clone)]
struct PendingMutation {
    verb: MutationVerb,
    label: String,
    command: String,
}

impl RuntimeTab {
    pub fn new() -> Self {
        let items = ListWidget::new("Runtime", Vec::new())
            .with_empty_message("No runtime data. Run discovery to populate.");
        let detail = DetailPane::new("Runtime Details");
        let action_menu = ActionMenu::new("Container Actions", Vec::new());
        let confirm_dialog =
            ConfirmDialog::new("Confirm", "Are you sure?").with_labels("Yes", "No");
        let progress = ProgressOverlay::new("Container Action");

        Self {
            items,
            detail,
            action_menu,
            confirm_dialog,
            progress,
            pending: None,
            last_receipt: None,
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
                    container_name: None,
                    running: false,
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
                        container_name: Some(container.name.clone()),
                        running: container.status.to_lowercase().contains("up"),
                    });
                }
            }

            // Init services would go here if we had them
        }

        self.items.set_items_preserving_selection(all_items);
    }

    fn build_detail(&mut self, state: &AppState) {
        self.detail.clear();

        if let Some(receipt) = &self.last_receipt {
            self.detail.set_title("Last Action");
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Receipt".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Action".to_string(),
                        value: receipt.plan.summary.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Command".to_string(),
                        value: receipt.plan.command_line(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Executed".to_string(),
                        value: if receipt.execution.executed {
                            "yes"
                        } else {
                            "no (dry-run)"
                        }
                        .to_string(),
                        style: Some(if receipt.execution.executed {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Yellow)
                        }),
                    },
                    crate::tui::widgets::DetailField {
                        key: "Exit Code".to_string(),
                        value: receipt
                            .execution
                            .exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "n/a".to_string()),
                        style: None,
                    },
                ],
            });

            if let Some(verification) = &receipt.verification {
                self.detail.add_section(crate::tui::widgets::DetailSection {
                    title: "Verification".to_string(),
                    expanded: true,
                    fields: vec![
                        crate::tui::widgets::DetailField {
                            key: "Command".to_string(),
                            value: verification.command.clone(),
                            style: None,
                        },
                        crate::tui::widgets::DetailField {
                            key: "Result".to_string(),
                            value: if verification.success {
                                "ok".to_string()
                            } else {
                                "failed".to_string()
                            },
                            style: Some(if verification.success {
                                Style::default().fg(Color::Green)
                            } else {
                                Style::default().fg(Color::Red)
                            }),
                        },
                    ],
                });
            }
        } else if let Some(selected) = self.items.selected() {
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
                    if selected.running {
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
                    } else {
                        actions.push(Action {
                            label: "Start".to_string(),
                            description: "Start the container".to_string(),
                            key: Some('s'),
                            enabled: true,
                            dangerous: false,
                        });
                    }
                }
                RuntimeItemKind::Runtime => {}
            }

            if !actions.is_empty() {
                self.action_menu = ActionMenu::new(format!("Actions: {}", selected.title), actions);
            }
        }
    }

    /// Resolve the mutation backend and target for the selected container.
    fn mutation_for(&self) -> Option<(&'static dyn MutationBackend, MutationTarget)> {
        let selected = self.items.selected()?;
        let name = selected.container_name.clone()?;
        let runtime = match selected.runtime_name.as_str() {
            "docker" => RuntimeKind::Docker,
            "podman" => RuntimeKind::Podman,
            _ => return None,
        };
        let backend: &'static dyn MutationBackend = match runtime {
            RuntimeKind::Docker => &DockerBackend,
            RuntimeKind::Podman => &PodmanBackend,
        };
        Some((backend, MutationTarget::Container { runtime, name }))
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
        // Overlays are handled by the tab manager before the tab sees a key.
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

    fn progress_overlay(&mut self) -> Option<&mut ProgressOverlay> {
        Some(&mut self.progress)
    }

    fn handle_menu_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        state: &mut AppState,
    ) {
        let crate::tui::widgets::DialogResult::Selected(index) = result else {
            return;
        };

        let Some(action) = self.action_menu.actions.get(index) else {
            return;
        };
        let label = action.label.clone();

        let verb = match label.as_str() {
            "Start" => MutationVerb::Start,
            "Stop" => MutationVerb::Stop,
            "Restart" => MutationVerb::Restart,
            _ => return,
        };

        let Some((backend, target)) = self.mutation_for() else {
            return;
        };

        // Plan first so the confirmation shows the exact command that will run.
        let plan = match state.plan_runtime_mutation(backend, &target, &verb) {
            Ok(plan) => plan,
            Err(error) => {
                self.progress.start(format!("{} failed", label));
                self.progress.error(format!("{error}"));
                return;
            }
        };

        let command = plan.command_line();
        self.pending = Some(PendingMutation {
            verb,
            label: label.clone(),
            command: command.clone(),
        });
        self.confirm_dialog = ConfirmDialog::new(
            format!("Confirm {label}"),
            format!("Run: {command}\n\nContinue?"),
        )
        .with_labels("Run", "Cancel");
        self.confirm_dialog.show();
    }

    fn handle_dialog_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        state: &mut AppState,
    ) {
        if !matches!(result, crate::tui::widgets::DialogResult::Confirmed) {
            self.pending = None;
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        let Some((backend, target)) = self.mutation_for() else {
            return;
        };

        self.progress.start(format!("{}...", pending.label));
        self.progress.set_cancellable(false);
        self.progress
            .add_detail(format!("Command: {}", pending.command));

        match state.execute_runtime_mutation(backend, &target, &pending.verb) {
            Ok(receipt) => {
                let executed = receipt.execution.executed;
                self.progress.set_progress(100);
                if executed {
                    self.progress
                        .success(format!("{} completed", pending.label));
                } else {
                    self.progress.success(format!(
                        "{} planned (dry-run): {}",
                        pending.label, pending.command
                    ));
                }
                if let Some(verification) = &receipt.verification {
                    self.progress.add_detail(format!(
                        "Verification: {} ({})",
                        if verification.success { "ok" } else { "failed" },
                        verification.command
                    ));
                }
                self.last_receipt = Some(receipt);
            }
            Err(error) => {
                self.progress
                    .error(format!("{} failed: {error}", pending.label));
            }
        }
    }

    fn update(&mut self, state: &AppState) {
        self.rebuild_items(state);
        self.build_detail(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessContext, AccessTier};
    use crate::config::Config;
    use crate::runtime::controller::MutationController;
    use crate::runtime::model::{ContainerRuntimeReport, ContainerSummary, RuntimeInspection};
    use crate::runtime::mutation::MutationMode;
    use crate::tui::widgets::DialogResult;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use std::sync::{Arc, Mutex};

    /// Records every command line the mutation stack executes.
    struct RecordingRunner {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl crate::platform::CommandRunner for RecordingRunner {
        fn run(
            &self,
            program: &std::path::Path,
            args: &[&str],
        ) -> Result<crate::platform::CommandOutput, std::io::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{} {}", program.display(), args.join(" ")));
            Ok(crate::platform::CommandOutput {
                status: Some(0),
                stdout: "true".to_string(),
                stderr: String::new(),
            })
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    /// A state with one stopped docker container named "web".
    fn state_with_container(runner: Arc<RecordingRunner>) -> AppState {
        let access = AccessContext {
            tier: AccessTier::Root,
        };
        let mut state = AppState::bootstrap(Config::default(), access, Vec::new());
        state.mutation_controller = Some(MutationController::new(
            access,
            MutationMode::Execute,
            runner as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        ));
        state.runtime = Some(RuntimeInspection {
            container_runtimes: vec![ContainerRuntimeReport {
                name: "docker".to_string(),
                socket: "/var/run/docker.sock".to_string(),
                binary: Some("/usr/bin/docker".to_string()),
                daemon_reachable: true,
                daemon_status: Some("running".to_string()),
                containers: vec![ContainerSummary {
                    id: "abc123def456".to_string(),
                    name: "web".to_string(),
                    image: "nginx:latest".to_string(),
                    status: "Exited (0) 2 minutes ago".to_string(),
                }],
            }],
            ..RuntimeInspection::default()
        });
        state
    }

    /// Move the selection from the runtime header to the container entry.
    fn select_container(tab: &mut RuntimeTab) {
        tab.items.handle_key(key(KeyCode::Char('j')));
        tab.build_action_menu();
    }

    #[test]
    fn stopped_container_offers_a_start_action() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_with_container(runner);
        let mut tab = RuntimeTab::new();

        tab.update(&state);
        select_container(&mut tab);

        assert_eq!(tab.action_menu.actions.len(), 1);
        assert_eq!(tab.action_menu.actions[0].label, "Start");
    }

    #[test]
    fn refreshing_the_runtime_keeps_the_selected_container() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_with_container(runner);
        let mut tab = RuntimeTab::new();

        tab.update(&state);
        select_container(&mut tab);
        assert_eq!(tab.items.selected_index(), Some(1));

        // A host refresh rebuilds the list from the shared state.
        tab.update(&state);

        assert_eq!(tab.items.selected_index(), Some(1));
        assert_eq!(
            tab.items
                .selected()
                .and_then(|item| item.container_name.clone()),
            Some("web".to_string())
        );
    }

    #[test]
    fn container_start_routes_through_the_mutation_controller() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_with_container(runner);
        let mut tab = RuntimeTab::new();

        tab.update(&state);
        select_container(&mut tab);

        // Selecting the action plans the mutation and asks for confirmation.
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);

        assert!(tab.confirm_dialog.is_visible());
        assert!(tab.pending.is_some());
        assert!(
            calls.lock().unwrap().is_empty(),
            "nothing runs before confirmation"
        );

        // Confirming executes the planned command.
        tab.handle_dialog_result(DialogResult::Confirmed, &mut state);

        let receipt = tab.last_receipt.expect("a receipt should be recorded");
        assert!(receipt.execution.executed);
        assert_eq!(receipt.plan.command_line(), "docker start web");
        assert_eq!(state.mutation_audit().len(), 1);

        let recorded = calls.lock().unwrap().clone();
        assert_eq!(recorded[0], "docker start web");
        assert!(
            recorded
                .iter()
                .any(|call| call.starts_with("docker inspect")),
            "the mutation should be verified: {recorded:?}"
        );
    }

    #[test]
    fn cancelling_the_confirmation_runs_nothing() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_with_container(runner);
        let mut tab = RuntimeTab::new();

        tab.update(&state);
        select_container(&mut tab);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);

        tab.handle_dialog_result(DialogResult::Cancelled, &mut state);

        assert!(calls.lock().unwrap().is_empty());
        assert!(tab.last_receipt.is_none());
        assert!(state.mutation_audit().is_empty());
        assert!(tab.pending.is_none());
    }

    #[test]
    fn read_only_sessions_cannot_plan_a_mutation() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let access = AccessContext {
            tier: AccessTier::ReadOnly,
        };
        let mut state = AppState::bootstrap(Config::default(), access, Vec::new());
        state.mutation_controller = Some(MutationController::new(
            access,
            MutationMode::Execute,
            runner as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        ));
        state.runtime = state_with_container(Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        }))
        .runtime;

        let mut tab = RuntimeTab::new();
        tab.update(&state);
        select_container(&mut tab);

        tab.handle_menu_result(DialogResult::Selected(0), &mut state);

        assert!(!tab.confirm_dialog.is_visible());
        assert!(tab.pending.is_none());
        assert!(calls.lock().unwrap().is_empty());
    }
}
