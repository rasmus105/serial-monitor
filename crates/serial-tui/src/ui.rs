//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs,
    },
    Frame,
};
use serial_core::{encode, Direction as DataDirection};
use strum::IntoEnumIterator;

use crate::app::{
    App, ConfigField, ConnectionState, HexGrouping, InputMode, PortSelectFocus, TrafficConfigField,
    TrafficFocus, View, WrapMode,
};
use crate::settings::{AnyCommand, SettingsTab};
use crate::wrap::{truncate_line, wrap_line, GutterConfig};

/// Create a centered separator line like "──── Title ────" that spans the full width
fn create_separator(title: &str, width: usize) -> String {
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

/// Format hex string with specified grouping
/// Input: "DE AD BE EF" (space-separated bytes from core)
/// Output depends on grouping:
///   - None: "DEADBEEF" (no spaces)
///   - Byte: "DE AD BE EF" (space every byte, unchanged)
///   - Word: "DEAD BEEF" (space every 2 bytes)
///   - DWord: "DEADBEEF" for 4 bytes, "DEADBEEF 12345678" for 8 bytes
fn format_hex_grouped(hex: &str, grouping: HexGrouping) -> String {
    match grouping {
        HexGrouping::Byte => hex.to_string(), // Already space-separated per byte
        HexGrouping::None => hex.replace(' ', ""),
        HexGrouping::Word | HexGrouping::DWord => {
            // Remove existing spaces and regroup
            let compact: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes_per_group = grouping.bytes_per_group();
            let chars_per_group = bytes_per_group * 2; // 2 hex chars per byte
            
            compact
                .as_bytes()
                .chunks(chars_per_group)
                .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

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

    // Render settings panel as overlay if open
    if app.settings_panel.open {
        render_settings_panel(frame, app);
    }
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
    // Split horizontally if config panel is visible (70/30 split)
    let (traffic_area, config_area) = if app.traffic.config_panel_visible {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Render the main traffic content
    render_traffic_content(frame, app, traffic_area);

    // Render config panel if visible
    if let Some(config_area) = config_area {
        render_traffic_config_panel(frame, app, config_area);
    }
}

fn render_traffic_content(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.traffic.focus == TrafficFocus::Traffic;

    let title = if app.file_send.handle.is_some() {
        // Show file send in progress
        let progress = app.file_send.progress.as_ref();
        let pct = progress.map(|p| (p.percentage() * 100.0) as u8).unwrap_or(0);
        format!(
            " Traffic [{}] [Sending: {}%] ",
            app.traffic.encoding, pct
        )
    } else if app.search.pattern.is_some() {
        format!(
            " Traffic [{}] [/: search, n/N: next/prev] ",
            app.traffic.encoding
        )
    } else {
        format!(
            " Traffic [{}] [c: config, /: search, e: enc, i: send] ",
            app.traffic.encoding
        )
    };

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let ConnectionState::Connected(ref handle) = app.connection {
        let buffer = handle.buffer();
        let all_chunks: Vec<_> = buffer.chunks().collect();

        // Filter chunks based on show_tx and show_rx settings
        let chunks: Vec<_> = all_chunks
            .iter()
            .enumerate()
            .filter(|(_, chunk)| match chunk.direction {
                DataDirection::Tx => app.traffic.show_tx,
                DataDirection::Rx => app.traffic.show_rx,
            })
            .collect();

        if chunks.is_empty() {
            let msg = if all_chunks.is_empty() {
                "Waiting for data..."
            } else {
                "No data matches current filters (check Show TX/RX settings)"
            };
            let paragraph =
                Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(paragraph, inner);
            // Update cached values for scroll logic
            app.traffic.total_rows = 0;
            app.traffic.visible_height = inner.height as usize;
            return;
        }

        let content_width = inner.width as usize;

        // Calculate line number width based on total visible chunks
        let line_number_width = if app.traffic.show_line_numbers {
            chunks.len().to_string().len().max(3)
        } else {
            0
        };

        // Get session start time for relative timestamps
        let session_start = app.traffic.session_start.unwrap_or_else(|| {
            all_chunks
                .first()
                .map(|c| c.timestamp)
                .unwrap_or_else(std::time::SystemTime::now)
        });

        // Gutter style: muted and bold
        let gutter_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        // Build all physical rows from logical chunks
        let encoded_chunks: Vec<String> = chunks
            .iter()
            .map(|(_, chunk)| {
                let encoded = encode(&chunk.data, app.traffic.encoding);
                // Apply hex grouping if in hex mode
                if app.traffic.encoding == serial_core::Encoding::Hex {
                    format_hex_grouped(&encoded, app.traffic.hex_grouping)
                } else {
                    encoded
                }
            })
            .collect();

        let mut all_physical_rows = Vec::new();

        for (display_idx, (original_idx, chunk)) in chunks.iter().enumerate() {
            let is_match = app.search.match_index == Some(*original_idx);
            let is_search_match = app.search.pattern.as_ref().is_some_and(|pattern| {
                encoded_chunks[display_idx]
                    .to_lowercase()
                    .contains(&pattern.to_lowercase())
            });

            // Use color to indicate direction
            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::White),
            };

            // Highlight the line if it's the current match or contains search pattern
            let content_style = if is_match {
                direction_style.bg(Color::Yellow).fg(Color::Black)
            } else if is_search_match {
                direction_style.bg(Color::DarkGray)
            } else {
                direction_style
            };

            // Build gutter config for this chunk
            let gutter = GutterConfig {
                line_number: if app.traffic.show_line_numbers {
                    Some(display_idx + 1) // 1-indexed based on filtered list
                } else {
                    None
                },
                line_number_width,
                timestamp: if app.traffic.show_timestamps {
                    Some(app.traffic.timestamp_format.format(chunk.timestamp, session_start))
                } else {
                    None
                },
                style: gutter_style,
            };

            // Wrap or truncate this chunk into physical rows based on wrap mode
            let physical_rows = match app.traffic.wrap_mode {
                WrapMode::Wrap => wrap_line(
                    &gutter,
                    &encoded_chunks[display_idx],
                    content_style,
                    *original_idx,
                    content_width,
                ),
                WrapMode::Truncate => truncate_line(
                    &gutter,
                    &encoded_chunks[display_idx],
                    content_style,
                    *original_idx,
                    content_width,
                ),
            };

            all_physical_rows.extend(physical_rows);
        }

        // Resolve scroll_to_chunk to physical row offset
        if let Some(target_chunk) = app.traffic.scroll_to_chunk.take()
            && let Some(row_idx) = all_physical_rows
                .iter()
                .position(|pr| pr.chunk_index == target_chunk)
        {
            app.traffic.scroll_offset = row_idx;
            app.traffic.was_at_bottom = false;
        }

        // Calculate scroll based on physical rows
        let visible_height = inner.height as usize;
        let total_rows = all_physical_rows.len();
        let max_scroll = total_rows.saturating_sub(visible_height);

        // Update cached values for scroll calculations
        app.traffic.total_rows = total_rows;
        app.traffic.visible_height = visible_height;

        // Handle auto-scroll and lock-to-bottom
        let scroll = if app.traffic.lock_to_bottom {
            // Lock to bottom: always show the bottom
            max_scroll
        } else if app.traffic.auto_scroll && app.traffic.was_at_bottom {
            // Auto-scroll: if we were at bottom, stay at bottom
            max_scroll
        } else {
            // Normal scroll: respect user's scroll position
            app.traffic.scroll_offset.min(max_scroll)
        };

        app.traffic.scroll_offset = scroll;

        // Update was_at_bottom for next frame (for auto-scroll logic)
        app.traffic.was_at_bottom = scroll >= max_scroll;

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

fn render_traffic_config_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.traffic.focus == TrafficFocus::Config;
    let dropdown_open = app.input.mode == InputMode::TrafficConfigDropdown;

    let border_style = if is_focused || dropdown_open {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Config [h: back, c: close] ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Get connection info for display
    let (port_name, baud_rate) = if let ConnectionState::Connected(ref handle) = app.connection {
        (
            handle.port_name().to_string(),
            app.port_select.serial_config.baud_rate.to_string(),
        )
    } else {
        ("Not connected".to_string(), "-".to_string())
    };

    // Create full-width separators
    let panel_width = inner.width as usize;
    let connection_sep = create_separator("Connection", panel_width);
    let settings_sep = create_separator("Settings", panel_width);

    let mut lines: Vec<Line> = vec![
        // Header: Connection Info (read-only)
        Line::from(Span::styled(
            connection_sep,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  Port: ", Style::default().fg(Color::DarkGray)),
            Span::styled(port_name, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Baud: ", Style::default().fg(Color::DarkGray)),
            Span::styled(baud_rate, Style::default().fg(Color::White)),
        ]),
        Line::from(""), // Spacer
        // Header: Settings
        Line::from(Span::styled(
            settings_sep,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )),
    ];

    // Build config lines using TrafficConfigField iterator
    for field in TrafficConfigField::iter() {
        let is_selected = app.traffic.config_field == field && (is_focused || dropdown_open);
        let prefix = if is_selected { "> " } else { "  " };

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let value = app.traffic.get_config_display(field);

        // For boolean toggles, show a checkbox-style indicator
        let value_span = if field.is_toggle() {
            let (indicator, color) = if value == "ON" {
                ("[x]", Color::Green)
            } else {
                ("[ ]", Color::DarkGray)
            };
            Span::styled(indicator, Style::default().fg(color))
        } else {
            let value_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            Span::styled(value, value_style)
        };

        // Build label with optional shortcut hint (from configurable keybindings)
        let shortcut_style = Style::default().fg(Color::DarkGray);
        let shortcut_hint = field
            .associated_command()
            .and_then(|cmd| app.settings.keybindings.traffic.shortcut_hint(cmd));
        let label_with_shortcut = if let Some(key) = shortcut_hint {
            vec![
                Span::styled(prefix, label_style),
                Span::styled(field.label(), label_style),
                Span::styled(format!(" ({}):", key), shortcut_style),
                Span::raw(" "),
                value_span,
            ]
        } else {
            vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{}: ", field.label()), label_style),
                value_span,
            ]
        };

        lines.push(Line::from(label_with_shortcut));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);

    // Render dropdown popup if open
    if dropdown_open {
        render_traffic_config_dropdown(frame, app, area);
    }
}

fn render_traffic_config_dropdown(frame: &mut Frame, app: &App, config_area: Rect) {
    let options = app.traffic.get_config_option_strings();
    if options.is_empty() {
        return;
    }

    let dropdown_height = (options.len() + 2) as u16; // +2 for borders
    let dropdown_width = options.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6;

    // Position the dropdown based on which field is selected
    // Account for the header lines (Connection section + spacer = 5 lines)
    let header_lines = 5u16;
    let field_index = app.traffic.config_field.index();

    // Position dropdown next to the selected field
    let dropdown_y = config_area.y + 1 + header_lines + field_index as u16;
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
            let is_selected = i == app.traffic.dropdown_index;
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
                InputMode::ConfigDropdown | InputMode::TrafficConfigDropdown => {
                    (app.input.mode.entry_prompt(), Color::Cyan)
                }
                _ => (app.status.as_str(), Color::White),
            };
            let status = Paragraph::new(text).style(Style::default().fg(color));
            frame.render_widget(status, area);
        }
    }
}

// =============================================================================
// Settings Panel
// =============================================================================

/// Calculate centered rect with given percentage of the area
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

fn render_settings_panel(frame: &mut Frame, app: &App) {
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

fn render_keybindings_tab(frame: &mut Frame, app: &App, area: Rect) {
    let all_commands = AnyCommand::all();
    let visible_height = area.height as usize;

    // Calculate visible range based on scroll offset
    let scroll_offset = app.settings_panel.scroll_offset;
    let start = scroll_offset;
    let end = (scroll_offset + visible_height).min(all_commands.len());

    let mut lines: Vec<Line> = Vec::new();
    let mut current_category = "";

    for (idx, cmd) in all_commands.iter().enumerate().skip(start).take(end - start) {
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
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(all_commands.len())
            .position(app.settings_panel.selected_command);

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
    let help_text = if app.settings_panel.recording_key {
        "Press any key to add binding | Esc: Cancel"
    } else {
        "j/k: Navigate | Ctrl+u/d: Page | a: Add | d: Delete | r: Reset | Esc: Close"
    };

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(help, area);
}

