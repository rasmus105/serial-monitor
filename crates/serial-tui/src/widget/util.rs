//! Shared utilities for widgets.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::keybind::KeyHint;

// Re-export formatting utilities from core
pub use serial_core::ui::{format_bytes, format_duration, format_rate};

/// Build a help bar line from key hints.
///
/// Creates a line of styled spans like: "Enter connect  r refresh  / search"
pub fn build_help_line(hints: &[KeyHint], key_style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(hint.key.to_string(), key_style));
        spans.push(Span::raw(format!(" {}", hint.description)));
    }
    Line::from(spans)
}
