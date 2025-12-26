//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Clear,
    Frame,
};

use crate::app::{App, ConfigSection, InputMode, View};

// Submodules
mod dialogs;
mod port_select;
mod settings;
mod traffic;

// Re-export the format_hex_grouped function (used by handlers)
pub use traffic::format_hex_grouped;

// =============================================================================
// Helper Functions (shared across modules)
// =============================================================================

/// Create a centered separator line like "──── Title ────" that spans the full width
pub(crate) fn create_separator(title: &str, width: usize) -> String {
    let title_with_spaces = format!(" {} ", title);
    let title_len = title_with_spaces.chars().count();
    let remaining = width.saturating_sub(title_len);
    let left = remaining / 2;
    let right = remaining - left;
    format!(
        "{}{}{}",
        "─".repeat(left),
        title_with_spaces,
        "─".repeat(right)
    )
}

/// Push section separator lines if the section has changed and has a header.
/// Returns the new section for tracking.
pub(crate) fn push_section_separator<'a>(
    lines: &mut Vec<Line<'a>>,
    prev_section: Option<ConfigSection>,
    new_section: ConfigSection,
    panel_width: usize,
) -> Option<ConfigSection> {
    if prev_section != Some(new_section) {
        if let Some(header) = new_section.header() {
            lines.push(Line::from("")); // Spacer
            lines.push(Line::from(Span::styled(
                create_separator(header, panel_width),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }
    Some(new_section)
}

/// Calculate centered rect with given percentage of the area
pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// =============================================================================
// Main Render Entry Point
// =============================================================================

/// Render the application
pub fn render(frame: &mut Frame, app: &mut App) {
    // Clear the entire frame to prevent artifacts from previous renders
    frame.render_widget(Clear, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main content
            Constraint::Length(1), // Status bar / input
        ])
        .split(frame.area());

    match app.view {
        View::PortSelect => port_select::render_port_select(frame, app, chunks[0]),
        View::Connected => traffic::render_connected(frame, app, chunks[0]),
    }

    dialogs::render_status_bar(frame, app, chunks[1]);

    // Render settings panel as overlay if open
    if app.settings_panel.open {
        settings::render_settings_panel(frame, app);
    }

    // Render quit confirmation dialog as overlay if showing
    if app.traffic.quit_confirm {
        dialogs::render_quit_confirm_dialog(frame);
    }

    // Render split selection dialog as overlay if in split select mode
    if app.input.mode == InputMode::SplitSelect {
        dialogs::render_split_select_dialog(frame, app);
    }
}
