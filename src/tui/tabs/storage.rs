//! Storage tab - interactive drive, pool, dataset management.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageView {
    Disks,
    Pools,
    Datasets,
    Subvolumes,
    Lvm,
    Mounts,
}

#[derive(Debug, Clone)]
struct StorageItem {
    id: String,
    title: String,
    subtitle: Option<String>,
    kind: StorageItemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageItemKind {
    Disk,
    ZfsPool,
    ZfsDataset,
    BtrfsSubvolume,
    LvmVolume,
    Mount,
}

impl SelectableItem for StorageItem {
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
            StorageItemKind::Disk => Some("💿"),
            StorageItemKind::ZfsPool => Some("🏊"),
            StorageItemKind::ZfsDataset => Some("📁"),
            StorageItemKind::BtrfsSubvolume => Some("🌿"),
            StorageItemKind::LvmVolume => Some("📦"),
            StorageItemKind::Mount => Some("🔗"),
        }
    }

    fn style(&self) -> Option<Style> {
        match self.kind {
            StorageItemKind::Disk => Some(Style::default().fg(Color::Blue)),
            StorageItemKind::ZfsPool => Some(Style::default().fg(Color::Cyan)),
            StorageItemKind::ZfsDataset => Some(Style::default().fg(Color::Green)),
            StorageItemKind::BtrfsSubvolume => Some(Style::default().fg(Color::Magenta)),
            StorageItemKind::LvmVolume => Some(Style::default().fg(Color::Yellow)),
            StorageItemKind::Mount => Some(Style::default().fg(Color::White)),
        }
    }
}

pub struct StorageTab {
    items: ListWidget<StorageItem>,
    detail: DetailPane,
    current_view: StorageView,
    show_help: bool,
}

impl StorageTab {
    pub fn new() -> Self {
        let items = ListWidget::new("Storage", Vec::new())
            .with_empty_message("No storage data. Run discovery to populate.");
        let detail = DetailPane::new("Storage Details");

        Self {
            items,
            detail,
            current_view: StorageView::Disks,
            show_help: false,
        }
    }

