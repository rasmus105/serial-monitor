//! Traffic view rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};
use serial_core::{encode, Direction as DataDirection, GraphMode};
use strum::IntoEnumIterator;

use crate::app::{
    App, ConfigSection, ConnectionState, EnumNavigation, GraphConfigField, GraphFocus,
    HexGrouping, InputMode, PaneContent, PaneFocus, SearchMatch, TrafficConfigField,
    TrafficFocus, WrapMode,
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
    let config_has_focus = app.traffic.config.visible && app.traffic.focus == TrafficFocus::Config;

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
        render_pane_with_title(
            frame,
            app,
            chunks[0],
            primary_content,
            primary_focused,
            active_tab,
            true,
        );

        // Render secondary pane (right) - focused only if PaneFocus::Secondary AND config doesn't have focus
        let secondary_focused = pane_focus == PaneFocus::Secondary && !config_has_focus;
        render_pane_with_title(
            frame,
            app,
            chunks[1],
            secondary_content,
            secondary_focused,
            active_tab,
            false,
        );
    } else {
        // No split - primary pane is focused only if config panel doesn't have focus
        let primary_focused = !config_has_focus;
        render_pane_with_title(
            frame,
            app,
            area,
            primary_content,
            primary_focused,
            active_tab,
            true,
        );
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
        PaneContent::Traffic => {
            render_traffic_content_with_tab_bar(frame, app, area, focused, active_tab, is_primary)
        }
        PaneContent::Graph => {
            render_graph_pane_with_tab_bar(frame, app, area, focused, active_tab, is_primary)
        }
        PaneContent::AdvancedSend => {
            render_send_pane_with_tab_bar(frame, app, area, focused, active_tab, is_primary)
        }
    }
}

fn render_graph_pane_with_tab_bar(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    focused: bool,
    active_tab: u8,
    is_primary: bool,
) {
    // Initialize graph engine if not yet done
    if !app.graph.initialized {
        app.initialize_graph();
    }

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

    // Layout: graph content on left, config panel on right (if focused)
    let show_config = matches!(app.graph.focus, GraphFocus::Config) && focused;
    let chunks = if show_config {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(30)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(area)
    };

    // Render graph content
    let graph_area = chunks[0];
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(graph_area);
    frame.render_widget(block, graph_area);

    // Render the appropriate graph based on mode
    if let Some(ref engine) = app.graph.engine {
        match engine.mode() {
            GraphMode::PacketRate => {
                render_packet_rate_graph(frame, app, inner);
            }
            GraphMode::ParsedData => {
                render_parsed_data_graph(frame, app, inner);
            }
        }
    } else {
        let placeholder = Paragraph::new("No data yet. Connect to a serial port to see graphs.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, inner);
    }

    // Render config panel if showing
    if show_config {
        render_graph_config_panel(frame, app, chunks[1], focused);
    }
}

fn render_send_pane_with_tab_bar(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    focused: bool,
    active_tab: u8,
    is_primary: bool,
) {
    use crate::app::SendFocus;

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

    // Layout: send content on left, config panel on right (if focused on config)
    let show_config = matches!(app.send.focus, SendFocus::Config) && focused;
    let chunks = if show_config {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(30)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(area)
    };

    // Render send content
    let send_area = chunks[0];
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(send_area);
    frame.render_widget(block, send_area);

    // Render the send panel content
    render_send_content(frame, app, inner);

    // Render config panel if showing
    if show_config {
        render_send_config_panel(frame, app, chunks[1], focused);
    }
}

/// Render the main send panel content
fn render_send_content(frame: &mut Frame, app: &App, area: Rect) {
    // Split into sections: file info, progress, instructions
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // File send info
            Constraint::Length(4), // Progress
            Constraint::Min(3),    // Instructions/status
        ])
        .split(area);

    // File send info section
    let file_info = if app.send.config.file_path.is_empty() {
        vec![
            Line::from(Span::styled("No file selected", Style::default().fg(Color::DarkGray))),
            Line::from(""),
            Line::from(Span::styled(
                "Press Tab or 'c' to open config panel",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("File: ", Style::default().fg(Color::Gray)),
                Span::styled(&app.send.config.file_path, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Chunk size: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} bytes", app.send.config.chunk_size),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled("  Delay: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} ms", app.send.config.chunk_delay),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled("  Loop: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    if app.send.config.continuous { "ON" } else { "OFF" },
                    Style::default().fg(if app.send.config.continuous { Color::Green } else { Color::DarkGray }),
                ),
            ]),
        ]
    };
    let file_info_widget = Paragraph::new(file_info);
    frame.render_widget(file_info_widget, chunks[0]);

    // Progress section
    if let Some(ref progress) = app.file_send.progress {
        let percentage = (progress.percentage() * 100.0) as u16;
        let bar_width = (chunks[1].width.saturating_sub(2)) as usize;
        let filled = (bar_width as f64 * progress.percentage()) as usize;
        let empty = bar_width.saturating_sub(filled);

        let progress_bar = format!(
            "[{}{}] {}%",
            "=".repeat(filled),
            " ".repeat(empty),
            percentage
        );

        let status_text = if let Some(ref err) = progress.error {
            format!("Error: {}", err)
        } else if progress.complete {
            "Complete!".to_string()
        } else {
            format!(
                "Sending... {}/{} chunks ({}/{} bytes)",
                progress.chunks_sent,
                progress.total_chunks,
                progress.bytes_sent,
                progress.total_bytes
            )
        };

        let loop_info = if app.send.config.continuous && progress.loops_completed > 0 {
            format!(" (loop {})", progress.loops_completed + 1)
        } else {
            String::new()
        };

        let progress_lines = vec![
            Line::from(Span::styled(progress_bar, Style::default().fg(Color::Cyan))),
            Line::from(vec![
                Span::raw(status_text),
                Span::styled(loop_info, Style::default().fg(Color::Yellow)),
            ]),
        ];
        let progress_widget = Paragraph::new(progress_lines);
        frame.render_widget(progress_widget, chunks[1]);
    } else if app.file_send.handle.is_some() {
        let waiting = Paragraph::new("Starting file transfer...")
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(waiting, chunks[1]);
    }

    // Instructions section
    let instructions = if app.file_send.handle.is_some() {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("x", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(" or "),
                Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(" - Cancel transfer"),
            ]),
        ]
    } else if !app.send.config.file_path.is_empty() {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("s", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" or "),
                Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" - Start sending file"),
            ]),
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(" or "),
                Span::styled("c", Style::default().fg(Color::Cyan)),
                Span::raw(" - Toggle config panel"),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(" or "),
                Span::styled("c", Style::default().fg(Color::Cyan)),
                Span::raw(" - Open config panel to set file path"),
            ]),
        ]
    };
    let instructions_widget = Paragraph::new(instructions);
    frame.render_widget(instructions_widget, chunks[2]);
}

