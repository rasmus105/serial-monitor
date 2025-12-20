//! Event handling for terminal input

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Terminal events
#[derive(Debug)]
pub enum AppEvent {
    /// Key press
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for UI refresh
    Tick,
}

/// Poll for terminal events with a timeout
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(Some(AppEvent::Key(key))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else {
        Ok(Some(AppEvent::Tick))
    }
}

/// Check if this is a quit key combination
pub fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } | KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}
