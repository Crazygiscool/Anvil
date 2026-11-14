//! Catalog tab - install applications from the real catalog index.
//!
//! The tab shows the entries of the catalog index that lives on disk
//! (`/etc/anvil/catalog.toml` or `$XDG_CONFIG_HOME/anvil/catalog.toml`
//! merged on top of the built-in defaults). The installed state of an entry
//! is derived from the runtime inspection, never from a local flag, and every
//! install or removal is routed through the mutation controller so the
//! privilege gate, the receipt, the verification and the rollback plan all
//! apply.

use crate::app::state::AppState;
use crate::catalog::model::{CatalogEntry, CatalogIndex};
use crate::catalog::{backend_for, install_target, runtime_kind};
use crate::runtime::controller::ControllerReceipt;
use crate::runtime::mutation::{MutationBackend, MutationMode, MutationTarget, MutationVerb};
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{
    Action, ActionMenu, BuildCache, ConfirmDialog, DetailPane, DialogResult, ListWidget,
    ProgressOverlay, SelectableItem,
};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// The label of the pseudo-category that shows every entry.
const ALL_CATEGORIES: &str = "All";

/// One catalog entry as shown in the list.
#[derive(Debug, Clone)]
struct CatalogApp {
    entry: CatalogEntry,
    /// Whether a container with this entry's id exists on the host.
    ///
    /// This is derived from the runtime inspection on every rebuild; it is
    /// never a local flag that the tab flips on itself.
    installed: bool,
    /// The runtime the container was found in, when it is installed.
    installed_runtime: Option<String>,
}

impl SelectableItem for CatalogApp {
    fn id(&self) -> String {
        self.entry.id.clone()
    }

    fn title(&self) -> String {
        let status = if self.installed { " ✓" } else { "" };
        format!("{}{}", self.entry.name, status)
    }

    fn subtitle(&self) -> Option<String> {
        Some(format!(
            "{} | {} | {}",
            self.entry.category, self.entry.runtime, self.entry.image
        ))
    }

    fn prefix(&self) -> Option<&str> {
        Some(match self.entry.category.as_str() {
            "management" => "⚙️",
            "media" => "🎬",
            "devops" => "🔧",
            "database" => "🗄️",
            "networking" => "🌐",
            "monitoring" => "📊",
            "backup" => "💾",
            _ => "📦",
        })
    }

    fn style(&self) -> Option<Style> {
        if self.installed {
            Some(Style::default().fg(Color::Green))
        } else {
            None
        }
    }

    fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.entry.name, self.entry.description, self.entry.category, self.entry.image
        )
    }
}

/// A catalog mutation waiting for the user's confirmation.
#[derive(Clone)]
struct PendingMutation {
    verb: MutationVerb,
    label: String,
    command: String,
    backend: &'static dyn MutationBackend,
    target: MutationTarget,
}

pub struct CatalogTab {
    apps: ListWidget<CatalogApp>,
    detail: DetailPane,
    action_menu: ActionMenu,
    confirm_dialog: ConfirmDialog,
    progress: ProgressOverlay,
    /// The loaded catalog index.
    index: Option<CatalogIndex>,
    /// Why the index could not be loaded, if it could not.
    index_error: Option<String>,
    /// The category filter: `All` plus the categories present in the index.
    categories: Vec<String>,
    selected_category: usize,
    pending: Option<PendingMutation>,
    last_receipt: Option<ControllerReceipt>,
    show_help: bool,
    /// What has already been built, so a keystroke does not rebuild the
    /// whole list.
    cache: BuildCache<String>,
}