/// Render the send config panel using the generic ConfigPanelWidget
fn render_send_config_panel(frame: &mut Frame, app: &mut App, area: Rect, focused: bool) {
    use crate::ui::config_panel::ConfigPanelWidget;
    
    // Sync UI state flags from input mode
    app.send.config_panel.dropdown_open = app.input.mode == InputMode::SendConfigDropdown;
    app.send.config_panel.text_input_open = app.input.mode == InputMode::SendConfigTextInput;
    if app.send.config_panel.text_input_open {
        app.send.config_panel.text_buffer = app.input.buffer.clone();
    }
    
    // Determine if panel is focused
    let is_focused = focused && matches!(app.send.focus, crate::app::SendFocus::Config);
    
    // Render using the generic widget
    let widget = ConfigPanelWidget::new(&app.send.config, &mut app.send.config_panel)
        .title("Send Config")
        .focused(is_focused);
    
    frame.render_widget(widget, area);
    
    // Render dropdown overlay if in dropdown mode
    if app.input.mode == InputMode::SendConfigDropdown {
        render_send_config_dropdown_generic(frame, app, area);
    }
}

/// Render dropdown for send config using generic helpers
fn render_send_config_dropdown_generic(frame: &mut Frame, app: &App, config_area: Rect) {
    use crate::ui::config_panel::{get_dropdown_options, render_dropdown};
    use config::Configure;
    
    let schema = <crate::app::SendConfig as Configure>::schema();
    let field_index = app.send.config_panel.selected_field;
    
    if let Some(field_schema) = schema.fields.get(field_index) {
        let options = get_dropdown_options(&field_schema.field_type);
        if !options.is_empty() {
            // Calculate anchor position based on field index
            // Account for description header (2 lines) + field lines
            let anchor_y = (field_index + 2) as u16; // +2 for description header
            let anchor_x = 15; // After label
            
            render_dropdown(
                frame.buffer_mut(),
                config_area,
                &options,
                app.send.config_panel.dropdown_index,
                anchor_y,
                anchor_x,
            );
        }
    }
}

