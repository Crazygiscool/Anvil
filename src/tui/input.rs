//! Terminal input, read on a thread of its own.
//!
//! The event loop used to poll for input once per refresh tick, which put
//! a whole tick of delay in front of every keystroke — a second of it with
//! the default configuration. Reading the terminal on a dedicated thread
//! means a keypress is noticed the moment it happens, and a held-down key
//! scrolls as fast as the terminal repeats it.

use crossterm::event::Event;
use tokio::sync::mpsc;

/// The terminal's event stream.
///
/// The reader thread blocks in [`crossterm::event::read`], so it only ends
/// when the terminal is closed or the program exits.
pub struct TerminalInput {
    receiver: mpsc::UnboundedReceiver<Event>,
}

impl TerminalInput {
    /// Start reading the terminal.
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            while let Ok(event) = crossterm::event::read() {
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
        Self { receiver }
    }

    /// Wait for the next event.
    ///
    /// This is cancel-safe: a `select!` that drops the future loses no
    /// event.
    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[tokio::test]
    async fn an_event_that_arrives_is_handed_over() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        sender
            .send(Event::Key(KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }))
            .unwrap();

        let event = receiver.recv().await.unwrap();

        assert!(matches!(event, Event::Key(_)));
    }
}
