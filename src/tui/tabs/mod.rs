//! Tab module declarations and base trait.

pub mod catalog;
pub mod discovery;
pub mod overview;
pub mod runtime;
pub mod storage;
pub mod telemetry;

use crate::app::state::AppState;
use crate::tui::ActiveTab;
use crate::tui::widgets::{ActionMenu, ConfirmDialog, DialogResult, ProgressOverlay};
use crossterm::event::KeyEvent;
use ratatui::Frame;

/// Base trait for all tabs.
pub trait Tab: Send + Sync {
    /// Tab identifier.
    fn id(&self) -> ActiveTab;

    /// Tab title for the tab bar.
    fn title(&self) -> &'static str;

    /// Handle a key event. Returns true if handled.
    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool;

    /// Render the tab content.
    fn render(&mut self, frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState);

    /// Called when the tab becomes active.
    fn on_activate(&mut self, _state: &mut AppState) {}

    /// Called when the tab becomes inactive.
    fn on_deactivate(&mut self) {}

    /// Update internal state from `AppState` (called each frame).
    fn update(&mut self, _state: &AppState) {}

    /// Request a different tab after handling a key (used by quick actions).
    fn requested_tab(&self) -> Option<ActiveTab> {
        None
    }

    /// Get action menu for selected item (optional).
    fn action_menu(&mut self) -> Option<&mut ActionMenu> {
        None
    }

    /// Get confirmation dialog (optional).
    fn confirm_dialog(&mut self) -> Option<&mut ConfirmDialog> {
        None
    }

    /// Get progress overlay (optional).
    fn progress_overlay(&mut self) -> Option<&mut ProgressOverlay> {
        None
    }

    /// Handle dialog result (default: do nothing).
    fn handle_dialog_result(&mut self, _result: DialogResult, _state: &mut AppState) {}

    /// Handle menu result (default: do nothing).
    fn handle_menu_result(&mut self, _result: DialogResult, _state: &mut AppState) {}
}

/// Tab manager to handle multiple tabs.
///
/// The manager owns the ordered list of tabs and tracks the active tab by its
/// [`ActiveTab`] identifier, so the tab bar and the rendered view can never
/// disagree about which tab is showing.
pub struct TabManager {
    pub tabs: Vec<Box<dyn Tab>>,
    active: ActiveTab,
}

impl TabManager {
    pub fn new(tabs: Vec<Box<dyn Tab>>) -> Self {
        let active = tabs
            .first()
            .map(|tab| tab.id())
            .unwrap_or(ActiveTab::Overview);
        Self { tabs, active }
    }

    /// Identifier of the active tab.
    pub fn active(&self) -> ActiveTab {
        self.active
    }

    /// Index of the active tab within [`TabManager::tabs`].
    pub fn active_index(&self) -> usize {
        self.index_of(self.active).unwrap_or(0)
    }

    /// Resolve a tab identifier to its index in [`TabManager::tabs`].
    fn index_of(&self, tab: ActiveTab) -> Option<usize> {
        self.tabs.iter().position(|candidate| candidate.id() == tab)
    }

    /// Mutable access to the active tab.
    pub fn active_tab(&mut self) -> &mut dyn Tab {
        let index = self.active_index();
        &mut *self.tabs[index]
    }

    /// Activate a tab by identifier. Unknown identifiers are ignored.
    pub fn set_active(&mut self, tab: ActiveTab, state: &mut AppState) {
        let Some(index) = self.index_of(tab) else {
            return;
        };
        let current = self.active_index();
        if index == current {
            return;
        }
        self.tabs[current].on_deactivate();
        self.active = tab;
        self.tabs[index].on_activate(state);
    }

    /// Activate the next tab in display order.
    pub fn next(&mut self, state: &mut AppState) {
        if self.tabs.is_empty() {
            return;
        }
        let index = (self.active_index() + 1) % self.tabs.len();
        let tab = self.tabs[index].id();
        self.set_active(tab, state);
    }

    /// Activate the previous tab in display order.
    pub fn previous(&mut self, state: &mut AppState) {
        if self.tabs.is_empty() {
            return;
        }
        let current = self.active_index();
        let index = if current == 0 {
            self.tabs.len() - 1
        } else {
            current - 1
        };
        let tab = self.tabs[index].id();
        self.set_active(tab, state);
    }