impl CatalogTab {
    pub fn new() -> Self {
        let apps = ListWidget::new("App Catalog", Vec::new())
            .with_empty_message("No apps in the catalog.");
        let detail = DetailPane::new("App Details");
        let action_menu = ActionMenu::new("App Actions", Vec::new());
        let confirm_dialog =
            ConfirmDialog::new("Confirm", "Are you sure?").with_labels("Run", "Cancel");
        let progress = ProgressOverlay::new("Catalog Action");

        Self {
            apps,
            detail,
            action_menu,
            confirm_dialog,
            progress,
            index: None,
            index_error: None,
            categories: vec![ALL_CATEGORIES.to_string()],
            selected_category: 0,
            pending: None,
            last_receipt: None,
            show_help: false,
            cache: BuildCache::default(),
        }
    }

    /// Load the catalog index from the standard locations.
    ///
    /// A broken index file is reported instead of being silently ignored: the
    /// tab shows the error rather than pretending the catalog is empty.
    fn load_index(&mut self) {
        match crate::catalog::load_index() {
            Ok(index) => {
                self.index_error = None;
                self.categories = Self::category_filter(&index);
                if self.selected_category >= self.categories.len() {
                    self.selected_category = 0;
                }
                self.index = Some(index);
                self.apps.empty_message = "No apps in the catalog.".to_string();
            }
            Err(error) => {
                self.index = None;
                self.index_error = Some(error.to_string());
                self.categories = vec![ALL_CATEGORIES.to_string()];
                self.selected_category = 0;
                self.apps.empty_message = format!("The catalog index could not be loaded: {error}");
                self.apps.set_items(Vec::new());
            }
        }
    }

    /// The category filter options: `All` plus the categories of the index.
    fn category_filter(index: &CatalogIndex) -> Vec<String> {
        let mut categories = vec![ALL_CATEGORIES.to_string()];
        categories.extend(index.categories());
        categories
    }

    /// The category that is currently selected.
    fn selected_category_name(&self) -> &str {
        self.categories
            .get(self.selected_category)
            .map(String::as_str)
            .unwrap_or(ALL_CATEGORIES)
    }

    /// Rebuild the list from the index and the runtime inspection.
    ///
    /// The installed state comes from the containers the runtime inspection
    /// found, so the list always reflects the host rather than what this tab
    /// did earlier.
    fn rebuild_items(&mut self, state: &AppState) {
        let Some(index) = &self.index else {
            return;
        };

        let category = self.selected_category_name().to_string();
        let entries: Vec<CatalogEntry> = index
            .apps
            .iter()
            .filter(|entry| category == ALL_CATEGORIES || entry.category == category)
            .cloned()
            .collect();

        let apps = entries
            .into_iter()
            .map(|entry| {
                let (installed, installed_runtime) = installed_state(&entry, state);
                CatalogApp {
                    entry,
                    installed,
                    installed_runtime,
                }
            })
            .collect();

        self.apps.set_items_preserving_selection(apps);
    }

