//! Dialog widgets for actions, confirmations, and menus.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

/// Result of a dialog interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    Confirmed,
    Cancelled,
    Selected(usize),
}

/// Action menu for item-specific actions (popup).
#[derive(Debug, Clone)]
pub struct ActionMenu {
    pub title: String,
    pub actions: Vec<Action>,
    pub state: ListState,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub label: String,
    pub description: String,
    pub key: Option<char>,
    pub enabled: bool,
    pub dangerous: bool, // Red styling for destructive actions
}

impl ActionMenu {
    pub fn new(title: impl Into<String>, actions: Vec<Action>) -> Self {
        let mut state = ListState::default();
        if !actions.is_empty() {
            state.select(Some(0));
        }
        Self {
            title: title.into(),
            actions,
            state,
            visible: false,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        if !self.actions.is_empty() && self.state.selected().is_none() {
            self.state.select(Some(0));
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// The action the cursor is on.
    ///
    /// The tabs act on the index the menu returns, so this is only used by
    /// the tests.
    #[cfg(test)]
    pub fn selected_action(&self) -> Option<&Action> {
        self.state.selected().map(|i| &self.actions[i])
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<DialogResult> {
        if !self.visible {
            return None;
        }

        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.hide();
                Some(DialogResult::Cancelled)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.actions.len();
                let selected = self.state.selected().unwrap_or(0);
                self.state.select(Some((selected + 1).min(len - 1)));
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let selected = self.state.selected().unwrap_or(0);
                self.state.select(Some(selected.saturating_sub(1)));
                None
            }
            KeyCode::Enter => {
                if let Some(idx) = self.state.selected()
                    && self.actions[idx].enabled
                {
                    self.hide();
                    return Some(DialogResult::Selected(idx));
                }
                None
            }
            KeyCode::Char(c) => {
                // Hotkey selection
                for (idx, action) in self.actions.iter().enumerate() {
                    if action.key == Some(c) && action.enabled {
                        self.hide();
                        return Some(DialogResult::Selected(idx));
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Center the popup
        let popup_width = 50;
        let popup_height = (self.actions.len() + 4).min(20) as u16;
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width.min(area.width),
            height: popup_height.min(area.height),
        };

        // Clear background
        frame.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .actions
            .iter()
            .enumerate()
            .map(|(idx, action)| {
                let mut spans = Vec::new();

                if let Some(key) = action.key {
                    spans.push(Span::styled(
                        format!("  [{}] ", key),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::raw("      "));
                }

                let label_style = if action.dangerous {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else if !action.enabled {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(action.label.clone(), label_style));

                if !action.description.is_empty() {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        action.description.clone(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                let mut item = ListItem::new(Line::from(spans));
                if idx == self.state.selected().unwrap_or(0) {
                    item = item.style(Style::default().add_modifier(Modifier::REVERSED));
                }
                item
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.title.clone()),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("► ");

        frame.render_stateful_widget(list, popup_area, &mut self.state);
    }
}

/// Confirmation dialog for destructive actions.
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub dangerous: bool,
    pub selected: bool, // true = confirm, false = cancel
    pub visible: bool,
}

impl ConfirmDialog {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: "Yes".to_string(),
            cancel_label: "No".to_string(),
            dangerous: false,
            selected: false,
            visible: false,
        }
    }

    pub fn with_labels(mut self, confirm: impl Into<String>, cancel: impl Into<String>) -> Self {
        self.confirm_label = confirm.into();
        self.cancel_label = cancel.into();
        self
    }

    pub fn dangerous(mut self, dangerous: bool) -> Self {
        self.dangerous = dangerous;
        self
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.selected = false; // Default to cancel (safe)
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<DialogResult> {
        if !self.visible {
            return None;
        }

        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.hide();
                Some(DialogResult::Cancelled)
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.selected = !self.selected;
                None
            }
            KeyCode::Enter => {
                self.hide();
                if self.selected {
                    Some(DialogResult::Confirmed)
                } else {
                    Some(DialogResult::Cancelled)
                }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.hide();
                Some(DialogResult::Confirmed)
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.hide();
                Some(DialogResult::Cancelled)
            }
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_width = 60;
        let popup_height = 10;
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width.min(area.width),
            height: popup_height.min(area.height),
        };

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.clone())
            .style(if self.dangerous {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            });

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Message
        let msg_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(3),
        };
        let msg = Paragraph::new(self.message.clone())
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(msg, msg_area);

        // Buttons
        let btn_y = inner.y + inner.height.saturating_sub(2);
        let btn_width = inner.width / 2;
        let confirm_x = inner.x;
        let cancel_x = inner.x + btn_width;

        let confirm_style = if self.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let cancel_style = if !self.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        let confirm_label = format!(" {} ", self.confirm_label);
        let cancel_label = format!(" {} ", self.cancel_label);

        let confirm_btn = Paragraph::new(confirm_label)
            .style(if self.dangerous {
                confirm_style.fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                confirm_style
            })
            .alignment(Alignment::Center);
        let cancel_btn = Paragraph::new(cancel_label)
            .style(cancel_style)
            .alignment(Alignment::Center);

        let confirm_area = Rect {
            x: confirm_x,
            y: btn_y,
            width: btn_width,
            height: 1,
        };
        let cancel_area = Rect {
            x: cancel_x,
            y: btn_y,
            width: btn_width,
            height: 1,
        };

        frame.render_widget(confirm_btn, confirm_area);
        frame.render_widget(cancel_btn, cancel_area);

        // Hint
        let hint_area = Rect {
            x: inner.x,
            y: btn_y + 1,
            width: inner.width,
            height: 1,
        };
        let hint = Paragraph::new("←/→/Tab: switch | Enter: confirm | Esc: cancel")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, hint_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn actions() -> Vec<Action> {
        vec![
            Action {
                label: "Start".to_string(),
                description: "Start the container".to_string(),
                key: Some('s'),
                enabled: true,
                dangerous: false,
            },
            Action {
                label: "Remove".to_string(),
                description: "Remove the container".to_string(),
                key: Some('r'),
                enabled: true,
                dangerous: true,
            },
            Action {
                label: "Unavailable".to_string(),
                description: String::new(),
                key: Some('x'),
                enabled: false,
                dangerous: false,
            },
        ]
    }

    fn render_text(render: impl FnOnce(&mut Frame)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
        terminal.draw(render).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn action_menu_stays_hidden_until_shown() {
        let mut menu = ActionMenu::new("Actions", actions());
        assert!(!menu.is_visible());

        menu.show();

        assert!(menu.is_visible());
        assert_eq!(
            menu.selected_action().map(|action| action.label.clone()),
            Some("Start".to_string())
        );
    }

    #[test]
    fn action_menu_enter_selects_the_highlighted_action() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        assert_eq!(
            menu.handle_key(key(KeyCode::Enter)),
            Some(DialogResult::Selected(0))
        );
        assert!(!menu.is_visible());
    }

    #[test]
    fn action_menu_hotkey_selects_the_matching_action() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        assert_eq!(
            menu.handle_key(key(KeyCode::Char('r'))),
            Some(DialogResult::Selected(1))
        );
    }

    #[test]
    fn action_menu_ignores_disabled_actions() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        assert_eq!(menu.handle_key(key(KeyCode::Char('x'))), None);
        assert!(menu.is_visible());
    }

    #[test]
    fn action_menu_escape_cancels() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        assert_eq!(
            menu.handle_key(key(KeyCode::Esc)),
            Some(DialogResult::Cancelled)
        );
        assert!(!menu.is_visible());
    }

    #[test]
    fn action_menu_j_k_moves_the_highlight() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        assert_eq!(menu.handle_key(key(KeyCode::Char('j'))), None);
        assert_eq!(
            menu.selected_action().map(|action| action.label.clone()),
            Some("Remove".to_string())
        );

        assert_eq!(menu.handle_key(key(KeyCode::Char('k'))), None);
        assert_eq!(
            menu.selected_action().map(|action| action.label.clone()),
            Some("Start".to_string())
        );
    }

    #[test]
    fn action_menu_ignores_keys_while_hidden() {
        let mut menu = ActionMenu::new("Actions", actions());
        assert_eq!(menu.handle_key(key(KeyCode::Enter)), None);
    }

    #[test]
    fn confirm_dialog_defaults_to_the_safe_option() {
        let mut dialog = ConfirmDialog::new("Confirm", "Run it?").with_labels("Run", "Cancel");
        dialog.show();

        assert!(dialog.is_visible());
        assert!(!dialog.selected);
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            Some(DialogResult::Cancelled)
        );
    }

