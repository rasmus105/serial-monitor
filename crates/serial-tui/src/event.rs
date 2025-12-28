//! Terminal event handling.

use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, MouseEvent};

/// Application events.
#[derive(Debug)]
pub enum AppEvent {
    /// Keyboard input
    Key(KeyEvent),
    /// Mouse input
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for periodic updates
    Tick,
}

/// Poll for terminal events with a timeout.
pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if event::poll(timeout).ok()? {
        match event::read().ok()? {
            Event::Key(key) => Some(AppEvent::Key(key)),
            Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
            Event::Resize(w, h) => Some(AppEvent::Resize(w, h)),
            _ => None,
        }
    } else {
        Some(AppEvent::Tick)
    }
}