    fn build_detail(&mut self, state: &AppState) {
        self.detail.clear();

        if let Some(error) = &self.index_error {
            self.detail.set_title("Catalog Unavailable");
            self.detail.push_field("Error", error.clone());
            self.detail.push_field(
                "Index",
                crate::catalog::default_index_path().display().to_string(),
            );
            return;
        }

        let Some(app) = self.apps.selected().cloned() else {
            self.detail.set_title("App Details");
            self.detail.push_field("Select an app", "to view details");
            return;
        };

        self.detail.set_title(app.entry.name.clone());
        self.detail.add_section(crate::tui::widgets::DetailSection {
            title: "Overview".to_string(),
            expanded: true,
            fields: vec![
                crate::tui::widgets::DetailField {
                    key: "Id".to_string(),
                    value: app.entry.id.clone(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Category".to_string(),
                    value: app.entry.category.clone(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Image".to_string(),
                    value: app.entry.image.clone(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Runtime".to_string(),
                    value: app.entry.runtime.clone(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Restart".to_string(),
                    value: app.entry.restart_policy.clone(),
                    style: None,
                },
                crate::tui::widgets::DetailField {
                    key: "Installed".to_string(),
                    value: match (&app.installed, &app.installed_runtime) {
                        (true, Some(runtime)) => format!("yes ({runtime})"),
                        (true, None) => "yes".to_string(),
                        (false, _) => "no".to_string(),
                    },
                    style: Some(if app.installed {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
                },
            ],
        });

        if !app.entry.description.is_empty() {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Description".to_string(),
                expanded: true,
                fields: vec![crate::tui::widgets::DetailField {
                    key: String::new(),
                    value: app.entry.description.clone(),
                    style: None,
                }],
            });
        }

        if !app.entry.ports.is_empty() {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Ports".to_string(),
                expanded: true,
                fields: app
                    .entry
                    .ports
                    .iter()
                    .map(|port| crate::tui::widgets::DetailField {
                        key: "Port".to_string(),
                        value: format!("{}:{}/{}", port.host, port.container, port.protocol),
                        style: None,
                    })
                    .collect(),
            });
        }

        if !app.entry.volumes.is_empty() {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Volumes".to_string(),
                expanded: true,
                fields: app
                    .entry
                    .volumes
                    .iter()
                    .map(|volume| crate::tui::widgets::DetailField {
                        key: volume.kind.clone(),
                        value: format!("{} -> {}", volume.source, volume.target),
                        style: None,
                    })
                    .collect(),
            });
        }

        if !app.entry.env.is_empty() {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Environment".to_string(),
                expanded: true,
                fields: app
                    .entry
                    .env
                    .iter()
                    .map(|(key, value)| crate::tui::widgets::DetailField {
                        key: key.clone(),
                        value: value.clone(),
                        style: None,
                    })
                    .collect(),
            });
        }

        if let Some(receipt) = &self.last_receipt {
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Last Action".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Action".to_string(),
                        value: receipt.plan.summary.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Command".to_string(),
                        value: receipt.plan.describe(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Executed".to_string(),
                        value: if receipt.execution.executed {
                            "yes".to_string()
                        } else {
                            "no (dry-run)".to_string()
                        },
                        style: Some(if receipt.execution.executed {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Yellow)
                        }),
                    },
                ],
            });
        }

        let _ = state;
    }

    fn build_action_menu(&mut self, state: &AppState) {
        let Some(app) = self.apps.selected().cloned() else {
            self.action_menu = ActionMenu::new("App Actions", Vec::new());
            return;
        };

        let dry_run = state.mutation_mode() != Some(MutationMode::Execute);
        let action = if app.installed {
            Action {
                label: "Uninstall".to_string(),
                description: if dry_run {
                    "Plan the removal of the container".to_string()
                } else {
                    "Remove the container from the host".to_string()
                },
                key: Some('u'),
                enabled: true,
                dangerous: true,
            }
        } else {
            Action {
                label: "Install".to_string(),
                description: if dry_run {
                    "Plan the deployment of this application".to_string()
                } else {
                    "Deploy this application on the host".to_string()
                },
                key: Some('i'),
                enabled: true,
                dangerous: false,
            }
        };

        self.action_menu = ActionMenu::new(format!("Actions: {}", app.entry.name), vec![action]);
    }

    /// Build the mutation target for an action on the selected entry.
    fn target_for(
        app: &CatalogApp,
        verb: &MutationVerb,
    ) -> crate::error::Result<(&'static dyn MutationBackend, MutationTarget)> {
        let runtime = match verb {
            MutationVerb::Install => app.entry.runtime.clone(),
            _ => app
                .installed_runtime
                .clone()
                .unwrap_or_else(|| app.entry.runtime.clone()),
        };

        let Some(backend) = backend_for(&runtime) else {
            return Err(crate::error::AnvilError::Config(format!(
                "catalog entry '{}': unsupported runtime '{}'",
                app.entry.id, runtime
            )));
        };

        let target = match verb {
            MutationVerb::Install => install_target(&app.entry)?,
            _ => {
                let Some(kind) = runtime_kind(&runtime) else {
                    return Err(crate::error::AnvilError::Config(format!(
                        "catalog entry '{}': unsupported runtime '{}'",
                        app.entry.id, runtime
                    )));
                };
                MutationTarget::Container {
                    runtime: kind,
                    name: app.entry.id.clone(),
                }
            }
        };

        Ok((backend, target))
    }
}

