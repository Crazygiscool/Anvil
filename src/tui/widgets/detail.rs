//! Detail pane widget for displaying structured information about selected items.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, Wrap},
};

/// A section in the detail pane (collapsible group of key-value pairs).
#[derive(Debug, Clone, Default)]
pub struct DetailSection {
    pub title: String,
    pub fields: Vec<DetailField>,
    pub expanded: bool,
}

/// A single key-value field in a detail section.
#[derive(Debug, Clone)]
pub struct DetailField {
    pub key: String,
    pub value: String,
    pub style: Option<Style>,
}

/// Detail pane for showing structured information.
#[derive(Debug, Clone, Default)]
pub struct DetailPane {
    pub title: String,
    pub sections: Vec<DetailSection>,
    pub scroll_offset: u16,
    pub max_height: u16,
}

impl DetailPane {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            sections: Vec::new(),
            scroll_offset: 0,
            max_height: 0,
        }
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn add_section(&mut self, section: DetailSection) {
        self.sections.push(section);
    }

    pub fn clear(&mut self) {
        self.sections.clear();
        self.scroll_offset = 0;
    }

    /// Build detail pane from a simple key-value map.
    ///
    /// Only used by the tests; the tabs build their sections directly.
    #[cfg(test)]
    pub fn from_pairs(title: impl Into<String>, pairs: Vec<(String, String)>) -> Self {
        let mut pane = Self::new(title);
        let section = DetailSection {
            title: "Details".to_string(),
            fields: pairs
                .into_iter()
                .map(|(k, v)| DetailField {
                    key: k,
                    value: v,
                    style: None,
                })
                .collect(),
            expanded: true,
        };
        pane.add_section(section);
        pane
    }

    /// Add a field to the last section (or create one).
    pub fn push_field(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(section) = self.sections.last_mut() {
            section.fields.push(DetailField {
                key,
                value,
                style: None,
            });
        } else {
            self.add_section(DetailSection {
                title: "Details".to_string(),
                fields: vec![DetailField {
                    key,
                    value,
                    style: None,
                }],
                expanded: true,
            });
        }
    }

    /// Scroll down.
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    /// Scroll up.
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll to top.
    pub fn scroll_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Handle keyboard input. Returns true if handled.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up(1);
                true
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_down(10);
                true
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_up(10);
                true
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_top();
                true
            }
            KeyCode::Char(' ') => {
                // Toggle first section expansion
                if let Some(section) = self.sections.first_mut() {
                    section.expanded = !section.expanded;
                }
                true
            }
            _ => false,
        }
    }

    /// Render the detail pane.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.height < 3 {
            return;
        }

        // Build content lines
        let mut lines = Vec::new();
        for section in &self.sections {
            // Section header
            let expand_icon = if section.expanded { "▼" } else { "▶" };
            lines.push(Line::from(vec![Span::styled(
                format!("{} {}", expand_icon, section.title),
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));

            if section.expanded {
                for field in &section.fields {
                    let key_style = Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD);
                    let value_style = field.style.unwrap_or(Style::default());

                    // Handle multi-line values
                    let value_lines: Vec<&str> = field.value.lines().collect();
                    if value_lines.len() == 1 {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {}: ", field.key), key_style),
                            Span::styled(field.value.clone(), value_style),
                        ]));
                    } else {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  {}:", field.key),
                            key_style,
                        )]));
                        for line in value_lines {
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled(line.to_string(), value_style),
                            ]));
                        }
                    }
                }
            }
            lines.push(Line::from(""));
        }

        // Calculate content height for scrolling
        let content_height = lines.len() as u16;
        self.max_height = area.height.saturating_sub(2); // Account for borders

        // Clamp scroll offset
        if content_height > self.max_height {
            let max_scroll = content_height - self.max_height;
            if self.scroll_offset > max_scroll {
                self.scroll_offset = max_scroll;
            }
        } else {
            self.scroll_offset = 0;
        }

        // Create visible lines
        let start = self.scroll_offset as usize;
        let end = (start + self.max_height as usize).min(lines.len());
        let visible_lines = if start < lines.len() {
            lines[start..end].to_vec()
        } else {
            vec![Line::from("")]
        };

        let paragraph = Paragraph::new(visible_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.title.clone()),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);

        // Render scrollbar if needed
        if content_height > self.max_height {
            let scrollbar_area = Rect {
                x: area.x + area.width - 1,
                y: area.y + 1,
                width: 1,
                height: area.height - 2,
            };
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(None)
                .thumb_symbol("█");
            frame.render_stateful_widget(
                scrollbar,
                scrollbar_area,
                &mut ratatui::widgets::ScrollbarState::new(content_height as usize)
                    .position(self.scroll_offset as usize)
                    .viewport_content_length(self.max_height as usize),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn pane() -> DetailPane {
        let mut pane = DetailPane::new("Details");
        pane.add_section(DetailSection {
            title: "Section".to_string(),
            expanded: true,
            fields: vec![DetailField {
                key: "Key".to_string(),
                value: "Value".to_string(),
                style: None,
            }],
        });
        pane
    }

    fn render_text(pane: &mut DetailPane) -> String {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|frame| pane.render(frame, frame.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn scroll_down_and_up_move_the_offset() {
        let mut pane = pane();
        assert_eq!(pane.scroll_offset, 0);

        pane.scroll_down(3);
        assert_eq!(pane.scroll_offset, 3);

        pane.scroll_up(1);
        assert_eq!(pane.scroll_offset, 2);

        pane.scroll_top();
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn scroll_up_saturates_at_zero() {
        let mut pane = pane();
        pane.scroll_up(5);
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn space_toggles_the_first_section() {
        let mut pane = pane();
        assert!(pane.sections[0].expanded);

        assert!(pane.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!pane.sections[0].expanded);
    }

    #[test]
    fn clear_removes_sections_and_resets_scroll() {
        let mut pane = pane();
        pane.scroll_down(5);

        pane.clear();

        assert!(pane.sections.is_empty());
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn push_field_creates_a_section_when_none_exists() {
        let mut pane = DetailPane::new("Details");

        pane.push_field("Key", "Value");
        assert_eq!(pane.sections.len(), 1);
        assert_eq!(pane.sections[0].fields.len(), 1);

        pane.push_field("Second", "2");
        assert_eq!(pane.sections.len(), 1);
        assert_eq!(pane.sections[0].fields.len(), 2);
    }

    #[test]
    fn from_pairs_builds_a_single_section() {
        let pane =
            DetailPane::from_pairs("Details", vec![("Key".to_string(), "Value".to_string())]);

        assert_eq!(pane.sections.len(), 1);
        assert_eq!(pane.sections[0].fields[0].key, "Key");
        assert_eq!(pane.sections[0].fields[0].value, "Value");
    }

    #[test]
    fn renders_section_titles_and_fields() {
        let mut pane = pane();
        let text = render_text(&mut pane);

        assert!(text.contains("Details"));
        assert!(text.contains("Section"));
        assert!(text.contains("Key"));
        assert!(text.contains("Value"));
    }

    #[test]
    fn collapsed_sections_hide_their_fields() {
        let mut pane = pane();
        pane.sections[0].expanded = false;

        let text = render_text(&mut pane);

        assert!(text.contains("Section"));
        assert!(!text.contains("Value"));
    }
}
