//! Dialog and status bar rendering

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, InputMode};

// =============================================================================
// Status Bar
// =============================================================================

pub(super) fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    match app.input.mode.style() {
        Some(style) => {
            // Text input mode - show prefix, buffer, and cursor
            let input_line = Line::from(vec![
                Span::styled(style.prefix, Style::default().fg(style.color)),
                Span::raw(&app.input.buffer),
                Span::styled("_", Style::default().fg(style.color)),
            ]);
            let input = Paragraph::new(input_line).style(Style::default().fg(Color::White));
            frame.render_widget(input, area);
        }
        None => {
            // Normal mode or special modes without text input
            let (text, color) = match app.input.mode {
                InputMode::ConfigDropdown
                | InputMode::TrafficConfigDropdown
                | InputMode::SettingsDropdown => (app.input.mode.entry_prompt(), Color::Cyan),
                _ => (app.status.as_str(), Color::White),
            };
            let status = Paragraph::new(text).style(Style::default().fg(color));
            frame.render_widget(status, area);
        }
    }
}

// =============================================================================
// Quit Confirmation Dialog
// =============================================================================

pub(super) fn render_quit_confirm_dialog(frame: &mut Frame) {
    // Create a small centered dialog
    let area = frame.area();

    // Dialog dimensions
    let dialog_width = 40u16;
    let dialog_height = 5u16;

    // Center the dialog
    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x,
        y,
        width: dialog_width.min(area.width),
        height: dialog_height.min(area.height),
    };

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Create the dialog block
    let block = Block::default()
        .title(" Disconnect? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Render the dialog content
    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Y",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Yes    "),
            Span::styled(
                "N/q/Esc",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Cancel"),
        ]),
    ];

    let paragraph = Paragraph::new(text).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(paragraph, inner);
}

// =============================================================================
// Split Selection Dialog
// =============================================================================

pub(super) fn render_split_select_dialog(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Get available split options based on current primary content
    let primary = app.layout.primary_content();
    let options = primary.available_splits();

    // Dialog dimensions - adjust based on content
    let dialog_width = 45u16;
    let dialog_height = (options.len() as u16 + 4).max(6); // +4 for borders, title line, and help line

    // Center the dialog
    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x,
        y,
        width: dialog_width.min(area.width),
        height: dialog_height.min(area.height),
    };

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Create the dialog block
    let title = format!(" Split {} with: ", primary.display_name());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Build content lines
    let mut lines = Vec::new();

    // Add options
    for content in options {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!("[{}]", content.tab_number()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(content.display_name(), Style::default().fg(Color::White)),
        ]));
    }

    // Add empty line and help
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("   "),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": Cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
