//! Reusable TUI widgets for interactive lists, detail panes, and dialogs.
//!
//! This module is a general-purpose widget toolkit: it exposes a complete API
//! for each widget, and not every entry point is wired into a tab yet.

#![allow(dead_code)]

pub mod detail;
pub mod dialog;
pub mod list;
pub mod progress;

pub use detail::{DetailField, DetailPane, DetailSection};
pub use dialog::{Action, ActionMenu, ConfirmDialog, DialogResult};
pub use list::{ListWidget, SelectableItem};
pub use progress::ProgressOverlay;