    /// Dispatch a key event to the active tab. Returns true if handled.
    pub fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        // Dialogs and overlays take priority over everything else.
        if let Some(dialog) = self.active_tab().confirm_dialog()
            && dialog.is_visible()
        {
            if let Some(result) = dialog.handle_key(key) {
                self.active_tab().handle_dialog_result(result, state);
                return true;
            }
            return true;
        }

        if let Some(menu) = self.active_tab().action_menu()
            && menu.is_visible()
        {
            if let Some(result) = menu.handle_key(key) {
                self.active_tab().handle_menu_result(result, state);
                return true;
            }
            return true;
        }

        if let Some(progress) = self.active_tab().progress_overlay()
            && progress.is_visible()
        {
            if progress.handle_key(key) {
                return true;
            }
            // Dismiss a completed progress overlay.
            if progress.is_done() && !progress.is_running() {
                progress.hide();
                return true;
            }
            return true;
        }

        // Tab switching.
        match key.code {
            crossterm::event::KeyCode::Tab => {
                self.next(state);
                return true;
            }
            crossterm::event::KeyCode::BackTab => {
                self.previous(state);
                return true;
            }
            _ => {}
        }

        let handled = self.active_tab().handle_key(key, state);

        // Honour a tab switch requested by the active tab (quick actions).
        if let Some(requested) = self.active_tab().requested_tab()
            && requested != self.active
        {
            self.set_active(requested, state);
        }

        handled
    }

    /// Render the active tab and any overlays it owns.
    pub fn render(&mut self, frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
        self.active_tab().update(state);
        self.active_tab().render(frame, area, state);

        if let Some(dialog) = self.active_tab().confirm_dialog()
            && dialog.is_visible()
        {
            dialog.render(frame, area);
        }

        if let Some(menu) = self.active_tab().action_menu()
            && menu.is_visible()
        {
            menu.render(frame, area);
        }

        if let Some(progress) = self.active_tab().progress_overlay()
            && progress.is_visible()
        {
            progress.render(frame, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::AccessContext;
    use crate::app::state::AppState;
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    struct StubTab {
        id: ActiveTab,
        activated: bool,
    }

    impl Tab for StubTab {
        fn id(&self) -> ActiveTab {
            self.id
        }

        fn title(&self) -> &'static str {
            self.id.name()
        }

        fn handle_key(&mut self, _key: KeyEvent, _state: &mut AppState) -> bool {
            false
        }

        fn render(&mut self, _frame: &mut Frame, _area: ratatui::layout::Rect, _state: &AppState) {}

        fn on_activate(&mut self, _state: &mut AppState) {
            self.activated = true;
        }
    }

    fn manager() -> TabManager {
        TabManager::new(vec![
            Box::new(StubTab {
                id: ActiveTab::Overview,
                activated: false,
            }),
            Box::new(StubTab {
                id: ActiveTab::Discovery,
                activated: false,
            }),
            Box::new(StubTab {
                id: ActiveTab::Storage,
                activated: false,
            }),
        ])
    }

    fn state() -> AppState {
        AppState::bootstrap(Config::default(), AccessContext::detect(false))
    }

    #[test]
    fn starts_on_the_first_tab() {
        let manager = manager();
        assert_eq!(manager.active(), ActiveTab::Overview);
        assert_eq!(manager.active_index(), 0);
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let mut manager = manager();
        let mut state = state();

        manager.next(&mut state);
        assert_eq!(manager.active(), ActiveTab::Discovery);
        manager.next(&mut state);
        manager.next(&mut state);
        assert_eq!(manager.active(), ActiveTab::Overview);
        manager.previous(&mut state);
        assert_eq!(manager.active(), ActiveTab::Storage);
    }

    #[test]
    fn set_active_activates_the_target_tab() {
        let mut manager = manager();
        let mut state = state();

        manager.set_active(ActiveTab::Storage, &mut state);

        assert_eq!(manager.active(), ActiveTab::Storage);
        assert_eq!(manager.active_index(), 2);
    }

    #[test]
    fn set_active_ignores_unknown_tabs() {
        let mut manager = manager();
        let mut state = state();

        manager.set_active(ActiveTab::Catalog, &mut state);

        assert_eq!(manager.active(), ActiveTab::Overview);
    }

    #[test]
    fn tab_key_switches_tabs() {
        let mut manager = manager();
        let mut state = state();
        let key = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };

        assert!(manager.handle_key(key, &mut state));
        assert_eq!(manager.active(), ActiveTab::Discovery);
    }
}
