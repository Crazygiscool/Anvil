//! Discovery tab - interactive hardware discovery tree.

use crate::app::state::AppState;
use crate::tui::ActiveTab;
use crate::tui::tabs::Tab;
use crate::tui::widgets::{DetailPane, ListWidget, SelectableItem};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Discovery tree item.
#[derive(Debug, Clone)]
struct DiscoveryItem {
    id: String,
    title: String,
    subtitle: Option<String>,
    kind: DiscoveryKind,
    expanded: bool,
    children: Vec<DiscoveryItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoveryKind {
    Category,
    Device,
    Interface,
    Diagnostic,
}

impl SelectableItem for DiscoveryItem {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> String {
        let prefix = if self.kind == DiscoveryKind::Category {
            if self.expanded { "▼ " } else { "▶ " }
        } else {
            "  "
        };
        format!("{}{}", prefix, self.title)
    }

    fn subtitle(&self) -> Option<String> {
        self.subtitle.clone()
    }

    fn style(&self) -> Option<Style> {
        match self.kind {
            DiscoveryKind::Category => Some(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan),
            ),
            DiscoveryKind::Diagnostic => Some(Style::default().fg(Color::Red)),
            _ => None,
        }
    }

    fn search_text(&self) -> String {
        let mut text = self.title();
        if let Some(sub) = self.subtitle() {
            text.push(' ');
            text.push_str(&sub);
        }
        for child in &self.children {
            text.push(' ');
            text.push_str(&child.search_text());
        }
        text
    }
}

pub struct DiscoveryTab {
    tree: Vec<DiscoveryItem>,
    flat_list: ListWidget<DiscoveryItem>,
    detail: DetailPane,
    selected_id: Option<String>,
    show_help: bool,
}

impl DiscoveryTab {
    pub fn new() -> Self {
        let list = ListWidget::new("Discovery", Vec::new())
            .with_empty_message("No discovery data. Run discovery to populate.");
        let detail = DetailPane::new("Discovery Details");
        Self {
            tree: Vec::new(),
            flat_list: list,
            detail,
            selected_id: None,
            show_help: false,
        }
    }

