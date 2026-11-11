//! Catalog tab - 1-click application installation.

use crate::app::state::AppState;
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
struct CatalogApp {
    id: String,
    name: String,
    description: String,
    category: String,
    image: String,
    ports: Vec<String>,
    volumes: Vec<String>,
    installed: bool,
}

impl SelectableItem for CatalogApp {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> String {
        let status = if self.installed { " ✓" } else { "" };
        format!("{}{}", self.name, status)
    }

    fn subtitle(&self) -> Option<String> {
        Some(format!("{} | {}", self.category, self.image))
    }

    fn prefix(&self) -> Option<&str> {
        Some(match self.category.as_str() {
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
        format!("{} {} {}", self.name, self.description, self.category)
    }
}

pub struct CatalogTab {
    apps: ListWidget<CatalogApp>,
    detail: DetailPane,
    action_menu: ActionMenu,
    confirm_dialog: ConfirmDialog,
    progress: ProgressOverlay,
    categories: Vec<String>,
    selected_category: usize,
    show_help: bool,
}

impl CatalogTab {
    pub fn new() -> Self {
        let apps = ListWidget::new("App Catalog", Vec::new())
            .with_empty_message("No apps in the catalog.");
        let detail = DetailPane::new("App Details");
        let action_menu = ActionMenu::new("App Actions", Vec::new());
        let confirm_dialog = ConfirmDialog::new("Confirm Install", "Install this application?")
            .with_labels("Install", "Cancel");
        let progress = ProgressOverlay::new("Installing Application");

        Self {
            apps,
            detail,
            action_menu,
            confirm_dialog,
            progress,
            categories: vec![
                "All".to_string(),
                "management".to_string(),
                "media".to_string(),
                "devops".to_string(),
                "database".to_string(),
                "networking".to_string(),
                "monitoring".to_string(),
                "backup".to_string(),
                "other".to_string(),
            ],
            selected_category: 0,
            show_help: false,
        }
    }

    fn load_catalog(&mut self) {
        // Built-in catalog - in reality this would come from a file or remote index
        let apps = vec![
            CatalogApp {
                id: "portainer".to_string(),
                name: "Portainer".to_string(),
                description: "Container management UI for Docker, Kubernetes, and edge".to_string(),
                category: "management".to_string(),
                image: "portainer/portainer-ce:latest".to_string(),
                ports: vec!["9000:9000".to_string(), "9443:9443".to_string()],
                volumes: vec![
                    "/var/run/docker.sock:/var/run/docker.sock".to_string(),
                    "portainer_data:/data".to_string(),
                ],
                installed: false,
            },
            CatalogApp {
                id: "watchtower".to_string(),
                name: "Watchtower".to_string(),
                description: "Automatically update running Docker containers".to_string(),
                category: "devops".to_string(),
                image: "containrrr/watchtower:latest".to_string(),
                ports: vec![],
                volumes: vec!["/var/run/docker.sock:/var/run/docker.sock".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "pihole".to_string(),
                name: "Pi-hole".to_string(),
                description: "Network-wide ad blocking via DNS".to_string(),
                category: "networking".to_string(),
                image: "pihole/pihole:latest".to_string(),
                ports: vec![
                    "53:53/tcp".to_string(),
                    "53:53/udp".to_string(),
                    "80:80".to_string(),
                ],
                volumes: vec![
                    "pihole_etc:/etc/pihole".to_string(),
                    "pihole_dns:/etc/dnsmasq.d".to_string(),
                ],
                installed: false,
            },
            CatalogApp {
                id: "grafana".to_string(),
                name: "Grafana".to_string(),
                description: "Open-source analytics and monitoring dashboard".to_string(),
                category: "monitoring".to_string(),
                image: "grafana/grafana:latest".to_string(),
                ports: vec!["3000:3000".to_string()],
                volumes: vec!["grafana_data:/var/lib/grafana".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "prometheus".to_string(),
                name: "Prometheus".to_string(),
                description: "Monitoring system and time series database".to_string(),
                category: "monitoring".to_string(),
                image: "prom/prometheus:latest".to_string(),
                ports: vec!["9090:9090".to_string()],
                volumes: vec!["prometheus_data:/prometheus".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "postgres".to_string(),
                name: "PostgreSQL".to_string(),
                description: "Advanced open-source relational database".to_string(),
                category: "database".to_string(),
                image: "postgres:16".to_string(),
                ports: vec!["5432:5432".to_string()],
                volumes: vec!["postgres_data:/var/lib/postgresql/data".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "redis".to_string(),
                name: "Redis".to_string(),
                description: "In-memory data structure store".to_string(),
                category: "database".to_string(),
                image: "redis:7-alpine".to_string(),
                ports: vec!["6379:6379".to_string()],
                volumes: vec!["redis_data:/data".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "jellyfin".to_string(),
                name: "Jellyfin".to_string(),
                description: "Free software media system".to_string(),
                category: "media".to_string(),
                image: "jellyfin/jellyfin:latest".to_string(),
                ports: vec!["8096:8096".to_string()],
                volumes: vec![
                    "jellyfin_config:/config".to_string(),
                    "jellyfin_cache:/cache".to_string(),
                    "media:/media".to_string(),
                ],
                installed: false,
            },
            CatalogApp {
                id: "homeassistant".to_string(),
                name: "Home Assistant".to_string(),
                description: "Open source home automation".to_string(),
                category: "management".to_string(),
                image: "ghcr.io/home-assistant/home-assistant:stable".to_string(),
                ports: vec!["8123:8123".to_string()],
                volumes: vec!["homeassistant_config:/config".to_string()],
                installed: false,
            },
            CatalogApp {
                id: "nginx-proxy-manager".to_string(),
                name: "Nginx Proxy Manager".to_string(),
                description: "Manage Nginx proxy hosts with a simple UI".to_string(),
                category: "networking".to_string(),
                image: "jc21/nginx-proxy-manager:latest".to_string(),
                ports: vec![
                    "80:80".to_string(),
                    "443:443".to_string(),
                    "81:81".to_string(),
                ],
                volumes: vec![
                    "npm_data:/data".to_string(),
                    "npm_letsencrypt:/etc/letsencrypt".to_string(),
                ],
                installed: false,
            },
        ];

        self.apps.set_items(apps);
    }

    fn filter_by_category(&mut self) {
        let category = &self.categories[self.selected_category];
        if category == "All" {
            self.apps.state.filtered_indices = (0..self.apps.items.len()).collect();
        } else {
            self.apps.state.filtered_indices = self
                .apps
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, app)| {
                    if app.category == *category {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
        }
        self.apps.state.list_state.select(Some(0));
    }

    fn build_detail(&mut self) {
        self.detail.clear();

        if let Some(app) = self.apps.selected() {
            self.detail.set_title(app.name.clone());
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Overview".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "Name".to_string(),
                        value: app.name.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Category".to_string(),
                        value: app.category.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Image".to_string(),
                        value: app.image.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Status".to_string(),
                        value: if app.installed {
                            "Installed ✓"
                        } else {
                            "Not Installed"
                        }
                        .to_string(),
                        style: Some(if app.installed {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Yellow)
                        }),
                    },
                ],
            });

            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Description".to_string(),
                expanded: true,
                fields: vec![crate::tui::widgets::DetailField {
                    key: "".to_string(),
                    value: app.description.clone(),
                    style: None,
                }],
            });

            if !app.ports.is_empty() {
                self.detail.add_section(crate::tui::widgets::DetailSection {
                    title: "Ports".to_string(),
                    expanded: true,
                    fields: app
                        .ports
                        .iter()
                        .map(|p| crate::tui::widgets::DetailField {
                            key: "Port".to_string(),
                            value: p.clone(),
                            style: None,
                        })
                        .collect(),
                });
            }

            if !app.volumes.is_empty() {
                self.detail.add_section(crate::tui::widgets::DetailSection {
                    title: "Volumes".to_string(),
                    expanded: true,
                    fields: app
                        .volumes
                        .iter()
                        .map(|v| crate::tui::widgets::DetailField {
                            key: "Volume".to_string(),
                            value: v.clone(),
                            style: None,
                        })
                        .collect(),
                });
            }
        } else {
            self.detail.set_title("App Details");
            self.detail.push_field("Select an app", "to view details");
        }
    }

    fn build_action_menu(&mut self) {
        if let Some(app) = self.apps.selected() {
            let mut actions = Vec::new();

            if app.installed {
                actions.push(Action {
                    label: "Uninstall".to_string(),
                    description: "Preview removal of the application containers".to_string(),
                    key: Some('u'),
                    enabled: true,
                    dangerous: true,
                });
                actions.push(Action {
                    label: "Update".to_string(),
                    description: "Pull latest image and recreate containers".to_string(),
                    key: Some('U'),
                    enabled: true,
                    dangerous: false,
                });
                actions.push(Action {
                    label: "View Logs".to_string(),
                    description: "Show application logs".to_string(),
                    key: Some('l'),
                    enabled: true,
                    dangerous: false,
                });
                actions.push(Action {
                    label: "Backup".to_string(),
                    description: "Backup application volumes".to_string(),
                    key: Some('b'),
                    enabled: true,
                    dangerous: false,
                });
            } else {
                actions.push(Action {
                    label: "Install".to_string(),
                    description: "Preview the deployment of this application".to_string(),
                    key: Some('i'),
                    enabled: true,
                    dangerous: false,
                });
                actions.push(Action {
                    label: "Configure".to_string(),
                    description: "Customize ports, volumes, environment before install".to_string(),
                    key: Some('c'),
                    enabled: true,
                    dangerous: false,
                });
            }

            if !actions.is_empty() {
                self.action_menu = ActionMenu::new(format!("Actions: {}", app.name), actions);
            }
        }
    }

    fn start_install(&mut self, app: &CatalogApp) {
        // Preview only: the catalog backend that executes real installs is not
        // wired up yet, so this must not claim to have installed anything.
        self.progress
            .start(format!("Previewing install of {}...", app.name));
        self.progress.set_cancellable(true);
        self.progress.add_detail(format!("Image: {}", app.image));
        self.progress
            .add_detail(format!("Ports: {}", app.ports.join(", ")));
        self.progress
            .add_detail(format!("Volumes: {}", app.volumes.join(", ")));
        self.progress.set_progress(50);
        self.progress
            .add_detail("No changes were made to the host.".to_string());
        self.progress.set_progress(100);
        self.progress
            .success(format!("Install preview ready for {}", app.name));
    }
}

impl Tab for CatalogTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Catalog
    }

    fn title(&self) -> &'static str {
        "Catalog"
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        // Check progress overlay
        if self.progress.is_visible() && self.progress.handle_key(key) {
            if self.progress.is_done() && !self.progress.is_running() {
                self.progress.hide();
            }
            return true;
        }

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
        if self.apps.handle_key(key) {
            self.build_detail();
            self.build_action_menu();
            return true;
        }

        // Handle detail keys
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
                self.selected_category = (self.selected_category + 1) % self.categories.len();
                self.filter_by_category();
                true
            }
            crossterm::event::KeyCode::Char('r') => {
                self.load_catalog();
                self.filter_by_category();
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
        if self.apps.items.is_empty() {
            self.load_catalog();
            self.filter_by_category();
        }
        self.build_detail();

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
                    format!("[{}]", c)
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
                r - Refresh catalog\n\
                / - Search apps\n\
                Ctrl+s - Sort\n\
                Detail: j/k scroll, Ctrl+d/u page\n\
                ? - Toggle help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[2]);
        }

        // Render progress overlay on top
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

    fn update(&mut self, _state: &AppState) {
        // Keep catalog up to date
    }
}

