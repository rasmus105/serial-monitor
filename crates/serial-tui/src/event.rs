//! Terminal event handling.

use std::{io, time::Duration};

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
/// Returns `None` if no event is available within the timeout.
/// Returns `Some(AppEvent::Tick)` only when timeout > 0 and no event arrived
/// (to signal that periodic updates should occur).
pub fn poll_event(timeout: Duration) -> io::Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(Some(AppEvent::Key(key))),
            Event::Mouse(mouse) => Ok(Some(AppEvent::Mouse(mouse))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else if timeout.is_zero() {
        // No event and non-blocking poll - return None so drain loops can exit
        Ok(None)
    } else {
        // No event but we waited - return Tick for periodic updates
        Ok(Some(AppEvent::Tick))
    }
}
