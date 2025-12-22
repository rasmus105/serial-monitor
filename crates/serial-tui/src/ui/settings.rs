//! Settings panel rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs},
    Frame,
};

use crate::app::{App, InputMode};
use crate::settings::{AnyCommand, GeneralSetting, SettingsTab};

use super::centered_rect;

// =============================================================================
// Settings Panel
// =============================================================================

pub(super) fn render_settings_panel(frame: &mut Frame, app: &App) {
    // Create a centered floating panel (80% width, 80% height)
    let area = centered_rect(80, 80, frame.area());

    // Clear the area behind the panel
    frame.render_widget(Clear, area);

    // Create the outer block with title
    let title = if app.settings_panel.recording_key {
        " Settings - Press a key to bind (Esc to cancel) "
    } else {
        " Settings "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    frame.render_widget(block, area);

    // Inner area for content
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Split into tabs and content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Tabs
            Constraint::Min(1),    // Content
            Constraint::Length(2), // Help line
        ])
        .split(inner);

    // Render tabs
    render_settings_tabs(frame, app, chunks[0]);

    // Render tab content
    match app.settings_panel.tab {
        SettingsTab::General => render_general_settings_tab(frame, app, chunks[1]),
        SettingsTab::Keybindings => render_keybindings_tab(frame, app, chunks[1]),
    }

    // Render help line
    render_settings_help(frame, app, chunks[2]);
}

fn render_settings_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let tab_titles: Vec<Line> = SettingsTab::all()
        .iter()
        .map(|t| Line::from(t.name()))
        .collect();

    let selected_idx = SettingsTab::all()
        .iter()
        .position(|&t| t == app.settings_panel.tab)
        .unwrap_or(0);

    let tabs = Tabs::new(tab_titles)
        .select(selected_idx)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    frame.render_widget(tabs, area);
}