fn format_tab(index: u8, name: &str, active: bool) -> String {
    if active {
        format!("[{}:{}]", index, name)
    } else {
        format!("{}:{}", index, name)
    }
}

/// Build the tab bar title string
/// Format: " [1:Traffic] - 2:Graph - 3:Send | [extra info] "
fn build_tab_bar_title(active_tab: u8) -> String {
    format!(
        " {} - {} - {} ",
        format_tab(1, "Traffic", active_tab == 1),
        format_tab(2, "Graph", active_tab == 2),
        format_tab(3, "Send", active_tab == 3),
    )
}

// =============================================================================
// Traffic Content Rendering
// =============================================================================

fn render_traffic_content_with_tab_bar(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    focused: bool,
    active_tab: u8,
    is_primary: bool,
) {
    // Get dynamic keybinding hints
    let search_key = app
        .settings
        .keybindings
        .traffic
        .shortcut_hint(TrafficCommand::EnterSearchMode)
        .unwrap_or_else(|| "/".to_string());
    let config_key = app
        .settings
        .keybindings
        .traffic
        .shortcut_hint(TrafficCommand::ToggleConfigPanel)
        .unwrap_or_else(|| "c".to_string());
    let send_key = app
        .settings
        .keybindings
        .traffic
        .shortcut_hint(TrafficCommand::EnterSendMode)
        .unwrap_or_else(|| "i".to_string());

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
        format!(
            "{}| [{}] {}[Sending: {}%] ",
            tab_bar, app.traffic.encoding, filter_indicator, pct
        )
    } else if app.search.has_pattern() {
        let next_key = app
            .settings
            .keybindings
            .traffic
            .shortcut_hint(TrafficCommand::NextMatch)
            .unwrap_or_else(|| "n".to_string());
        let prev_key = app
            .settings
            .keybindings
            .traffic
            .shortcut_hint(TrafficCommand::PrevMatch)
            .unwrap_or_else(|| "N".to_string());
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
    let back_key = app
        .settings
        .keybindings
        .traffic
        .shortcut_hint(TrafficCommand::FocusTraffic)
        .unwrap_or_else(|| "h".to_string());
    let close_key = app
        .settings
        .keybindings
        .traffic
        .shortcut_hint(TrafficCommand::ToggleConfigPanel)
        .unwrap_or_else(|| "c".to_string());
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
        prev_section =
            push_section_separator(&mut lines, prev_section, field.section(), panel_width);

        let is_selected =
            app.traffic.config.field == field && (is_focused || dropdown_open || text_input_open);
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

// =============================================================================
// Graph Rendering
// =============================================================================

fn render_packet_rate_graph(frame: &mut Frame, app: &App, area: Rect) {
    let engine = match &app.graph.engine {
        Some(e) => e,
        None => return,
    };

    let rate_data = engine.packet_rate();

    // Calculate time bounds based on selected time window
    let now = std::time::SystemTime::now();
    let time_window = app.graph.time_window.as_duration();
    let start_time = time_window.map(|d| now - d);

    // Collect data points for RX and TX
    let mut rx_data: Vec<(f64, f64)> = Vec::new();
    let mut tx_data: Vec<(f64, f64)> = Vec::new();
    let mut min_time = f64::MAX;
    let mut max_time = f64::MIN;
    let mut max_count: f64 = 0.0;

    for sample in rate_data.samples() {
        // Filter by time window
        if let Some(start) = start_time {
            if sample.window_start < start {
                continue;
            }
        }

        // Convert timestamp to seconds since epoch for x-axis
        let time_secs = sample
            .window_start
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        min_time = min_time.min(time_secs);
        max_time = max_time.max(time_secs);

        if app.graph.show_rx {
            let rx_count = sample.rx_count as f64;
            rx_data.push((time_secs, rx_count));
            max_count = max_count.max(rx_count);
        }

        if app.graph.show_tx {
            let tx_count = sample.tx_count as f64;
            tx_data.push((time_secs, tx_count));
            max_count = max_count.max(tx_count);
        }
    }

    // Handle empty data
    if rx_data.is_empty() && tx_data.is_empty() {
        let placeholder = Paragraph::new("No packet data yet. Waiting for data...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, area);
        return;
    }

    // Ensure we have some range for the axes
    if (max_time - min_time).abs() < 0.001 {
        max_time = min_time + 1.0;
    }
    if max_count < 1.0 {
        max_count = 10.0;
    }

    // Format time labels (show relative seconds)
    let time_span = max_time - min_time;
    let x_labels = vec![
        Span::raw(format!("-{:.0}s", time_span)),
        Span::raw(format!("-{:.0}s", time_span / 2.0)),
        Span::raw("now"),
    ];

    // Build datasets
    let mut datasets = Vec::new();

    if app.graph.show_rx && !rx_data.is_empty() {
        datasets.push(
            Dataset::default()
                .name("RX")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&rx_data),
        );
    }

    if app.graph.show_tx && !tx_data.is_empty() {
        datasets.push(
            Dataset::default()
                .name("TX")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Yellow))
                .data(&tx_data),
        );
    }

    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::Gray))
                .bounds([min_time, max_time])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title("Packets")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, max_count * 1.1])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{:.0}", max_count / 2.0)),
                    Span::raw(format!("{:.0}", max_count)),
                ]),
        );

    frame.render_widget(chart, area);
}