impl CatalogTab {
    fn handle_action_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        _state: &mut AppState,
    ) {
        if let crate::tui::widgets::DialogResult::Selected(idx) = result
            && let Some(app) = self.apps.selected()
            && idx < self.action_menu.actions.len()
        {
            let action = &self.action_menu.actions[idx];
            match action.label.as_str() {
                "Install" => {
                    self.confirm_dialog = ConfirmDialog::new(
                        "Confirm Install",
                        format!("Install {} ({})?", app.name, app.image),
                    )
                    .with_labels("Install", "Cancel");
                    self.confirm_dialog.show();
                }
                "Uninstall" => {
                    self.confirm_dialog = ConfirmDialog::new(
                        "Confirm Uninstall",
                        format!(
                            "Uninstall {}? This will remove containers and volumes.",
                            app.name
                        ),
                    )
                    .dangerous(true)
                    .with_labels("Uninstall", "Cancel");
                    self.confirm_dialog.show();
                }
                "Update" => {
                    // Start update progress
                    self.progress.start(format!("Updating {}...", app.name));
                    self.progress.set_cancellable(true);
                }
                "Configure" => {
                    // Would show config dialog
                }
                _ => {}
            }
        }
    }

    fn handle_confirm_result(
        &mut self,
        result: crate::tui::widgets::DialogResult,
        _state: &mut AppState,
    ) {
        if let crate::tui::widgets::DialogResult::Confirmed = result {
            // Clone the selected app so the list can be mutated afterwards.
            let Some(app) = self.apps.selected().cloned() else {
                return;
            };

            // Preview only: nothing is installed or removed yet, so the
            // installed flag must keep reflecting the host's real state.
            if app.installed {
                self.progress
                    .start(format!("Previewing uninstall of {}...", app.name));
                self.progress.set_cancellable(true);
                self.progress
                    .add_detail("No changes were made to the host.".to_string());
                self.progress.set_progress(100);
                self.progress
                    .success(format!("Uninstall preview ready for {}", app.name));
            } else {
                self.start_install(&app);
            }
        }
    }
}
