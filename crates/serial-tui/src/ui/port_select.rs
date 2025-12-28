//! Port selection view rendering

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use strum::IntoEnumIterator;

use crate::app::{App, ConfigField, ConfigSection, EnumNavigation, InputMode, PortSelectFocus};
use crate::command::{GlobalNavCommand, PortSelectCommand};

use super::push_section_separator;

// =============================================================================
// Port Selection View
// =============================================================================

pub(super) fn render_port_select(frame: &mut Frame, app: &App, area: Rect) {
    // Split horizontally if config panel is visible
    let (port_area, config_area) = if app.port_select.config.visible {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Render port list
    render_port_list(frame, app, port_area);

    // Render config panel if visible
    if let Some(config_area) = config_area {
        render_config_panel(frame, app, config_area);
    }
}

fn render_port_list(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.port_select.focus == PortSelectFocus::PortList;

    let items: Vec<ListItem> = app
        .port_select
        .ports
        .iter()
        .enumerate()
        .map(|(i, port)| {
            let style = if i == app.port_select.selected_port && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if i == app.port_select.selected_port {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let prefix = if i == app.port_select.selected_port {
                "> "
            } else {
                "  "
            };

            let label = if let Some(ref product) = port.product {
                format!("{}{} ({})", prefix, port.name, product)
            } else {
                format!("{}{}", prefix, port.name)
            };

            ListItem::new(label).style(style)
        })
        .collect();

    // Build dynamic title with actual keybindings
    let path_key = app
        .settings
        .keybindings
        .port_select
        .shortcut_hint(PortSelectCommand::EnterPortPath)
        .unwrap_or_else(|| ":".to_string());
    let refresh_key = app
        .settings
        .keybindings
        .port_select
        .shortcut_hint(PortSelectCommand::RefreshPorts)
        .unwrap_or_else(|| "r".to_string());
    let toggle_key = app
        .settings
        .keybindings
        .port_select
        .shortcut_hint(PortSelectCommand::ToggleConfigPanel)
        .unwrap_or_else(|| "t".to_string());
    let nav_keys = app
        .settings
        .keybindings
        .global_nav
        .shortcut_hint(GlobalNavCommand::Down)
        .unwrap_or_else(|| "j".to_string());
    let confirm_key = app
        .settings
        .keybindings
        .global_nav
        .shortcut_hint(GlobalNavCommand::Confirm)
        .unwrap_or_else(|| "Enter".to_string());

    let title = if app.port_select.ports.is_empty() {
        format!(
            " Select Port [{}: path, {}: refresh, {}: toggle config] ",
            path_key, refresh_key, toggle_key
        )
    } else {
        format!(
            " Select Port [{}: nav, {}: connect, {}: path, {}: refresh] ",
            nav_keys, confirm_key, path_key, refresh_key
        )
    };

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn render_config_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.port_select.focus == PortSelectFocus::Config;
    let dropdown_open = app.input.mode == InputMode::ConfigDropdown;
    let text_input_open = app.input.mode == InputMode::ConfigTextInput;

    let border_style = if is_focused || dropdown_open || text_input_open {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build dynamic title
    let confirm_key = app
        .settings
        .keybindings
        .global_nav
        .shortcut_hint(GlobalNavCommand::Confirm)
        .unwrap_or_else(|| "Enter".to_string());
    let toggle_key = app
        .settings
        .keybindings
        .port_select
        .shortcut_hint(PortSelectCommand::ToggleConfigPanel)
        .unwrap_or_else(|| "t".to_string());
    let title = format!(" Config [{}: select, {}: toggle] ", confirm_key, toggle_key);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build config lines using ConfigField iterator, grouping by section
    let panel_width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut prev_section: Option<ConfigSection> = None;

    for field in ConfigField::iter() {
        // Add separator when section changes
        prev_section =
            push_section_separator(&mut lines, prev_section, field.section(), panel_width);

        let is_selected = app.port_select.config.field == field
            && (is_focused || dropdown_open || text_input_open);
        let prefix = if is_selected { "> " } else { "  " };

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let value_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let value = app.port_select.get_config_display(field);

        // For text input fields being edited, show the input buffer
        let display_value = if text_input_open && is_selected && field.is_text_input() {
            format!("{}▌", app.input.buffer)
        } else {
            value
        };

        // For SaveDirectory, wrap the value if it's too long
        if field == ConfigField::SaveDirectory {
            let label_text = format!("{}{}: ", prefix, field.label());
            let label_len = label_text.chars().count();
            let available_width = panel_width.saturating_sub(label_len);

            if display_value.chars().count() <= available_width {
                // Fits on one line
                lines.push(Line::from(vec![
                    Span::styled(prefix, label_style),
                    Span::styled(format!("{}: ", field.label()), label_style),
                    Span::styled(display_value, value_style),
                ]));
            } else {
                // Need to wrap - first line has label
                let chars: Vec<char> = display_value.chars().collect();
                let first_line_chars: String = chars.iter().take(available_width).collect();
                let remaining: String = chars.iter().skip(available_width).collect();

                lines.push(Line::from(vec![
                    Span::styled(prefix, label_style),
                    Span::styled(format!("{}: ", field.label()), label_style),
                    Span::styled(first_line_chars, value_style),
                ]));

                // Continuation lines - indent to align with value
                let indent = " ".repeat(label_len);
                let mut remaining_chars: Vec<char> = remaining.chars().collect();
                while !remaining_chars.is_empty() {
                    let line_chars: String = remaining_chars.iter().take(available_width).collect();
                    remaining_chars = remaining_chars.into_iter().skip(available_width).collect();
                    lines.push(Line::from(vec![
                        Span::raw(indent.clone()),
                        Span::styled(line_chars, value_style),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{}: ", field.label()), label_style),
                Span::styled(display_value, value_style),
            ]));
        }
    }

    // Calculate visible height and apply scroll
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let scroll_offset = app.port_select.config.scroll_offset;

    // Only scroll if content exceeds visible height
    let needs_scroll = total_lines > visible_height;
    let actual_scroll = if needs_scroll {
        scroll_offset.min(total_lines.saturating_sub(visible_height))
    } else {
        0
    };

    // Take only the visible lines
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(actual_scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);

    // Render scroll indicator if needed
    if needs_scroll {
        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(actual_scroll)
            .viewport_content_length(visible_height);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█")
            .track_style(Style::default().fg(Color::DarkGray))
            .thumb_style(Style::default().fg(Color::Gray));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }

    // Render dropdown popup if open
    if dropdown_open {
        render_config_dropdown(frame, app, area);
    }
}

fn render_config_dropdown(frame: &mut Frame, app: &App, config_area: Rect) {
    let options = app.port_select.get_config_option_strings();
    let dropdown_height = (options.len() + 2) as u16; // +2 for borders
    let dropdown_width = options.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6; // +6 for padding and borders

    // Position the dropdown based on which field is selected
    let field_index = app.port_select.config.field.index();

    // Position dropdown next to the selected field
    let dropdown_y = config_area.y + 1 + field_index as u16; // +1 for border
    let dropdown_x = config_area.x + config_area.width.saturating_sub(dropdown_width + 1);

    // Ensure dropdown fits on screen
    let available_height = frame.area().height.saturating_sub(dropdown_y);
    let actual_height = dropdown_height.min(available_height).max(3);

    let dropdown_area = Rect::new(
        dropdown_x,
        dropdown_y,
        dropdown_width.min(config_area.width),
        actual_height,
    );

    // Clear the dropdown area first
    frame.render_widget(Clear, dropdown_area);

    // Build dropdown items
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let is_selected = i == app.port_select.config.dropdown_index;
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
