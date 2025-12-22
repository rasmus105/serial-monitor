//! Traffic view rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use serial_core::{encode, Direction as DataDirection};
use strum::IntoEnumIterator;

use crate::app::{
    App, ConfigSection, ConnectionState, EnumNavigation, HexGrouping, InputMode, PaneContent,
    PaneFocus, SearchMatch, TrafficConfigField, TrafficFocus, WrapMode,
};
use crate::command::TrafficCommand;
use crate::wrap::{truncate_line_styled, wrap_line_styled, GutterConfig, StyledSegment};

use super::{create_separator, push_section_separator};

// =============================================================================
// Hex Formatting
// =============================================================================

/// Format hex string with specified grouping
/// Input: "DE AD BE EF" (space-separated bytes from core)
/// Output depends on grouping:
///   - None: "DEADBEEF" (no spaces)
///   - Byte: "DE AD BE EF" (space every byte, unchanged)
///   - Word: "DEAD BEEF" (space every 2 bytes)
///   - DWord: "DEADBEEF" for 4 bytes, "DEADBEEF 12345678" for 8 bytes
pub fn format_hex_grouped(hex: &str, grouping: HexGrouping) -> String {
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

// =============================================================================
// Search Highlighting
// =============================================================================

/// Build styled segments for a chunk's content with search highlighting.
fn build_highlighted_segments(
    content: &str,
    chunk_index: usize,
    base_style: Style,
    matches: &[SearchMatch],
    current_match: Option<&SearchMatch>,
) -> Vec<StyledSegment> {
    // Filter matches to only those in this chunk
    let chunk_matches: Vec<&SearchMatch> = matches
        .iter()
        .filter(|m| m.chunk_index == chunk_index)
        .collect();

    if chunk_matches.is_empty() {
        return vec![StyledSegment {
            content: content.to_owned(),
            style: base_style,
        }];
    }

    // Styles for highlighting
    let current_highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);
    let other_highlight_style = base_style.bg(Color::DarkGray);

    let mut segments = Vec::new();
    let mut last_end = 0;

    for m in chunk_matches {
        // Sanity check byte ranges
        let start = m.byte_start.min(content.len());
        let end = m.byte_end.min(content.len());

        if start > last_end {
            // Non-matching prefix
            segments.push(StyledSegment {
                content: content[last_end..start].to_owned(),
                style: base_style,
            });
        }

        // The match itself
        let is_current = current_match.is_some_and(|cur| cur == m);
        let highlight_style = if is_current {
            current_highlight_style
        } else {
            other_highlight_style
        };

        segments.push(StyledSegment {
            content: content[start..end].to_owned(),
            style: highlight_style,
        });

        last_end = end;
    }

    // Remaining suffix
    if last_end < content.len() {
        segments.push(StyledSegment {
            content: content[last_end..].to_owned(),
            style: base_style,
        });
    }

    segments
}

// =============================================================================
// Connected View (Split layout)
// =============================================================================

pub(super) fn render_connected(frame: &mut Frame, app: &mut App, area: Rect) {
    // Config panel is always a 30% sidebar on the right when visible
    let (content_area, config_area) = if app.traffic.config.visible {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Render the tab layout (with potential splits)
    render_tab_layout(frame, app, content_area);

    // Render config panel as sidebar
    if let Some(config_area) = config_area {
        render_traffic_config_panel(frame, app, config_area);
    }
}

fn render_tab_layout(frame: &mut Frame, app: &mut App, area: Rect) {
    let active_tab = app.layout.active_tab_number();
    let primary_content = app.layout.primary_content();
    let secondary_content = app.layout.secondary();
    let split_ratio = app.layout.split_ratio();
    let pane_focus = app.layout.focus();
    
    // Determine if panes are focused (vs config panel having focus)
    let config_has_focus = app.traffic.config.visible 
        && app.traffic.focus == TrafficFocus::Config;
    
    if let Some(secondary_content) = secondary_content {
        // We have a split - create left | right layout
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(split_ratio),
                Constraint::Percentage(100 - split_ratio),
            ])
            .split(area);

        // Render primary pane (left) - focused only if PaneFocus::Primary AND config doesn't have focus
        let primary_focused = pane_focus == PaneFocus::Primary && !config_has_focus;
        render_pane_with_title(frame, app, chunks[0], primary_content, primary_focused, active_tab, true);

        // Render secondary pane (right) - focused only if PaneFocus::Secondary AND config doesn't have focus
        let secondary_focused = pane_focus == PaneFocus::Secondary && !config_has_focus;
        render_pane_with_title(frame, app, chunks[1], secondary_content, secondary_focused, active_tab, false);
    } else {
        // No split - primary pane is focused only if config panel doesn't have focus
        let primary_focused = !config_has_focus;
        render_pane_with_title(frame, app, area, primary_content, primary_focused, active_tab, true);
    }
}