fn render_parsed_data_graph(frame: &mut Frame, app: &App, area: Rect) {
    let engine = match &app.graph.engine {
        Some(e) => e,
        None => return,
    };

    let parsed_data = engine.parsed_data();

    if parsed_data.is_empty() {
        let placeholder = Paragraph::new(
            "No parsed data yet.\nEnsure your data contains key=value patterns.",
        )
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, area);
        return;
    }

    // Calculate time bounds
    let now = std::time::SystemTime::now();
    let time_window = app.graph.time_window.as_duration();
    let start_time = time_window.map(|d| now - d);

    // Collect data for all visible series
    let mut all_data: Vec<(String, Vec<(f64, f64)>, Color)> = Vec::new();
    let mut min_time = f64::MAX;
    let mut max_time = f64::MIN;
    let mut min_val = f64::MAX;
    let mut max_val = f64::MIN;

    let colors = [
        Color::Green,
        Color::Yellow,
        Color::Cyan,
        Color::Magenta,
        Color::Red,
        Color::Blue,
    ];

    for (idx, series) in parsed_data.all_series().enumerate() {
        if !series.visible {
            continue;
        }

        let mut series_data: Vec<(f64, f64)> = Vec::new();
        let color = colors[idx % colors.len()];

        for point in &series.points {
            // Filter by time window
            if let Some(start) = start_time {
                if point.timestamp < start {
                    continue;
                }
            }

            // Filter by direction if needed
            if !app.graph.show_rx && point.direction == serial_core::Direction::Rx {
                continue;
            }
            if !app.graph.show_tx && point.direction == serial_core::Direction::Tx {
                continue;
            }

            let time_secs = point
                .timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();

            min_time = min_time.min(time_secs);
            max_time = max_time.max(time_secs);
            min_val = min_val.min(point.value);
            max_val = max_val.max(point.value);

            series_data.push((time_secs, point.value));
        }

        if !series_data.is_empty() {
            all_data.push((series.name.clone(), series_data, color));
        }
    }

    if all_data.is_empty() {
        let placeholder =
            Paragraph::new("No visible data in selected time window.")
                .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, area);
        return;
    }

    // Ensure we have some range for the axes
    if (max_time - min_time).abs() < 0.001 {
        max_time = min_time + 1.0;
    }
    if (max_val - min_val).abs() < 0.001 {
        min_val -= 1.0;
        max_val += 1.0;
    }

    // Add some padding to y-axis
    let y_range = max_val - min_val;
    min_val -= y_range * 0.1;
    max_val += y_range * 0.1;

    // Format time labels
    let time_span = max_time - min_time;
    let x_labels = vec![
        Span::raw(format!("-{:.0}s", time_span)),
        Span::raw(format!("-{:.0}s", time_span / 2.0)),
        Span::raw("now"),
    ];

    // Build datasets - need to store data in a way that outlives the dataset references
    let datasets: Vec<Dataset> = all_data
        .iter()
        .map(|(name, data, color)| {
            Dataset::default()
                .name(name.as_str())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(*color))
                .data(data)
        })
        .collect();

    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::Gray))
                .bounds([min_time, max_time])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title("Value")
                .style(Style::default().fg(Color::Gray))
                .bounds([min_val, max_val])
                .labels(vec![
                    Span::raw(format!("{:.1}", min_val)),
                    Span::raw(format!("{:.1}", (min_val + max_val) / 2.0)),
                    Span::raw(format!("{:.1}", max_val)),
                ]),
        );

    frame.render_widget(chart, area);
}