    fn rebuild_tree(&mut self, state: &AppState) {
        self.tree.clear();

        if let Some(discovery) = &state.discovery {
            // Storage category
            let mut storage_children = Vec::new();
            if discovery.storage.zfs.present {
                storage_children.push(DiscoveryItem {
                    id: "zfs".to_string(),
                    title: "ZFS".to_string(),
                    subtitle: Some("Available".to_string()),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
            } else {
                storage_children.push(DiscoveryItem {
                    id: "zfs".to_string(),
                    title: "ZFS".to_string(),
                    subtitle: Some("Not available".to_string()),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
            }
            if discovery.storage.btrfs.present {
                storage_children.push(DiscoveryItem {
                    id: "btrfs".to_string(),
                    title: "Btrfs".to_string(),
                    subtitle: Some("Available".to_string()),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
            } else {
                storage_children.push(DiscoveryItem {
                    id: "btrfs".to_string(),
                    title: "Btrfs".to_string(),
                    subtitle: Some("Not available".to_string()),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
            }
            for mount in &discovery.storage.mounts {
                storage_children.push(DiscoveryItem {
                    id: format!("mount-{}", mount.mount_point.display()),
                    title: mount.mount_point.display().to_string(),
                    subtitle: Some(format!("{} on {}", mount.fs_type, mount.device)),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
            }
            self.tree.push(DiscoveryItem {
                id: "storage".to_string(),
                title: "Storage".to_string(),
                subtitle: Some(format!("{} items", storage_children.len())),
                kind: DiscoveryKind::Category,
                expanded: true,
                children: storage_children,
            });

            // Network category
            let mut network_children = Vec::new();
            for iface in &discovery.network.interfaces {
                network_children.push(DiscoveryItem {
                    id: format!("net-{}", iface.name),
                    title: iface.name.clone(),
                    subtitle: Some(format!(
                        "{:?} (phys={}, wireless={})",
                        iface.kind, iface.device_present, iface.wireless
                    )),
                    kind: DiscoveryKind::Interface,
                    expanded: false,
                    children: Vec::new(),
                });
            }
            self.tree.push(DiscoveryItem {
                id: "network".to_string(),
                title: "Network".to_string(),
                subtitle: Some(format!("{} interfaces", network_children.len())),
                kind: DiscoveryKind::Category,
                expanded: true,
                children: network_children,
            });

            // Runtime category
            if let Some(runtime) = &state.runtime {
                let mut runtime_children = Vec::new();
                runtime_children.push(DiscoveryItem {
                    id: "init".to_string(),
                    title: format!("Init: {}", runtime.init.kind),
                    subtitle: Some(
                        if runtime.init.detected {
                            "Detected"
                        } else {
                            "Not found"
                        }
                        .to_string(),
                    ),
                    kind: DiscoveryKind::Device,
                    expanded: false,
                    children: Vec::new(),
                });
                for cr in &runtime.container_runtimes {
                    runtime_children.push(DiscoveryItem {
                        id: format!("cr-{}", cr.name),
                        title: format!(
                            "{}: {}",
                            cr.name,
                            cr.daemon_status.as_deref().unwrap_or("unknown")
                        ),
                        subtitle: Some(format!(
                            "{} containers, socket: {}",
                            cr.containers.len(),
                            cr.socket
                        )),
                        kind: DiscoveryKind::Device,
                        expanded: false,
                        children: Vec::new(),
                    });
                }
                self.tree.push(DiscoveryItem {
                    id: "runtime".to_string(),
                    title: "Runtime".to_string(),
                    subtitle: Some(format!("{} items", runtime_children.len())),
                    kind: DiscoveryKind::Category,
                    expanded: true,
                    children: runtime_children,
                });
            }

            // Binaries category
            let mut binary_children = Vec::new();
            if let Some(report) = &state.discovery {
                for bin in &report.binaries {
                    binary_children.push(DiscoveryItem {
                        id: format!("bin-{}", bin.name),
                        title: bin.name.clone(),
                        subtitle: Some(format!(
                            "{} {}",
                            if bin.available { "✓" } else { "✗" },
                            bin.path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "not found".to_string())
                        )),
                        kind: DiscoveryKind::Device,
                        expanded: false,
                        children: Vec::new(),
                    });
                }
            }
            self.tree.push(DiscoveryItem {
                id: "binaries".to_string(),
                title: "Binaries".to_string(),
                subtitle: Some(format!("{} binaries", binary_children.len())),
                kind: DiscoveryKind::Category,
                expanded: false,
                children: binary_children,
            });

            // Diagnostics
            if !discovery.diagnostics.is_empty() {
                let mut diag_children = Vec::new();
                for diag in &discovery.diagnostics {
                    diag_children.push(DiscoveryItem {
                        id: format!("diag-{}", diag.source),
                        title: diag.source.clone(),
                        subtitle: Some(diag.error.clone()),
                        kind: DiscoveryKind::Diagnostic,
                        expanded: false,
                        children: Vec::new(),
                    });
                }
                self.tree.push(DiscoveryItem {
                    id: "diagnostics".to_string(),
                    title: "Diagnostics".to_string(),
                    subtitle: Some(format!("{} issues", diag_children.len())),
                    kind: DiscoveryKind::Category,
                    expanded: true,
                    children: diag_children,
                });
            }
        }

        self.flatten_tree();
    }

    fn flatten_tree(&mut self) {
        let mut flat = Vec::new();
        for item in &self.tree {
            flatten_item(&mut flat, item);
        }
        self.flat_list.set_items(flat);
    }

    fn toggle_expand(&mut self, id: &str) {
        let expanded = match find_item_mut(&mut self.tree, id) {
            Some(item) if item.kind == DiscoveryKind::Category || !item.children.is_empty() => {
                item.expanded = !item.expanded;
                true
            }
            _ => false,
        };
        if expanded {
            self.flatten_tree();
        }
    }

    fn build_detail(&mut self) {
        self.detail.clear();

        // Work on an owned snapshot so the detail pane can be mutated freely.
        let snapshot = self
            .selected_id
            .as_ref()
            .and_then(|id| find_item(&self.tree, id))
            .cloned();

        if let Some(item) = snapshot {
            self.detail.set_title(format!("Details: {}", item.title));
            self.detail.add_section(crate::tui::widgets::DetailSection {
                title: "Properties".to_string(),
                expanded: true,
                fields: vec![
                    crate::tui::widgets::DetailField {
                        key: "ID".to_string(),
                        value: item.id.clone(),
                        style: None,
                    },
                    crate::tui::widgets::DetailField {
                        key: "Type".to_string(),
                        value: format!("{:?}", item.kind),
                        style: None,
                    },
                ],
            });

            if let Some(sub) = &item.subtitle {
                self.detail.push_field("Description", sub.clone());
            }
        } else {
            self.detail.set_title("Discovery Details");
            self.detail.push_field("Select an item", "to view details");
        }
    }
}

/// Flatten a tree node and its expanded children into a list.
fn flatten_item(flat: &mut Vec<DiscoveryItem>, item: &DiscoveryItem) {
    flat.push(item.clone());
    if item.expanded {
        for child in &item.children {
            flatten_item(flat, child);
        }
    }
}

/// Find a mutable node by identifier anywhere in the tree.
fn find_item_mut<'a>(tree: &'a mut [DiscoveryItem], id: &str) -> Option<&'a mut DiscoveryItem> {
    for item in tree.iter_mut() {
        if item.id == id {
            return Some(item);
        }
        if let Some(found) = find_item_mut(&mut item.children, id) {
            return Some(found);
        }
    }
    None
}

/// Find a node by identifier anywhere in the tree.
fn find_item<'a>(tree: &'a [DiscoveryItem], id: &str) -> Option<&'a DiscoveryItem> {
    for item in tree {
        if item.id == id {
            return Some(item);
        }
        if let Some(found) = find_item(&item.children, id) {
            return Some(found);
        }
    }
    None
}

impl Tab for DiscoveryTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Discovery
    }

    fn title(&self) -> &'static str {
        "Discovery"
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        // Rebuild tree if needed
        if self.tree.is_empty() && state.discovery.is_some() {
            self.rebuild_tree(state);
        }

        // Handle list keys
        if self.flat_list.handle_key(key) {
            let selected = self.flat_list.selected().cloned();
            if let Some(selected) = selected {
                self.selected_id = Some(selected.id.clone());
                self.build_detail();
                // Toggle expansion on Enter for categories
                if selected.kind == DiscoveryKind::Category {
                    self.toggle_expand(&selected.id);
                }
            }
            return true;
        }

        // Handle detail keys
        if self.detail.handle_key(key) {
            return true;
        }

        match key.code {
            crossterm::event::KeyCode::Char(' ') => {
                if let Some(id) = self.selected_id.clone() {
                    self.toggle_expand(&id);
                }
                true
            }
            crossterm::event::KeyCode::Char('r') => {
                // Refresh discovery - would need async trigger
                self.rebuild_tree(state);
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
        if self.tree.is_empty() && state.discovery.is_some() {
            self.rebuild_tree(state);
        }
        self.build_detail();

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 8 } else { 0 }),
            ])
            .split(chunks[0]);

        self.flat_list.render(frame, left_chunks[0]);
        self.detail.render(frame, chunks[1]);

        if self.show_help {
            let help = Paragraph::new(
                "Discovery Help:\n\
                j/k - Navigate items\n\
                Enter/Space - Expand/collapse category\n\
                / - Filter items\n\
                Ctrl+s - Sort\n\
                r - Refresh discovery\n\
                Detail: j/k scroll, Ctrl+d/u page\n\
                ? - Toggle help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[1]);
        }
    }

    fn update(&mut self, state: &AppState) {
        if self.tree.is_empty() && state.discovery.is_some() {
            self.rebuild_tree(state);
        }
    }
}
