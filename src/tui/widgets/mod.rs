//! Reusable TUI widgets for interactive lists, detail panes, and dialogs.
//!
//! This module is a general-purpose widget toolkit: it exposes a complete API
//! for each widget, and an entry point that no tab uses does not stay in
//! the tree — the compiler says so, and the build fails on the warning.

pub mod cache;
pub mod detail;
pub mod dialog;
pub mod list;
pub mod progress;

pub use cache::BuildCache;
pub use detail::{DetailField, DetailPane, DetailSection};
pub use dialog::{Action, ActionMenu, ConfirmDialog, DialogResult};
pub use list::{ListWidget, SelectableItem};
pub use progress::ProgressOverlay;