fn render_graph_config_panel(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let dropdown_open = app.input.mode == InputMode::GraphConfigDropdown;
    let text_input_open = app.input.mode == InputMode::GraphConfigTextInput;
    
    let border_style = if (focused && matches!(app.graph.focus, GraphFocus::Config)) || dropdown_open || text_input_open {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Config ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build config items
    let mut lines: Vec<Line> = Vec::new();
    let is_focused = focused && matches!(app.graph.focus, GraphFocus::Config);
    let panel_width = inner.width as usize;

    // Settings section header
    let settings_sep = create_separator("Settings", panel_width);
    lines.push(Line::from(Span::styled(
        settings_sep,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    for field in GraphConfigField::iter() {
        // Skip RegexPattern if parser is not Regex type
        if field == GraphConfigField::RegexPattern && !app.graph.should_show_regex_pattern() {
            continue;
        }

        let is_selected = app.graph.config.field == field && (is_focused || dropdown_open || text_input_open);

        let name: &'static str = field.into();
        let value = app.graph.get_config_display(field);

        // For text input fields being edited, show the input buffer with cursor
        let is_editing_this_field = text_input_open && is_selected && field.is_text_input();
        let display_value = if is_editing_this_field {
            format!("{}▌", app.input.buffer)
        } else {
            value.clone()
        };

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

        let prefix = if is_selected { "> " } else { "  " };

        // For boolean toggles, show a checkbox-style indicator
        if field.is_toggle() {
            let (indicator, color) = if value == "ON" {
                ("[x]", Color::Green)
            } else {
                ("[ ]", Color::DarkGray)
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{}: ", name), label_style),
                Span::styled(indicator, Style::default().fg(color)),
            ]));
        } else if field.is_text_input() {
            // Text input field - may need wrapping for long patterns
            let label_text = format!("{}{}: ", prefix, name);
            let label_len = label_text.chars().count();
            let available_width = panel_width.saturating_sub(label_len);

            if display_value.chars().count() <= available_width {
                lines.push(Line::from(vec![
                    Span::styled(prefix, label_style),
                    Span::styled(format!("{}: ", name), label_style),
                    Span::styled(display_value.clone(), value_style),
                ]));
            } else {
                // Wrap long values
                let chars: Vec<char> = display_value.chars().collect();
                let first_line_chars: String = chars.iter().take(available_width).collect();
                let remaining: String = chars.iter().skip(available_width).collect();

                lines.push(Line::from(vec![
                    Span::styled(prefix, label_style),
                    Span::styled(format!("{}: ", name), label_style),
                    Span::styled(first_line_chars, value_style),
                ]));

                // Continuation lines
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
            // Dropdown field
            let indicator = " ▼";
            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{}: ", name), label_style),
                Span::styled(display_value.clone(), value_style),
                Span::styled(indicator, Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Add series section if in ParsedData mode and there are series
    let series_names = app.graph.series_names();
    if !series_names.is_empty() 
        && app.graph.engine.as_ref().map(|e| e.mode()) == Some(GraphMode::ParsedData) 
    {
        lines.push(Line::from("")); // Spacer
        let series_sep = create_separator("Series", panel_width);
        lines.push(Line::from(Span::styled(
            series_sep,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));

        for name in &series_names {
            let is_visible = app.graph.is_series_visible(name);
            let (indicator, color) = if is_visible {
                ("[x]", Color::Green)
            } else {
                ("[ ]", Color::DarkGray)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}: ", name), Style::default()),
                Span::styled(indicator, Style::default().fg(color)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);

    // Render dropdown if open
    if dropdown_open {
        render_graph_config_dropdown(frame, app, area);
    }
}

fn render_graph_config_dropdown(frame: &mut Frame, app: &App, config_area: Rect) {
    let options = app.graph.get_config_option_strings();
    if options.is_empty() {
        return;
    }

    let field_index = GraphConfigField::iter()
        .position(|f| f == app.graph.config.field)
        .unwrap_or(0);

    // Calculate dropdown dimensions
    let dropdown_width = options.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6;
    let dropdown_height = (options.len() + 2) as u16;

    // Position dropdown next to the selected field
    let dropdown_y = config_area.y + 1 + field_index as u16;
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
            let is_selected = i == app.graph.config.dropdown_index;
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