fn render_general_settings_tab(frame: &mut Frame, app: &App, area: Rect) {
    let dropdown_open = app.input.mode == InputMode::SettingsDropdown;
    let selected_setting = app.settings_panel.selected_general_setting;

    let mut lines: Vec<Line> = Vec::new();

    // Section header for Search
    lines.push(Line::from(vec![Span::styled(
        "── Search ──",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Search mode setting
    let is_search_selected = selected_setting == GeneralSetting::SearchMode;
    let search_prefix = if is_search_selected { "> " } else { "  " };
    let search_mode_display = format!("[ {} ]", app.search.mode().name());
    let search_label_style = if is_search_selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let search_value_style = if is_search_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };

    lines.push(Line::from(vec![
        Span::styled(search_prefix, search_label_style),
        Span::styled("Search mode: ", search_label_style),
        Span::styled(search_mode_display, search_value_style),
    ]));

    // Hint for the current search mode
    lines.push(Line::from(vec![
        Span::raw("               "),
        Span::styled(
            app.search.mode().description(),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    lines.push(Line::from("")); // Spacer

    // Section header for Filtering
    lines.push(Line::from(vec![Span::styled(
        "── Filtering ──",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Filter mode setting
    let is_filter_selected = selected_setting == GeneralSetting::FilterMode;
    let filter_prefix = if is_filter_selected { "> " } else { "  " };
    let filter_mode_display = format!("[ {} ]", app.traffic.filter.mode().name());
    let filter_label_style = if is_filter_selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let filter_value_style = if is_filter_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };

    lines.push(Line::from(vec![
        Span::styled(filter_prefix, filter_label_style),
        Span::styled("Filter mode: ", filter_label_style),
        Span::styled(filter_mode_display, filter_value_style),
    ]));

    // Hint for the current filter mode
    lines.push(Line::from(vec![
        Span::raw("              "),
        Span::styled(
            app.traffic.filter.mode().description(),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);

    // Render dropdown popup if open
    if dropdown_open {
        render_settings_dropdown(frame, app, area);
    }
}

fn render_settings_dropdown(frame: &mut Frame, app: &App, settings_area: Rect) {
    let selected_setting = app.settings_panel.selected_general_setting;
    let options = ["Regex", "Normal"];
    let dropdown_height = (options.len() + 2) as u16; // +2 for borders
    let dropdown_width = options.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6; // +6 for padding and borders

    // Position the dropdown based on which setting is selected
    // Search mode is at line index 2 (after header and empty line)
    // Filter mode is at line index 8 (after search section + spacer + header + empty line)
    let line_offset = match selected_setting {
        GeneralSetting::SearchMode => 2,
        GeneralSetting::FilterMode => 8,
    };
    let dropdown_y = settings_area.y + line_offset;
    let label_offset = match selected_setting {
        GeneralSetting::SearchMode => 18, // After "> Search mode: "
        GeneralSetting::FilterMode => 17, // After "> Filter mode: "
    };
    let dropdown_x = settings_area.x + label_offset;

    // Ensure dropdown fits on screen
    let available_height = frame.area().height.saturating_sub(dropdown_y);
    let actual_height = dropdown_height.min(available_height).max(3);

    let dropdown_area = Rect::new(
        dropdown_x.min(settings_area.x + settings_area.width.saturating_sub(dropdown_width)),
        dropdown_y,
        dropdown_width.min(settings_area.width),
        actual_height,
    );

    // Clear the dropdown area first
    frame.render_widget(Clear, dropdown_area);

    // Build dropdown items
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let is_selected = i == app.settings_panel.dropdown_index;
            let prefix = if is_selected { "> " } else { "  " };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(format!("{}{}", prefix, option)).style(style)
        })
        .collect();

    let dropdown_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let dropdown_list = List::new(items).block(dropdown_block);

    frame.render_widget(dropdown_list, dropdown_area);
}

fn render_keybindings_tab(frame: &mut Frame, app: &App, area: Rect) {
    let all_commands = AnyCommand::all();
    let visible_height = area.height as usize;

    // Calculate visible range based on scroll offset
    let scroll_offset = app.settings_panel.scroll_offset;
    let start = scroll_offset;
    let end = (scroll_offset + visible_height).min(all_commands.len());

    let mut lines: Vec<Line> = Vec::new();
    let mut current_category = "";

    for (idx, cmd) in all_commands
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
    {
        let category = cmd.category();

        // Add category header if changed
        if category != current_category {
            current_category = category;
            // Add empty line before category (except first)
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![Span::styled(
                format!("── {} ──", category),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
        }

        // Build the command line
        let is_selected = idx == app.settings_panel.selected_command;
        let bindings = app.settings.get_bindings(*cmd);
        let bindings_str = if bindings.is_empty() {
            "<none>".to_string()
        } else {
            bindings
                .iter()
                .map(|b| b.display())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let prefix = if is_selected { "▶ " } else { "  " };
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let bindings_style = if is_selected {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Create line with command name and bindings
        let name_width = 25;
        let name_padded = format!("{:<width$}", cmd.name(), width = name_width);

        lines.push(Line::from(vec![
            Span::styled(prefix, name_style),
            Span::styled(name_padded, name_style),
            Span::styled(bindings_str, bindings_style),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);

    // Render scrollbar if needed
    if all_commands.len() > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("┐"))
            .end_symbol(Some("┘"))
            .track_symbol(Some("│"))
            .thumb_symbol("█")
            .track_style(Style::default().fg(Color::DarkGray))
            .thumb_style(Style::default().fg(Color::Gray));

        let mut scrollbar_state =
            ScrollbarState::new(all_commands.len()).position(app.settings_panel.selected_command);

        let scrollbar_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y,
            width: 1,
            height: area.height,
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

fn render_settings_help(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.settings_panel.tab {
        SettingsTab::General => "Space/Enter: Toggle | h/l: Switch tab | Esc: Close",
        SettingsTab::Keybindings => {
            if app.settings_panel.recording_key {
                "Press any key to add binding | Esc: Cancel"
            } else {
                "j/k: Navigate | Ctrl+u/d: Page | a: Add | d: Delete | r: Reset | Esc: Close"
            }
        }
    };

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(help, area);
}