/// Whether a container for the entry exists, and in which runtime.
///
/// The entry's own runtime is preferred; a container with the same name in
/// another runtime is still reported, because the container really exists.
fn installed_state(entry: &CatalogEntry, state: &AppState) -> (bool, Option<String>) {
    let Some(runtime) = &state.runtime else {
        return (false, None);
    };

    let matches = |name: &str| name.trim_start_matches('/') == entry.id;

    for report in &runtime.container_runtimes {
        if report.name == entry.runtime
            && report
                .containers
                .iter()
                .any(|container| matches(&container.name))
        {
            return (true, Some(report.name.clone()));
        }
    }

    for report in &runtime.container_runtimes {
        if report
            .containers
            .iter()
            .any(|container| matches(&container.name))
        {
            return (true, Some(report.name.clone()));
        }
    }

    (false, None)
}

impl Tab for CatalogTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Catalog
    }

    fn title(&self) -> &'static str {
        "Catalog"
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut AppState) -> bool {
        // Overlays are handled by the tab manager before the tab sees a key.
        if self.apps.handle_key(key) {
            // The detail pane and the action menu follow the selection in
            // update().
            return true;
        }

        if self.detail.handle_key(key) {
            return true;
        }

        match key.code {
            crossterm::event::KeyCode::Char('a') => {
                if self.apps.selected().is_some() {
                    self.action_menu.show();
                }
                true
            }
            crossterm::event::KeyCode::Char('c') => {
                // Cycle category
                if !self.categories.is_empty() {
                    self.selected_category = (self.selected_category + 1) % self.categories.len();
                }
                // The rows show another category now.
                self.cache.invalidate();
                true
            }
            crossterm::event::KeyCode::Char('r') => {
                self.load_index();
                self.cache.invalidate();
                true
            }
            crossterm::event::KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 12 } else { 0 }),
            ])
            .split(chunks[0]);

        // Category bar
        let cat_str = self
            .categories
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == self.selected_category {
                    format!("[{c}]")
                } else {
                    c.clone()
                }
            })
            .collect::<Vec<String>>()
            .join(" | ");
        let cat_bar = Paragraph::new(format!("Category: {} (c: cycle)", cat_str))
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(cat_bar, left_chunks[0]);

        self.apps.render(frame, left_chunks[1]);
        self.detail.render(frame, chunks[1]);

        if self.show_help {
            let help = Paragraph::new(
                "Catalog Help:\n\
                j/k - Navigate apps\n\
                Enter - Select app\n\
                a - Show actions for selected app\n\
                c - Cycle category filter\n\
                r - Reload the catalog index\n\
                / - Search apps\n\
                Ctrl+s - Sort\n\
                Detail: j/k scroll, Ctrl+d/u page\n\
                ? - Toggle help\n\
                Installs need root and ANVIL_ALLOW_MUTATION=1.",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[2]);
        }

        if self.progress.is_visible() {
            self.progress.render(frame, area);
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

    fn update(&mut self, state: &AppState) {
        if self.index.is_none() && self.index_error.is_none() {
            self.load_index();
            self.cache.invalidate();
        }
        if self
            .cache
            .rows_changed(Some(state.host_generation), None, None)
        {
            self.rebuild_items(state);
        }
        if self
            .cache
            .detail_changed(self.apps.selected().map(|item| item.id()))
        {
            self.build_detail(state);
            self.build_action_menu(state);
        }
    }

    fn handle_menu_result(&mut self, result: DialogResult, state: &mut AppState) {
        let DialogResult::Selected(index) = result else {
            return;
        };

        let Some(action) = self.action_menu.actions.get(index) else {
            return;
        };

        let verb = match action.label.as_str() {
            "Install" => MutationVerb::Install,
            "Uninstall" => MutationVerb::Remove,
            _ => return,
        };

        let Some(app) = self.apps.selected().cloned() else {
            return;
        };

        let (backend, target) = match Self::target_for(&app, &verb) {
            Ok(resolved) => resolved,
            Err(error) => {
                self.progress.start(format!("{} failed", action.label));
                self.progress.error(error.to_string());
                return;
            }
        };

        // Plan through the controller so the privilege gate applies before
        // anything is shown as runnable.
        let plan = match state.plan_runtime_mutation(backend, &target, &verb) {
            Ok(plan) => plan,
            Err(error) => {
                self.progress.start(format!("{} failed", action.label));
                self.progress.error(error.to_string());
                return;
            }
        };

        let command = plan.describe();
        let dry_run = state.mutation_mode() != Some(MutationMode::Execute);
        let message = if dry_run {
            format!(
                "Run: {command}\n\nThis session only plans; nothing on the host changes.\nContinue?"
            )
        } else {
            format!("Run: {command}\n\nThis changes the host. Continue?")
        };

        self.pending = Some(PendingMutation {
            verb,
            label: action.label.clone(),
            command: command.clone(),
            backend,
            target,
        });

        self.confirm_dialog = ConfirmDialog::new(format!("Confirm {}", action.label), message)
            .with_labels("Run", "Cancel")
            .dangerous(action.dangerous);
        self.confirm_dialog.show();
    }

    fn handle_dialog_result(&mut self, result: DialogResult, state: &mut AppState) {
        if !matches!(result, DialogResult::Confirmed) {
            self.pending = None;
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };

        self.progress.start(format!("{}...", pending.label));
        self.progress.set_cancellable(false);
        self.progress
            .add_detail(format!("Command: {}", pending.command));

        match state.execute_runtime_mutation(pending.backend, &pending.target, &pending.verb) {
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
                if let Some(rollback) = &receipt.rollback {
                    self.progress
                        .add_detail(format!("Rollback: {}", rollback.command_line()));
                }
                self.last_receipt = Some(receipt);
                // The receipt is part of the detail pane.
                self.cache.invalidate_detail();
            }
            Err(error) => {
                self.progress
                    .error(format!("{} failed: {error}", pending.label));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessContext, AccessTier};
    use crate::config::Config;
    use crate::runtime::controller::MutationController;
    use crate::runtime::model::{ContainerRuntimeReport, ContainerSummary, RuntimeInspection};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::BTreeMap;
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

    fn entry(id: &str, category: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            name: id.to_string(),
            description: format!("the {id} app"),
            category: category.to_string(),
            image: format!("{id}:latest"),
            runtime: "docker".to_string(),
            restart_policy: "unless-stopped".to_string(),
            ports: Vec::new(),
            volumes: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    fn test_index() -> CatalogIndex {
        CatalogIndex::new(vec![
            entry("nginx", "networking"),
            entry("redis", "database"),
        ])
    }

    /// A tab that already holds a known index.
    fn tab_with_index() -> CatalogTab {
        let mut tab = CatalogTab::new();
        tab.index = Some(test_index());
        tab.categories = CatalogTab::category_filter(&test_index());
        tab
    }

    /// A state with a root controller and one running docker container.
    fn state_with_container(runner: Arc<RecordingRunner>, container: &str) -> AppState {
        let access = AccessContext {
            tier: AccessTier::Root,
        };
        let mut state = AppState::bootstrap(Config::default(), access);
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
                    name: container.to_string(),
                    image: "nginx:latest".to_string(),
                    status: "Up 2 minutes".to_string(),
                }],
            }],
            ..RuntimeInspection::default()
        });
        state
    }

    fn state_without_containers(runner: Arc<RecordingRunner>) -> AppState {
        let access = AccessContext {
            tier: AccessTier::Root,
        };
        let mut state = AppState::bootstrap(Config::default(), access);
        state.mutation_controller = Some(MutationController::new(
            access,
            MutationMode::Execute,
            runner as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        ));
        state.runtime = Some(RuntimeInspection::default());
        state
    }

    #[test]
    fn the_tab_lists_the_index_entries() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);

        assert_eq!(tab.apps.total_count(), 2);
        assert_eq!(tab.apps.items[0].id(), "nginx".to_string());
    }

    #[test]
    fn the_category_filter_narrows_the_list() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);
        assert_eq!(tab.apps.total_count(), 2);

        tab.selected_category = tab
            .categories
            .iter()
            .position(|category| category == "database")
            .expect("the database category exists");
        tab.rebuild_items(&state);

        assert_eq!(tab.apps.total_count(), 1);
        assert_eq!(tab.apps.items[0].id(), "redis".to_string());
    }

    #[test]
    fn the_installed_state_comes_from_the_runtime_inspection() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_with_container(runner, "nginx");
        let mut tab = tab_with_index();

        tab.update(&state);

        let nginx = tab
            .apps
            .items
            .iter()
            .find(|app| app.id() == "nginx")
            .expect("nginx is in the catalog");
        assert!(nginx.installed);
        assert_eq!(nginx.installed_runtime.as_deref(), Some("docker"));

        let redis = tab
            .apps
            .items
            .iter()
            .find(|app| app.id() == "redis")
            .expect("redis is in the catalog");
        assert!(!redis.installed);
        assert!(redis.installed_runtime.is_none());
    }

    #[test]
    fn an_installed_app_offers_an_uninstall_action() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_with_container(runner, "nginx");
        let mut tab = tab_with_index();

        tab.update(&state);

        assert_eq!(tab.action_menu.actions.len(), 1);
        assert_eq!(tab.action_menu.actions[0].label, "Uninstall");
        assert!(tab.action_menu.actions[0].dangerous);
    }

    #[test]
    fn an_app_that_is_not_installed_offers_an_install_action() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);

        assert_eq!(tab.action_menu.actions.len(), 1);
        assert_eq!(tab.action_menu.actions[0].label, "Install");
        assert!(!tab.action_menu.actions[0].dangerous);
    }

    #[test]
    fn an_install_routes_through_the_mutation_controller() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);

        assert!(tab.confirm_dialog.is_visible());
        assert!(tab.pending.is_some());
        assert!(
            calls.lock().unwrap().is_empty(),
            "nothing runs before the confirmation"
        );

        tab.handle_dialog_result(DialogResult::Confirmed, &mut state);

        let receipt = tab.last_receipt.expect("a receipt should be recorded");
        assert!(receipt.execution.executed);
        assert_eq!(
            receipt.plan.command_line(),
            "docker run -d --name nginx --restart unless-stopped nginx:latest"
        );
        assert_eq!(state.mutation_audit().len(), 1);

        let recorded = calls.lock().unwrap().clone();
        assert_eq!(
            recorded[0],
            "docker run -d --name nginx --restart unless-stopped nginx:latest"
        );
        assert!(
            recorded
                .iter()
                .any(|call| call.starts_with("docker inspect")),
            "the mutation should be verified: {recorded:?}"
        );
    }

    #[test]
    fn an_uninstall_removes_the_container() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_with_container(runner, "nginx");
        let mut tab = tab_with_index();

        tab.update(&state);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);
        tab.handle_dialog_result(DialogResult::Confirmed, &mut state);

        let receipt = tab.last_receipt.expect("a receipt should be recorded");
        assert!(receipt.execution.executed);
        assert_eq!(receipt.plan.command_line(), "docker rm -f nginx");
        assert_eq!(state.mutation_audit().len(), 1);

        let recorded = calls.lock().unwrap().clone();
        assert_eq!(recorded[0], "docker rm -f nginx");
    }

    #[test]
    fn cancelling_the_confirmation_runs_nothing() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);
        tab.handle_dialog_result(DialogResult::Cancelled, &mut state);

        assert!(calls.lock().unwrap().is_empty());
        assert!(tab.last_receipt.is_none());
        assert!(state.mutation_audit().is_empty());
        assert!(tab.pending.is_none());
    }

    #[test]
    fn a_dry_run_session_produces_a_dry_run_receipt() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let access = AccessContext {
            tier: AccessTier::Root,
        };
        let mut state = AppState::bootstrap(Config::default(), access);
        state.mutation_controller = Some(MutationController::new(
            access,
            MutationMode::DryRun,
            runner as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        ));
        let mut tab = tab_with_index();

        tab.update(&state);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);
        tab.handle_dialog_result(DialogResult::Confirmed, &mut state);

        let receipt = tab.last_receipt.expect("a receipt should be recorded");
        assert!(!receipt.execution.executed);
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(state.mutation_audit().len(), 1);
    }

    #[test]
    fn a_read_only_session_cannot_plan_an_install() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let access = AccessContext {
            tier: AccessTier::ReadOnly,
        };
        let mut state = AppState::bootstrap(Config::default(), access);
        state.mutation_controller = Some(MutationController::new(
            access,
            MutationMode::Execute,
            runner as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        ));
        let mut tab = tab_with_index();

        tab.update(&state);
        tab.handle_menu_result(DialogResult::Selected(0), &mut state);

        assert!(!tab.confirm_dialog.is_visible());
        assert!(tab.pending.is_none());
        assert!(calls.lock().unwrap().is_empty());
        assert!(tab.progress.is_done());
        assert_eq!(
            tab.progress.state,
            crate::tui::widgets::progress::ProgressState::Error
        );
    }

    #[test]
    fn the_action_key_opens_the_action_menu() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);
        assert!(tab.handle_key(key(KeyCode::Char('a')), &mut state));

        assert!(tab.action_menu.is_visible());
    }

    #[test]
    fn the_category_key_cycles_the_filter() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let mut state = state_without_containers(runner);
        let mut tab = tab_with_index();

        tab.update(&state);
        assert_eq!(tab.selected_category, 0);

        assert!(tab.handle_key(key(KeyCode::Char('c')), &mut state));

        assert_eq!(tab.selected_category, 1);

        // The rows are rebuilt in the next update, as the event loop does.
        tab.update(&state);
        assert_eq!(tab.apps.total_count(), 1);
    }

    #[test]
    fn the_tab_renders_the_catalog_entries() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_with_container(runner, "nginx");
        let mut tab = tab_with_index();
        tab.update(&state);

        let mut terminal =
            Terminal::new(TestBackend::new(90, 24)).expect("the test terminal starts");
        terminal
            .draw(|frame| tab.render(frame, frame.area(), &state))
            .expect("the tab renders");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(text.contains("nginx"), "{text}");
        assert!(text.contains("redis"), "{text}");
        assert!(text.contains("App Catalog"), "{text}");
    }

    #[test]
    fn a_broken_index_is_reported_instead_of_hidden() {
        let mut tab = CatalogTab::new();
        tab.index = None;
        tab.index_error = Some("catalog file is broken".to_string());
        tab.apps.empty_message = "The catalog index could not be loaded".to_string();

        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(RecordingRunner {
            calls: Arc::clone(&calls),
        });
        let state = state_without_containers(runner);

        tab.build_detail(&state);

        assert!(tab.apps.items.is_empty());
        assert!(
            tab.detail
                .sections
                .iter()
                .flat_map(|section| section.fields.iter())
                .any(|field| field.value.contains("catalog file is broken")),
            "the error should be visible in the detail pane"
        );
    }
}
