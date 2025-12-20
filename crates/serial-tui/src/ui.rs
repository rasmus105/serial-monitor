//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use serial_core::{encode, Direction as DataDirection};
use strum::IntoEnumIterator;

use crate::app::{App, ConfigField, ConnectionState, InputMode, PortSelectFocus, View};
use crate::wrap::wrap_line;

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
        View::PortSelect => render_port_select(frame, app, chunks[0]),
        View::Traffic => render_traffic(frame, app, chunks[0]),
    }

    render_status_bar(frame, app, chunks[1]);
}

fn render_port_select(frame: &mut Frame, app: &App, area: Rect) {
    // Split horizontally if config panel is visible
    let (port_area, config_area) = if app.port_select.config_panel_visible {
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

    let title = if app.port_select.ports.is_empty() {
        " Select Port [:: path, r: refresh, t: toggle config] "
    } else {
        " Select Port [j/k: nav, Enter: connect, :: path, r: refresh] "
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

    let border_style = if is_focused || dropdown_open {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Config [Enter: select, t: toggle] ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build config lines using ConfigField iterator
    let lines: Vec<Line> = ConfigField::iter()
        .map(|field| {
            let is_selected = app.port_select.config_field == field && (is_focused || dropdown_open);
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

            Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{}: ", field.label()), label_style),
                Span::styled(value, value_style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);

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
    let field_index = app.port_select.config_field.index();

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
            let is_selected = i == app.port_select.dropdown_index;
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

fn render_traffic(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = if app.file_send.handle.is_some() {
        // Show file send in progress
        let progress = app.file_send.progress.as_ref();
        let pct = progress.map(|p| (p.percentage() * 100.0) as u8).unwrap_or(0);
        format!(
            " Traffic [{}] [Sending file: {}% - press 'f' to cancel] ",
            app.traffic.encoding, pct
        )
    } else if app.search.pattern.is_some() {
        format!(
            " Traffic [{}] [/: search, n/N: next/prev, Esc: clear] ",
            app.traffic.encoding
        )
    } else {
        format!(
            " Traffic [{}] [/: search, e: encoding, i: send, f: file, q: quit] ",
            app.traffic.encoding
        )
    };

    let block = Block::default().title(title).borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let ConnectionState::Connected(ref handle) = app.connection {
        let buffer = handle.buffer();
        let chunks: Vec<_> = buffer.chunks().collect();

        if chunks.is_empty() {
            let msg =
                Paragraph::new("Waiting for data...").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner);
            return;
        }

        let content_width = inner.width as usize;

        // Build all physical rows from logical chunks
        let encoded_chunks: Vec<String> = chunks
            .iter()
            .map(|chunk| encode(&chunk.data, app.traffic.encoding))
            .collect();

        let mut all_physical_rows = Vec::new();

        for (idx, chunk) in chunks.iter().enumerate() {
            let is_match = app.search.match_index == Some(idx);
            let is_search_match = app.search.pattern.as_ref().is_some_and(|pattern| {
                encoded_chunks[idx]
                    .to_lowercase()
                    .contains(&pattern.to_lowercase())
            });

            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::Cyan),
            };

            let prefix = match chunk.direction {
                DataDirection::Tx => "TX: ",
                DataDirection::Rx => "RX: ",
            };

            // Highlight the line if it's the current match or contains search pattern
            let content_style = if is_match {
                direction_style.bg(Color::Yellow).fg(Color::Black)
            } else if is_search_match {
                direction_style.bg(Color::DarkGray)
            } else {
                direction_style
            };

            let prefix_style = if is_match {
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if is_search_match {
                direction_style
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                direction_style.add_modifier(Modifier::BOLD)
            };

            // Wrap this chunk into physical rows
            let physical_rows = wrap_line(
                prefix,
                prefix_style,
                &encoded_chunks[idx],
                content_style,
                idx,
                content_width,
            );

            all_physical_rows.extend(physical_rows);
        }

        // Resolve scroll_to_chunk to physical row offset
        if let Some(target_chunk) = app.traffic.scroll_to_chunk.take()
            && let Some(row_idx) = all_physical_rows
                .iter()
                .position(|pr| pr.chunk_index == target_chunk)
        {
            app.traffic.scroll_offset = row_idx;
        }

        // Calculate scroll based on physical rows
        let visible_height = inner.height as usize;
        let total_rows = all_physical_rows.len();
        let max_scroll = total_rows.saturating_sub(visible_height);
        let scroll = app.traffic.scroll_offset.min(max_scroll);
        app.traffic.scroll_offset = scroll;

        // Extract the visible physical rows
        let visible_rows: Vec<Line> = all_physical_rows
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .map(|pr| pr.line)
            .collect();

        // Render without wrapping - we've already handled it
        let paragraph = Paragraph::new(visible_rows);
        frame.render_widget(paragraph, inner);

        // Render scrollbar over the right border
        if total_rows > visible_height {
            let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█");

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
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
                InputMode::ConfigDropdown => {
                    (app.input.mode.entry_prompt(), Color::Cyan)
                }
                _ => (app.status.as_str(), Color::White),
            };
            let status = Paragraph::new(text).style(Style::default().fg(color));
            frame.render_widget(status, area);
        }
    }
}