    #[test]
    fn confirm_dialog_tab_switches_to_confirm() {
        let mut dialog = ConfirmDialog::new("Confirm", "Run it?");
        dialog.show();

        assert_eq!(dialog.handle_key(key(KeyCode::Tab)), None);
        assert!(dialog.selected);
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            Some(DialogResult::Confirmed)
        );
    }

    #[test]
    fn confirm_dialog_y_confirms_and_n_cancels() {
        let mut dialog = ConfirmDialog::new("Confirm", "Run it?");

        dialog.show();
        assert_eq!(
            dialog.handle_key(key(KeyCode::Char('y'))),
            Some(DialogResult::Confirmed)
        );
        assert!(!dialog.is_visible());

        dialog.show();
        assert_eq!(
            dialog.handle_key(key(KeyCode::Char('n'))),
            Some(DialogResult::Cancelled)
        );
    }

    #[test]
    fn confirm_dialog_escape_cancels() {
        let mut dialog = ConfirmDialog::new("Confirm", "Run it?");
        dialog.show();

        assert_eq!(
            dialog.handle_key(key(KeyCode::Esc)),
            Some(DialogResult::Cancelled)
        );
        assert!(!dialog.is_visible());
    }

    #[test]
    fn action_menu_renders_its_actions() {
        let mut menu = ActionMenu::new("Actions", actions());
        menu.show();

        let text = render_text(|frame| menu.render(frame, frame.area()));

        assert!(text.contains("Actions"));
        assert!(text.contains("Start"));
        assert!(text.contains("Remove"));
    }

    #[test]
    fn confirm_dialog_renders_its_message_and_buttons() {
        let mut dialog = ConfirmDialog::new("Confirm Start", "Run docker start web?")
            .with_labels("Run", "Cancel");
        dialog.show();

        let text = render_text(|frame| dialog.render(frame, frame.area()));

        assert!(text.contains("Confirm Start"));
        assert!(text.contains("Run docker start web?"));
        assert!(text.contains("Run"));
        assert!(text.contains("Cancel"));
    }
}