fn render_pane_with_title(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    content: PaneContent,
    focused: bool,
    active_tab: u8,
    is_primary: bool,
) {
    match content {
        PaneContent::Traffic => render_traffic_pane_with_tab_bar(frame, app, area, focused, active_tab, is_primary),
        PaneContent::Graph => render_graph_pane_with_tab_bar(frame, area, focused, active_tab, is_primary),
        PaneContent::AdvancedSend => render_send_pane_with_tab_bar(frame, area, focused, active_tab, is_primary),
    }
}

fn render_traffic_pane_with_tab_bar(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    focused: bool,
    active_tab: u8,
    is_primary: bool,
) {
    render_traffic_content_with_tab_bar(frame, app, area, focused, active_tab, is_primary);
}

fn render_graph_pane_with_tab_bar(frame: &mut Frame, area: Rect, focused: bool, active_tab: u8, is_primary: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build title with tab bar only for primary pane
    let title = if is_primary {
        build_tab_bar_title(active_tab)
    } else {
        " Graph ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let placeholder = Paragraph::new("Graph view - Coming soon")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(placeholder, inner);
}

fn render_send_pane_with_tab_bar(frame: &mut Frame, area: Rect, focused: bool, active_tab: u8, is_primary: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build title with tab bar only for primary pane
    let title = if is_primary {
        build_tab_bar_title(active_tab)
    } else {
        " Send ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let placeholder = Paragraph::new("Advanced send options - Coming soon")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(placeholder, inner);
}

/// Build the tab bar title string
/// Format: " [1:Traffic] - 2:Graph - 3:Send | [extra info] "
fn build_tab_bar_title(active_tab: u8) -> String {
    let t1 = if active_tab == 1 { "[1:Traffic]" } else { "1:Traffic" };
    let t2 = if active_tab == 2 { "[2:Graph]" } else { "2:Graph" };
    let t3 = if active_tab == 3 { "[3:Send]" } else { "3:Send" };
    format!(" {} - {} - {} ", t1, t2, t3)
}

// =============================================================================
// Traffic Content Rendering
// =============================================================================

fn render_traffic_content_with_tab_bar(frame: &mut Frame, app: &mut App, area: Rect, focused: bool, active_tab: u8, is_primary: bool) {
    // Get dynamic keybinding hints
    let search_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::EnterSearchMode).unwrap_or_else(|| "/".to_string());
    let config_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::ToggleConfigPanel).unwrap_or_else(|| "c".to_string());
    let send_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::EnterSendMode).unwrap_or_else(|| "i".to_string());

    // Build title with tab bar (only for primary pane)
    let tab_bar = if is_primary {
        build_tab_bar_title(active_tab)
    } else {
        " Traffic ".to_string()
    };

    // Build filter indicator if filter is active
    let filter_indicator = if app.traffic.should_apply_filter(app.traffic.encoding) {
        let pattern = app.traffic.filter.pattern().unwrap_or("");
        format!("[Filter: {}] ", pattern)
    } else if app.traffic.filter_enabled && app.traffic.filter.has_pattern() {
        // Filter is enabled but not applied (wrong encoding)
        "[Filter: N/A] ".to_string()
    } else {
        String::new()
    };

    let title = if app.file_send.handle.is_some() {
        // Show file send in progress
        let progress = app.file_send.progress.as_ref();
        let pct = progress
            .map(|p| (p.percentage() * 100.0) as u8)
            .unwrap_or(0);
        format!("{}| [{}] {}[Sending: {}%] ", tab_bar, app.traffic.encoding, filter_indicator, pct)
    } else if app.search.has_pattern() {
        let next_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::NextMatch).unwrap_or_else(|| "n".to_string());
        let prev_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::PrevMatch).unwrap_or_else(|| "N".to_string());
        format!(
            "{}| [{}] {}[{}: search, {}/{}: next/prev] ",
            tab_bar, app.traffic.encoding, filter_indicator, search_key, next_key, prev_key
        )
    } else {
        format!(
            "{}| [{}] {}[{}: config, {}: search, {}: send] ",
            tab_bar, app.traffic.encoding, filter_indicator, config_key, search_key, send_key
        )
    };

    let border_style = if focused {
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

        // First pass: Filter chunks based on show_tx and show_rx settings
        let direction_filtered: Vec<_> = all_chunks
            .iter()
            .enumerate()
            .filter(|(_, chunk)| match chunk.direction {
                DataDirection::Tx => app.traffic.show_tx,
                DataDirection::Rx => app.traffic.show_rx,
            })
            .collect();

        // Check if we should apply text filter (only for ASCII/UTF-8 encodings)
        let apply_filter = app.traffic.should_apply_filter(app.traffic.encoding);

        // Second pass: Apply text filter if enabled
        let chunks: Vec<_> = if apply_filter {
            direction_filtered
                .into_iter()
                .filter(|(_, chunk)| {
                    let encoded = encode(&chunk.data, app.traffic.encoding);
                    app.traffic.matches_filter(&encoded)
                })
                .collect()
        } else {
            direction_filtered
        };

        if chunks.is_empty() {
            let msg = if all_chunks.is_empty() {
                "Waiting for data..."
            } else if apply_filter {
                "No data matches current filter"
            } else {
                "No data matches current filters (check Show TX/RX settings)"
            };
            let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
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

        // Get search state for highlighting
        let search_matches = app.search.matches();
        let current_match = app.search.current_match();

        let mut all_physical_rows = Vec::new();

        for (display_idx, (original_idx, chunk)) in chunks.iter().enumerate() {
            // Use color to indicate direction
            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::White),
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
                    Some(
                        app.traffic
                            .timestamp_format
                            .format(chunk.timestamp, session_start),
                    )
                } else {
                    None
                },
                style: gutter_style,
            };

            // Build styled segments with search highlighting
            let segments = build_highlighted_segments(
                &encoded_chunks[display_idx],
                *original_idx,
                direction_style,
                search_matches,
                current_match,
            );

            // Wrap or truncate this chunk into physical rows based on wrap mode
            let physical_rows = match app.traffic.wrap_mode {
                WrapMode::Wrap => wrap_line_styled(&gutter, segments, *original_idx, content_width),
                WrapMode::Truncate => {
                    truncate_line_styled(&gutter, segments, *original_idx, content_width)
                }
            };

            all_physical_rows.extend(physical_rows);
        }

        // Resolve scroll_to_chunk to physical row offset
        // Use scroll_off to show context above the match (like vim's scrolloff)
        const SCROLL_OFF: usize = 8;
        if let Some(target_chunk) = app.traffic.scroll_to_chunk.take()
            && let Some(row_idx) = all_physical_rows
                .iter()
                .position(|pr| pr.chunk_index == target_chunk)
        {
            // Position the match with SCROLL_OFF lines above it (if possible)
            app.traffic.scroll_offset = row_idx.saturating_sub(SCROLL_OFF);
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
                .begin_symbol(Some("┐"))
                .end_symbol(Some("┘"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

// =============================================================================
// Traffic Config Panel
// =============================================================================

fn render_traffic_config_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.traffic.focus == TrafficFocus::Config;
    let dropdown_open = app.input.mode == InputMode::TrafficConfigDropdown;
    let text_input_open = app.input.mode == InputMode::TrafficConfigTextInput;

    let border_style = if is_focused || dropdown_open || text_input_open {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build dynamic title
    let back_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::FocusTraffic).unwrap_or_else(|| "h".to_string());
    let close_key = app.settings.keybindings.traffic.shortcut_hint(TrafficCommand::ToggleConfigPanel).unwrap_or_else(|| "c".to_string());
    let title = format!(" Config [{}: back, {}: close] ", back_key, close_key);

    let block = Block::default()
        .title(title)
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

    // Create full-width separators for custom headers
    let panel_width = inner.width as usize;
    let connection_sep = create_separator("Connection", panel_width);
    let settings_sep = create_separator("Settings", panel_width);

    let mut lines: Vec<Line> = vec![
        // Header: Connection Info (read-only)
        Line::from(Span::styled(
            connection_sep,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
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
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    // Build config lines using TrafficConfigField iterator
    // TrafficDisplay section has no header (first section after Settings)
    let mut prev_section: Option<ConfigSection> = Some(ConfigSection::TrafficDisplay);

    for field in TrafficConfigField::iter() {
        // Add separator when section changes
        prev_section = push_section_separator(&mut lines, prev_section, field.section(), panel_width);

        let is_selected = app.traffic.config.field == field && (is_focused || dropdown_open || text_input_open);
        let prefix = if is_selected { "> " } else { "  " };

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let value = app.traffic.get_config_display(field);

        // For text input fields being edited, show the input buffer with cursor
        let is_editing_this_field = text_input_open && is_selected && field.is_text_input();
        let display_value = if is_editing_this_field {
            format!("{}▌", app.input.buffer)
        } else {
            value.clone()
        };

        let value_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };

        // For boolean toggles, show a checkbox-style indicator
        let value_span = if field.is_toggle() {
            let (indicator, color) = if value == "ON" {
                ("[x]", Color::Green)
            } else {
                ("[ ]", Color::DarkGray)
            };
            Span::styled(indicator, Style::default().fg(color))
        } else {
            Span::styled(display_value.clone(), value_style)
        };

        // Build label with optional shortcut hint (from configurable keybindings)
        let shortcut_style = Style::default().fg(Color::DarkGray);
        let shortcut_hint = field
            .associated_command()
            .and_then(|cmd| app.settings.keybindings.traffic.shortcut_hint(cmd));
        
        // For SaveDirectory, wrap the value if it's too long
        if field == TrafficConfigField::SaveDirectory {
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
    }

    // Calculate visible height and apply scroll
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let scroll_offset = app.traffic.config.scroll_offset;
    
    // Only scroll if content exceeds visible height
    let needs_scroll = total_lines > visible_height;
    let actual_scroll = if needs_scroll {
        scroll_offset.min(total_lines.saturating_sub(visible_height))
    } else {
        0
    };
    
    // Take only the visible lines
    let visible_lines: Vec<Line> = lines.into_iter().skip(actual_scroll).take(visible_height).collect();

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
    let field_index = app.traffic.config.field.index();

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
            let is_selected = i == app.traffic.config.dropdown_index;
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
