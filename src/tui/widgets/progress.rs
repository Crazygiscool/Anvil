//! Progress overlay for async operations.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

/// State of a progress operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    /// Operation not started.
    Idle,
    /// Operation running with known progress (0-100).
    Running(u16),
    /// Operation running with unknown progress (spinner).
    Indeterminate,
    /// Operation completed successfully.
    Success,
    /// Operation failed.
    Error,
    /// Operation cancelled by user.
    Cancelled,
}

/// Progress overlay for showing async operation status.
#[derive(Debug, Clone)]
pub struct ProgressOverlay {
    pub title: String,
    pub message: String,
    pub state: ProgressState,
    pub progress: u16, // 0-100
    pub show_details: bool,
    pub details: Vec<String>,
    pub cancellable: bool,
    pub visible: bool,
}

impl ProgressOverlay {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            state: ProgressState::Idle,
            progress: 0,
            show_details: false,
            details: Vec::new(),
            cancellable: false,
            visible: false,
        }
    }

    pub fn start(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.state = ProgressState::Indeterminate;
        self.progress = 0;
        self.details.clear();
        self.visible = true;
    }

    pub fn start_determinate(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.state = ProgressState::Running(0);
        self.progress = 0;
        self.details.clear();
        self.visible = true;
    }

    pub fn set_progress(&mut self, progress: u16) {
        self.progress = progress.min(100);
        self.state = ProgressState::Running(self.progress);
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    pub fn add_detail(&mut self, detail: impl Into<String>) {
        self.details.push(detail.into());
    }

    pub fn set_cancellable(&mut self, cancellable: bool) {
        self.cancellable = cancellable;
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.state = ProgressState::Success;
        self.progress = 100;
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.state = ProgressState::Error;
    }

    pub fn cancel(&mut self) {
        self.state = ProgressState::Cancelled;
        self.message = "Cancelled".to_string();
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            ProgressState::Running(_) | ProgressState::Indeterminate
        )
    }

    pub fn is_done(&self) -> bool {
        matches!(
            self.state,
            ProgressState::Success | ProgressState::Error | ProgressState::Cancelled
        )
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if !self.visible || !self.is_running() {
            return false;
        }

        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Esc if self.cancellable => {
                self.cancel();
                true
            }
            KeyCode::Char('c')
                if self.cancellable
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.cancel();
                true
            }
            KeyCode::Char('d') => {
                self.show_details = !self.show_details;
                true
            }
            _ => false,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_width = 70;
        let base_height = 8;
        let details_height = if self.show_details {
            self.details.len() as u16 + 2
        } else {
            0
        };
        let popup_height = base_height + details_height;
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width.min(area.width),
            height: popup_height.min(area.height),
        };

        frame.render_widget(ratatui::widgets::Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.clone())
            .style(match self.state {
                ProgressState::Error => Style::default().fg(Color::Red),
                ProgressState::Success => Style::default().fg(Color::Green),
                ProgressState::Cancelled => Style::default().fg(Color::Yellow),
                _ => Style::default(),
            });
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut y = inner.y;

        // Spinner or progress bar
        let gauge_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };

        match self.state {
            ProgressState::Indeterminate => {
                // Spinner animation based on time
                let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let idx = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    / 100) as usize
                    % spinner_chars.len();
                let spinner = Paragraph::new(format!("{} Working...", spinner_chars[idx]))
                    .style(Style::default().fg(Color::Cyan))
                    .alignment(Alignment::Center);
                frame.render_widget(spinner, gauge_area);
            }
            ProgressState::Running(p) => {
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Cyan))
                    .percent(p)
                    .label(format!("{}%", p));
                frame.render_widget(gauge, gauge_area);
            }
            ProgressState::Success => {
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Green))
                    .percent(100)
                    .label("Complete");
                frame.render_widget(gauge, gauge_area);
            }
            ProgressState::Error => {
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Red))
                    .percent(100)
                    .label("Failed");
                frame.render_widget(gauge, gauge_area);
            }
            ProgressState::Cancelled => {
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Yellow))
                    .percent(100)
                    .label("Cancelled");
                frame.render_widget(gauge, gauge_area);
            }
            ProgressState::Idle => {}
        }
        y += 2;

        // Message
        let msg_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 2,
        };
        let msg = Paragraph::new(self.message.clone())
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(msg, msg_area);
        y += 3;

        // Details
        if self.show_details && !self.details.is_empty() {
            let details_text = self.details.join("\n");
            let details_area = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: self.details.len() as u16 + 1,
            };
            let details = Paragraph::new(details_text)
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("Details"));
            frame.render_widget(details, details_area);
            y += self.details.len() as u16 + 2;
        }

        // Hint
        if self.is_running() {
            let hint_area = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            let hint = if self.cancellable {
                "Esc/Ctrl+C: cancel | d: toggle details"
            } else {
                "d: toggle details"
            };
            let hint = Paragraph::new(hint)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(hint, hint_area);
        } else if self.is_done() {
            let hint_area = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            let hint = Paragraph::new("Press any key to dismiss")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(hint, hint_area);
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

    fn render_text(overlay: &mut ProgressOverlay) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        terminal
            .draw(|frame| overlay.render(frame, frame.area()))
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
    fn starts_hidden_and_idle() {
        let overlay = ProgressOverlay::new("Operation");
        assert!(!overlay.is_visible());
        assert!(!overlay.is_running());
        assert!(!overlay.is_done());
        assert_eq!(overlay.state, ProgressState::Idle);
    }

    #[test]
    fn start_makes_the_overlay_visible_and_running() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");

        assert!(overlay.is_visible());
        assert!(overlay.is_running());
        assert!(!overlay.is_done());
        assert_eq!(overlay.message, "Working");
    }

    #[test]
    fn progress_is_clamped_to_one_hundred() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start_determinate("Working");
        overlay.set_progress(150);

        assert_eq!(overlay.progress, 100);
        assert_eq!(overlay.state, ProgressState::Running(100));
    }

    #[test]
    fn success_is_a_terminal_state() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");
        overlay.success("Finished");

        assert!(overlay.is_done());
        assert!(!overlay.is_running());
        assert_eq!(overlay.progress, 100);
        assert_eq!(overlay.message, "Finished");
    }

    #[test]
    fn error_is_a_terminal_state() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");
        overlay.error("Boom");

        assert!(overlay.is_done());
        assert!(!overlay.is_running());
        assert_eq!(overlay.message, "Boom");
    }

    #[test]
    fn cancel_is_a_terminal_state() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");
        overlay.cancel();

        assert!(overlay.is_done());
        assert!(!overlay.is_running());
        assert_eq!(overlay.message, "Cancelled");
    }

    #[test]
    fn start_clears_previous_details() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("First");
        overlay.add_detail("a detail");
        assert_eq!(overlay.details.len(), 1);

        overlay.start("Second");

        assert!(overlay.details.is_empty());
    }

    #[test]
    fn details_toggle_only_while_running() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");

        assert!(overlay.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE)));
        assert!(overlay.show_details);

        overlay.success("Finished");
        assert!(!overlay.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE)));
    }

    #[test]
    fn cancel_key_requires_a_cancellable_operation() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Working");

        assert!(!overlay.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(overlay.is_running());

        overlay.set_cancellable(true);
        assert!(overlay.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(overlay.is_done());
    }

    #[test]
    fn hidden_overlay_ignores_keys() {
        let mut overlay = ProgressOverlay::new("Operation");
        assert!(!overlay.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE)));
        assert!(!overlay.show_details);
    }

    #[test]
    fn renders_the_message() {
        let mut overlay = ProgressOverlay::new("Operation");
        overlay.start("Starting container");
        overlay.success("Container started");

        let text = render_text(&mut overlay);

        assert!(text.contains("Operation"));
        assert!(text.contains("Container started"));
    }
}