    fn rebuild_items(&mut self, state: &AppState) {
        let mut all_items = Vec::new();

        if let Some(storage) = &state.storage {
            match self.current_view {
                StorageView::Disks => {
                    // In a real implementation, we'd get block devices from discovery
                    // For now, show a placeholder
                    all_items.push(StorageItem {
                        id: "disks-info".to_string(),
                        title: "Block Devices".to_string(),
                        subtitle: Some(
                            "Use 'lsblk' for detailed view (not yet implemented)".to_string(),
                        ),
                        kind: StorageItemKind::Disk,
                    });
                }
                StorageView::Pools => {
                    for pool in &storage.zfs_pools {
                        all_items.push(StorageItem {
                            id: format!("zpool-{}", pool.name),
                            title: pool.name.clone(),
                            subtitle: Some(format!(
                                "Health: {} | Size: {}",
                                pool.health, pool.size
                            )),
                            kind: StorageItemKind::ZfsPool,
                        });
                    }
                }
                StorageView::Datasets => {
                    for ds in &storage.zfs_datasets {
                        all_items.push(StorageItem {
                            id: format!("zds-{}", ds.name),
                            title: ds.name.clone(),
                            subtitle: Some(format!(
                                "Mount: {} | Used: {} | Avail: {}",
                                ds.mountpoint, ds.used, ds.available
                            )),
                            kind: StorageItemKind::ZfsDataset,
                        });
                    }
                }
                StorageView::Subvolumes => {
                    for sv in &storage.btrfs_subvolumes {
                        all_items.push(StorageItem {
                            id: format!("btrfs-{}", sv.id),
                            title: format!("Subvolume {}", sv.id),
                            subtitle: Some(format!("Path: {}", sv.path)),
                            kind: StorageItemKind::BtrfsSubvolume,
                        });
                    }
                }
                StorageView::Lvm => {
                    for lv in &storage.lvm_volumes {
                        all_items.push(StorageItem {
                            id: format!("lvm-{}", lv.name),
                            title: lv.name.clone(),
                            subtitle: Some(format!(
                                "VG: {} | Size: {} | Attr: {}",
                                lv.volume_group, lv.size, lv.attributes
                            )),
                            kind: StorageItemKind::LvmVolume,
                        });
                    }
                }
                StorageView::Mounts => {
                    if let Some(discovery) = &state.discovery {
                        for mount in &discovery.storage.mounts {
                            all_items.push(StorageItem {
                                id: format!(
                                    "mount-{}",
                                    mount.mount_point.display().to_string().replace('/', "-")
                                ),
                                title: mount.mount_point.display().to_string(),
                                subtitle: Some(format!("{} on {}", mount.fs_type, mount.device)),
                                kind: StorageItemKind::Mount,
                            });
                        }
                    }
                }
            }
        }

        self.items.set_items_preserving_selection(all_items);
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
                        key: "ID".to_string(),
                        value: selected.id.clone(),
                        style: None,
                    },
                ],
            });

            if let Some(sub) = &selected.subtitle {
                self.detail.push_field("Info", sub.clone());
            }

            // Add detailed fields based on type
            if let Some(storage) = &state.storage {
                match selected.kind {
                    StorageItemKind::ZfsPool => {
                        if let Some(pool) =
                            storage.zfs_pools.iter().find(|p| p.name == selected.title)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "ZFS Pool Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "Name".to_string(),
                                        value: pool.name.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Health".to_string(),
                                        value: pool.health.clone(),
                                        style: Some(if pool.health == "ONLINE" {
                                            Style::default().fg(Color::Green)
                                        } else {
                                            Style::default().fg(Color::Red)
                                        }),
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Size".to_string(),
                                        value: pool.size.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Allocated".to_string(),
                                        value: pool.allocated.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Free".to_string(),
                                        value: pool.free.clone(),
                                        style: None,
                                    },
                                ],
                            });
                        }
                    }
                    StorageItemKind::ZfsDataset => {
                        if let Some(ds) = storage
                            .zfs_datasets
                            .iter()
                            .find(|d| d.name == selected.title)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "ZFS Dataset Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "Name".to_string(),
                                        value: ds.name.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Mountpoint".to_string(),
                                        value: ds.mountpoint.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Used".to_string(),
                                        value: ds.used.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Available".to_string(),
                                        value: ds.available.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Referenced".to_string(),
                                        value: ds.referenced.clone(),
                                        style: None,
                                    },
                                ],
                            });
                        }
                    }
                    StorageItemKind::BtrfsSubvolume => {
                        if let Some(sv) = storage
                            .btrfs_subvolumes
                            .iter()
                            .find(|s| format!("Subvolume {}", s.id) == selected.title)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "Btrfs Subvolume Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "ID".to_string(),
                                        value: sv.id.to_string(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Path".to_string(),
                                        value: sv.path.clone(),
                                        style: None,
                                    },
                                ],
                            });
                        }
                    }
                    StorageItemKind::LvmVolume => {
                        if let Some(lv) = storage
                            .lvm_volumes
                            .iter()
                            .find(|l| l.name == selected.title)
                        {
                            self.detail.add_section(crate::tui::widgets::DetailSection {
                                title: "LVM Volume Details".to_string(),
                                expanded: true,
                                fields: vec![
                                    crate::tui::widgets::DetailField {
                                        key: "Name".to_string(),
                                        value: lv.name.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Volume Group".to_string(),
                                        value: lv.volume_group.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Size".to_string(),
                                        value: lv.size.clone(),
                                        style: None,
                                    },
                                    crate::tui::widgets::DetailField {
                                        key: "Attributes".to_string(),
                                        value: lv.attributes.clone(),
                                        style: None,
                                    },
                                ],
                            });
                        }
                    }
                    _ => {}
                }
            }
        } else {
            self.detail.set_title("Storage Details");
            self.detail.push_field("Select an item", "to view details");
        }
    }

    fn cycle_view(&mut self) {
        self.current_view = match self.current_view {
            StorageView::Disks => StorageView::Pools,
            StorageView::Pools => StorageView::Datasets,
            StorageView::Datasets => StorageView::Subvolumes,
            StorageView::Subvolumes => StorageView::Lvm,
            StorageView::Lvm => StorageView::Mounts,
            StorageView::Mounts => StorageView::Disks,
        };
        // Rebuild will happen in update()
    }
}

impl Tab for StorageTab {
    fn id(&self) -> ActiveTab {
        ActiveTab::Storage
    }

    fn title(&self) -> &'static str {
        "Storage"
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        // Handle list keys
        if self.items.handle_key(key) {
            self.build_detail(state);
            return true;
        }

        // Handle detail keys
        if self.detail.handle_key(key) {
            return true;
        }

        match key.code {
            crossterm::event::KeyCode::Char('v') => {
                self.cycle_view();
                self.rebuild_items(state);
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
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(if self.show_help { 10 } else { 0 }),
            ])
            .split(chunks[0]);

        // View tabs
        let view_titles = match self.current_view {
            StorageView::Disks => "Disks",
            StorageView::Pools => "ZFS Pools",
            StorageView::Datasets => "ZFS Datasets",
            StorageView::Subvolumes => "Btrfs Subvolumes",
            StorageView::Lvm => "LVM Volumes",
            StorageView::Mounts => "Mounts",
        };
        let view_bar = Paragraph::new(format!("View: {} (v: cycle)", view_titles))
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(view_bar, left_chunks[0]);

        self.items.render(frame, left_chunks[1]);
        self.detail.render(frame, chunks[1]);

        if self.show_help {
            let help = Paragraph::new(
                "Storage Help:\n\
                j/k - Navigate items\n\
                Enter - Select item\n\
                v - Cycle view (Disks/Pools/Datasets/Subvolumes/LVM/Mounts)\n\
                r - Refresh\n\
                / - Filter\n\
                Ctrl+s - Sort\n\
                Detail: j/k scroll, Ctrl+d/u page\n\
                ? - Toggle help",
            )
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, left_chunks[2]);
        }
    }

    fn update(&mut self, state: &AppState) {
        self.rebuild_items(state);
        self.build_detail(state);
    }
}
