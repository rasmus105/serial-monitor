//! Traffic view: main data display with search and send functionality.

use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};
use serial_core::{
    Direction as DataDirection, SerialConfig, SessionHandle,
    buffer::{PatternMode, SearchMatch},
    ui::{
        TimestampFormat,
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
        encoding::{ENCODING_DISPLAY_NAMES, ENCODING_VARIANTS},
    },
};

use crate::{
    app::{Focus, TrafficAction},
    theme::Theme,
    widget::{ConfigKeyResult, ConfigPanel, TextInput, handle_config_key, text_input::TextInputState},
};

/// Traffic view state.
pub struct TrafficView {
    /// Current scroll position (in display lines when wrapped, chunks when truncated).
    pub scroll: usize,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
    /// Filter input state.
    pub filter_input: TextInputState,
    /// Whether filter input is focused.
    pub filter_focused: bool,
    /// Send input state.
    pub send_input: TextInputState,
    /// Whether send input is focused.
    pub send_focused: bool,
    /// Traffic config.
    pub config: TrafficConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// Session start time for relative timestamps.
    pub session_start: Option<SystemTime>,
    /// Last known visible height (for scroll bounds calculation).
    last_visible_height: usize,
    /// Last known content width (for wrap calculation in key handler).
    last_content_width: usize,
}

/// Traffic view configuration.
#[derive(Debug, Clone)]
pub struct TrafficConfig {
    pub encoding_index: usize,
    pub show_tx: bool,
    pub show_rx: bool,
    pub show_timestamps: bool,
    pub timestamp_format_index: usize,
    pub auto_scroll: bool,
    pub pattern_mode_index: usize,
    pub wrap_text: bool,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            encoding_index: 0, // UTF-8
            show_tx: true,
            show_rx: true,
            show_timestamps: true,
            timestamp_format_index: 0, // Relative
            auto_scroll: true,
            pattern_mode_index: 0, // Normal
            wrap_text: true, // Wrap by default
        }
    }
}

impl TrafficConfig {
    /// Get the timestamp format from the index.
    pub fn timestamp_format(&self) -> TimestampFormat {
        match self.timestamp_format_index {
            0 => TimestampFormat::Relative,
            1 => TimestampFormat::AbsoluteMillis,
            2 => TimestampFormat::Absolute,
            _ => TimestampFormat::Relative,
        }
    }
}

// Config panel definitions
const ENCODING_OPTIONS: &[&str] = ENCODING_DISPLAY_NAMES;
const PATTERN_MODE_OPTIONS: &[&str] = &["Normal", "Regex"];
const TIMESTAMP_FORMAT_OPTIONS: &[&str] = &["Relative", "HH:MM:SS.mmm", "HH:MM:SS"];

static TRAFFIC_CONFIG_SECTIONS: &[Section<TrafficConfig>] = &[
    Section {
        header: Some("Display"),
        fields: &[
            FieldDef {
                id: "encoding",
                label: "Encoding",
                kind: FieldKind::Select {
                    options: ENCODING_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.encoding_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.encoding_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "show_timestamps",
                label: "Timestamps",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_timestamps),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_timestamps = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "timestamp_format",
                label: "Time Format",
                kind: FieldKind::Select {
                    options: TIMESTAMP_FORMAT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.timestamp_format_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.timestamp_format_index = i;
                    }
                },
                visible: |c| c.show_timestamps,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_scroll",
                label: "Auto Scroll",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_scroll),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_scroll = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "wrap_text",
                label: "Wrap Text",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.wrap_text),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.wrap_text = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Filter"),
        fields: &[
            FieldDef {
                id: "show_tx",
                label: "Show TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_tx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_tx = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "show_rx",
                label: "Show RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_rx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_rx = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "pattern_mode",
                label: "Pattern Mode",
                kind: FieldKind::Select {
                    options: PATTERN_MODE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.pattern_mode_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.pattern_mode_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
];

// Connection info section (read-only) - common baud rates for display
#[allow(dead_code)]
const BAUD_RATE_DISPLAY: &[&str] = &[
    "300", "1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "230400", "460800",
    "921600",
];

impl TrafficView {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            search_input: TextInputState::new().with_placeholder("Search pattern..."),
            search_focused: false,
            filter_input: TextInputState::new().with_placeholder("Filter pattern..."),
            filter_focused: false,
            send_input: TextInputState::new().with_placeholder("Data to send..."),
            send_focused: false,
            config: TrafficConfig::default(),
            config_nav: ConfigPanelNav::new(),
            session_start: None,
            last_visible_height: 20, // Conservative default
            last_content_width: 80,  // Conservative default
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused || self.filter_focused || self.send_focused
    }

    /// Sync config changes to the session buffer
    pub fn sync_config_to_buffer(&self, handle: &SessionHandle) {
        let mut buffer = handle.buffer_mut();
        
        // Sync encoding
        let encoding = ENCODING_VARIANTS[self.config.encoding_index];
        buffer.set_encoding(encoding);
        
        // Sync show_tx/show_rx
        buffer.set_show_tx(self.config.show_tx);
        buffer.set_show_rx(self.config.show_rx);
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        handle: &SessionHandle,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        // Main area layout: traffic + optional search/filter/send bar
        let show_input_bar = self.search_focused || self.filter_focused || self.send_focused;
        let main_chunks = if show_input_bar {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(main_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3)])
                .split(main_area)
        };

        // Draw traffic
        self.draw_traffic(main_chunks[0], buf, handle, focus);

        // Draw input bar if active
        if show_input_bar {
            self.draw_input_bar(main_chunks[1], buf, handle);
        }

        // Draw config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, serial_config, focus);
        }
    }

    fn draw_traffic(&mut self, area: Rect, buf: &mut Buffer, handle: &SessionHandle, focus: Focus) {
        // Update search matches before we get an immutable borrow
        // This ensures matches are current for highlighting
        let _ = handle.buffer_mut().matches();
        
        let buffer = handle.buffer();
        let current_match = buffer.current_match().copied();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main && !self.is_input_mode() {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let inner = block.inner(area);
        
        if inner.height == 0 || inner.width == 0 {
            block.render(area, buf);
            return;
        }

        let visible_height = inner.height as usize;
        let inner_width = inner.width as usize;

        // Calculate prefix width: "TX " or "RX " = 3 chars
        let base_prefix_width = 3;
        
        // For relative timestamps, calculate the max width needed for alignment
        let timestamp_width = if self.config.show_timestamps {
            match self.config.timestamp_format() {
                TimestampFormat::Relative => {
                    // Find the maximum timestamp width across all chunks
                    let session_start = self.session_start.unwrap_or_else(SystemTime::now);
                    buffer
                        .chunks()
                        .map(|chunk| {
                            let elapsed = chunk.timestamp.duration_since(session_start).unwrap_or_default();
                            let secs = elapsed.as_secs_f64();
                            format!("+{:.3}s", secs).len()
                        })
                        .max()
                        .unwrap_or(7)
                }
                TimestampFormat::AbsoluteMillis => 12,
                TimestampFormat::Absolute => 8,
            }
        } else {
            0
        };

        let prefix_width = base_prefix_width + if self.config.show_timestamps { timestamp_width + 1 } else { 0 };
        let content_width = inner_width.saturating_sub(prefix_width);

        // Store for key handler calculations
        self.last_content_width = content_width;

        if self.config.wrap_text {
            self.draw_traffic_wrapped(
                area, buf, handle, &buffer, inner, visible_height, 
                content_width, prefix_width, timestamp_width, block,
                current_match.as_ref(),
            );
        } else {
            self.draw_traffic_truncated(
                area, buf, handle, &buffer, inner, visible_height,
                content_width, prefix_width, timestamp_width, block,
                current_match.as_ref(),
            );
        }
    }

    fn draw_traffic_truncated(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _handle: &SessionHandle,
        buffer: &serial_core::buffer::DataBuffer,
        inner: Rect,
        visible_height: usize,
        content_width: usize,
        prefix_width: usize,
        timestamp_width: usize,
        block: Block,
        current_match: Option<&SearchMatch>,
    ) {
        let total = buffer.len();
        
        // Update last visible height for key handler scroll bounds
        self.last_visible_height = visible_height;
        
        // Calculate scroll position with proper bounds
        let max_scroll = total.saturating_sub(visible_height);
        let scroll = if self.config.auto_scroll && total > 0 {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        self.scroll = scroll;

        // Render block with title
        let filter_info = if buffer.filter_pattern().is_some() {
            let total_unfiltered = buffer.total_len();
            format!(" | filter: {}/{}", total, total_unfiltered)
        } else {
            String::new()
        };
        let block = block.title(format!(
            " Traffic [{}/{}]{} ",
            scroll + 1,
            total.max(1),
            filter_info,
        ));
        block.render(area, buf);

        // Render chunks
        let mut y = inner.y;
        for (visible_idx, chunk) in buffer.chunks().enumerate().skip(scroll).take(visible_height) {
            if y >= inner.y + inner.height {
                break;
            }

            // Get matches for this chunk
            let matches: Vec<_> = buffer.matches_in_chunk(visible_idx).cloned().collect();
            let line = self.format_chunk_line_highlighted(
                &chunk, timestamp_width, content_width, prefix_width, true,
                &matches, current_match,
            );
            Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
            y += 1;
        }

        // Help text
        if total == 0 {
            let help = "No data yet. Waiting for traffic...";
            Paragraph::new(help)
                .style(Theme::muted())
                .render(Rect::new(inner.x + 1, inner.y, inner.width - 2, 1), buf);
        }

        // Scrollbar
        if total > visible_height {
            self.render_scrollbar(area, buf, max_scroll + 1, scroll);
        }
    }

    fn draw_traffic_wrapped(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _handle: &SessionHandle,
        buffer: &serial_core::buffer::DataBuffer,
        inner: Rect,
        visible_height: usize,
        content_width: usize,
        prefix_width: usize,
        timestamp_width: usize,
        block: Block,
        current_match: Option<&SearchMatch>,
    ) {
        // Calculate total display lines (each chunk may span multiple lines when wrapped)
        let chunks: Vec<_> = buffer.chunks().collect();
        let mut display_lines: Vec<(usize, usize)> = Vec::new(); // (chunk_index, line_within_chunk)
        
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let content_len = chunk.encoded.len();
            let num_lines = if content_width > 0 {
                (content_len + content_width - 1) / content_width.max(1)
            } else {
                1
            }.max(1); // At least one line per chunk
            
            for line_idx in 0..num_lines {
                display_lines.push((chunk_idx, line_idx));
            }
        }

        let total_display_lines = display_lines.len();
        
        // Update last visible height for key handler scroll bounds
        self.last_visible_height = visible_height;
        
        // Calculate scroll position with proper bounds (in display lines)
        let max_scroll = total_display_lines.saturating_sub(visible_height);
        let scroll = if self.config.auto_scroll && total_display_lines > 0 {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        self.scroll = scroll;

        // Render block with title showing display line position
        let filter_info = if buffer.filter_pattern().is_some() {
            let total_unfiltered = buffer.total_len();
            let filtered_chunks = buffer.len();
            format!(" | filter: {}/{}", filtered_chunks, total_unfiltered)
        } else {
            String::new()
        };
        let block = block.title(format!(
            " Traffic [{}/{}]{} ",
            scroll + 1,
            total_display_lines.max(1),
            filter_info,
        ));
        block.render(area, buf);

        // Render display lines starting from scroll position
        let mut y = inner.y;
        for &(chunk_idx, line_within_chunk) in display_lines.iter().skip(scroll).take(visible_height) {
            if y >= inner.y + inner.height {
                break;
            }

            let chunk = &chunks[chunk_idx];
            let content = &chunk.encoded;
            
            // Get matches for this chunk
            let matches: Vec<_> = buffer.matches_in_chunk(chunk_idx).cloned().collect();
            
            // Calculate which part of content to show
            let start = line_within_chunk * content_width;
            let end = (start + content_width).min(content.len());

            // Only show prefix on first line of chunk
            if line_within_chunk == 0 {
                let line = self.format_chunk_line_with_content_highlighted(
                    chunk, timestamp_width, content, start, end, &matches, current_match
                );
                Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
            } else {
                // Continuation line - indent to align with content
                let indent = " ".repeat(prefix_width);
                let content_spans = self.create_highlighted_spans(content, start, end, &matches, current_match);
                let mut spans = vec![Span::raw(indent)];
                spans.extend(content_spans);
                let line = Line::from(spans);
                Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
            }
            
            y += 1;
        }

        // Help text
        if chunks.is_empty() {
            let help = "No data yet. Waiting for traffic...";
            Paragraph::new(help)
                .style(Theme::muted())
                .render(Rect::new(inner.x + 1, inner.y, inner.width - 2, 1), buf);
        }

        // Scrollbar
        if total_display_lines > visible_height {
            self.render_scrollbar(area, buf, max_scroll + 1, scroll);
        }
    }

    fn format_chunk_line_highlighted(
        &self,
        chunk: &serial_core::buffer::ChunkView,
        timestamp_width: usize,
        content_width: usize,
        _prefix_width: usize,
        truncate: bool,
        matches: &[SearchMatch],
        current_match: Option<&SearchMatch>,
    ) -> Line<'static> {
        let (dir_char, dir_style) = match chunk.direction {
            DataDirection::Tx => ("TX", Theme::tx()),
            DataDirection::Rx => ("RX", Theme::rx()),
        };

        let mut spans = vec![Span::styled(format!("{} ", dir_char), dir_style)];

        if self.config.show_timestamps {
            let session_start = self.session_start.unwrap_or(chunk.timestamp);
            let formatted = self.config.timestamp_format().format(chunk.timestamp, session_start);
            let padded = format!("{:>width$} ", formatted, width = timestamp_width);
            spans.push(Span::styled(padded, Theme::muted()));
        }

        let content = &chunk.encoded;
        
        // Calculate display bounds
        let byte_end = if truncate && content.len() > content_width {
            content_width.saturating_sub(3)
        } else {
            content.len()
        };
        
        // Add highlighted content spans
        let content_spans = self.create_highlighted_spans(content, 0, byte_end, matches, current_match);
        spans.extend(content_spans);
        
        // Add ellipsis if truncated
        if truncate && content.len() > content_width {
            spans.push(Span::raw("...".to_string()));
        }

        Line::from(spans)
    }

    fn format_chunk_line_with_content_highlighted(
        &self,
        chunk: &serial_core::buffer::ChunkView,
        timestamp_width: usize,
        full_content: &str,
        byte_start: usize,
        byte_end: usize,
        matches: &[SearchMatch],
        current_match: Option<&SearchMatch>,
    ) -> Line<'static> {
        let (dir_char, dir_style) = match chunk.direction {
            DataDirection::Tx => ("TX", Theme::tx()),
            DataDirection::Rx => ("RX", Theme::rx()),
        };

        let mut spans = vec![Span::styled(format!("{} ", dir_char), dir_style)];

        if self.config.show_timestamps {
            let session_start = self.session_start.unwrap_or(chunk.timestamp);
            let formatted = self.config.timestamp_format().format(chunk.timestamp, session_start);
            let padded = format!("{:>width$} ", formatted, width = timestamp_width);
            spans.push(Span::styled(padded, Theme::muted()));
        }

        let content_spans = self.create_highlighted_spans(full_content, byte_start, byte_end, matches, current_match);
        spans.extend(content_spans);

        Line::from(spans)
    }

    /// Create spans for content with search match highlighting.
    ///
    /// Takes the full content, byte range to display (for wrapped lines), and matches
    /// that fall within this chunk. Returns spans with appropriate highlighting.
    fn create_highlighted_spans(
        &self,
        content: &str,
        byte_start: usize,
        byte_end: usize,
        matches: &[SearchMatch],
        current_match: Option<&SearchMatch>,
    ) -> Vec<Span<'static>> {
        // Early return if no matches or empty content
        if matches.is_empty() || byte_start >= byte_end {
            let slice = if byte_start < content.len() {
                &content[byte_start..byte_end.min(content.len())]
            } else {
                ""
            };
            return vec![Span::raw(slice.to_string())];
        }

        let mut spans = Vec::new();
        let mut pos = byte_start;

        // Filter and sort matches that overlap with our display range
        let mut relevant_matches: Vec<_> = matches
            .iter()
            .filter(|m| m.byte_start < byte_end && m.byte_end > byte_start)
            .collect();
        relevant_matches.sort_by_key(|m| m.byte_start);

        for m in relevant_matches {
            // Clamp match bounds to our display range
            let match_start = m.byte_start.max(byte_start);
            let match_end = m.byte_end.min(byte_end);

            // Add text before this match
            if pos < match_start && pos < content.len() {
                let text = &content[pos..match_start.min(content.len())];
                spans.push(Span::raw(text.to_string()));
            }

            // Add the highlighted match
            if match_start < content.len() {
                let match_text = &content[match_start..match_end.min(content.len())];
                let is_current = current_match.is_some_and(|c| c == m);
                let style = if is_current {
                    // Current match gets inverted/more prominent styling
                    Theme::search_match_current()
                } else {
                    Theme::search_match()
                };
                spans.push(Span::styled(match_text.to_string(), style));
            }

            pos = match_end;
        }

        // Add remaining text after last match
        if pos < byte_end && pos < content.len() {
            let text = &content[pos..byte_end.min(content.len())];
            spans.push(Span::raw(text.to_string()));
        }

        if spans.is_empty() {
            let slice = if byte_start < content.len() {
                &content[byte_start..byte_end.min(content.len())]
            } else {
                ""
            };
            spans.push(Span::raw(slice.to_string()));
        }

        spans
    }

    fn render_scrollbar(&self, area: Rect, buf: &mut Buffer, content_length: usize, position: usize) {
        let mut scrollbar_state = ScrollbarState::new(content_length).position(position);
        let scrollbar_area = Rect::new(
            area.x + area.width - 1,
            area.y + 1,
            1,
            area.height.saturating_sub(2),
        );
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .render(scrollbar_area, buf, &mut scrollbar_state);
    }

    fn draw_input_bar(&self, area: Rect, buf: &mut Buffer, handle: &SessionHandle) {
        let (title, input_state) = if self.search_focused {
            // Get search status from buffer
            let buffer = handle.buffer();
            let status = buffer.search_status();
            let title = if status.is_empty() {
                "Search".to_string()
            } else {
                format!("Search [{}]", status)
            };
            (title, &self.search_input)
        } else if self.filter_focused {
            // Get filter status from buffer
            let buffer = handle.buffer();
            let total_chunks = buffer.total_len();
            let visible_chunks = buffer.len();
            let title = if total_chunks == visible_chunks {
                "Filter".to_string()
            } else {
                format!("Filter [{}/{}]", visible_chunks, total_chunks)
            };
            (title, &self.filter_input)
        } else {
            ("Send".to_string(), &self.send_input)
        };

        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let mut state = input_state.clone();
        TextInput::new(&mut state)
            .block(block)
            .focused(true)
            .render(area, buf);
    }

    fn draw_config(
        &self,
        area: Rect,
        buf: &mut Buffer,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(5)])
            .split(area);

        // Connection info (read-only)
        let conn_block = Block::default()
            .title(" Connection ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let conn_inner = conn_block.inner(chunks[0]);
        conn_block.render(chunks[0], buf);

        let conn_lines = vec![
            Line::from(vec![
                Span::styled("Baud:  ", Theme::muted()),
                Span::raw(serial_config.baud_rate.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Data:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.data_bits)),
            ]),
            Line::from(vec![
                Span::styled("Parity:", Theme::muted()),
                Span::raw(format!(" {:?}", serial_config.parity)),
            ]),
            Line::from(vec![
                Span::styled("Stop:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.stop_bits)),
            ]),
            Line::from(vec![
                Span::styled("Flow:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.flow_control)),
            ]),
        ];

        for (i, line) in conn_lines.into_iter().enumerate() {
            if i >= conn_inner.height as usize {
                break;
            }
            Paragraph::new(line).render(
                Rect::new(conn_inner.x, conn_inner.y + i as u16, conn_inner.width, 1),
                buf,
            );
        }

        // Traffic config
        let config_block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Config {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        ConfigPanel::new(TRAFFIC_CONFIG_SECTIONS, &self.config, &self.config_nav)
            .block(config_block)
            .focused(focus == Focus::Config)
            .render(chunks[1], buf);
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        focus: Focus,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        // Handle input modes
        if self.search_focused {
            return self.handle_search_key(key, handle);
        }
        if self.filter_focused {
            return self.handle_filter_key(key, handle);
        }
        if self.send_focused {
            return self.handle_send_key(key);
        }

        match focus {
            Focus::Main => self.handle_main_key(key, handle),
            Focus::Config => self.handle_config_key(key, handle),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        // Half-page scroll amount based on actual visible height
        let half_page = self.last_visible_height / 2;

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        let buffer = handle.buffer();
        
        // Calculate total scrollable items based on wrap mode
        let total = if self.config.wrap_text {
            // Count display lines (wrapped)
            let content_width = self.last_content_width.max(1);
            buffer.chunks().map(|chunk| {
                let content_len = chunk.encoded.len();
                ((content_len + content_width - 1) / content_width).max(1)
            }).sum()
        } else {
            // Count chunks
            buffer.len()
        };
        
        // Use the last known visible height for accurate scroll bounds
        let max_scroll = total.saturating_sub(self.last_visible_height);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.scroll = self.scroll.saturating_add(1).min(max_scroll);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.scroll = self.scroll.saturating_sub(1);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down
                self.scroll = self.scroll.saturating_add(half_page).min(max_scroll);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up
                self.scroll = self.scroll.saturating_sub(half_page);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('g') => {
                self.scroll = 0;
                self.config.auto_scroll = false;
            }
            KeyCode::Char('G') => {
                self.scroll = max_scroll;
                self.config.auto_scroll = true;
            }
            KeyCode::Char('/') => {
                self.search_focused = true;
            }
            KeyCode::Char('f') => {
                self.filter_focused = true;
            }
            KeyCode::Char('s') => {
                self.send_focused = true;
            }
            KeyCode::Char('n') => {
                // Next search match - need to calculate scroll position while we have buffer access
                let scroll_pos = if self.config.wrap_text {
                    let content_width = self.last_content_width.max(1);
                    // Pre-calculate display line offsets for each chunk
                    let display_offsets: Vec<usize> = buffer.chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_len = chunk.encoded.len();
                            let num_lines = ((content_len + content_width - 1) / content_width).max(1);
                            *acc += num_lines;
                            Some(offset)
                        })
                        .collect();
                    drop(buffer);
                    handle.buffer_mut().goto_next_match().map(|idx| {
                        display_offsets.get(idx).copied().unwrap_or(0)
                    })
                } else {
                    drop(buffer);
                    handle.buffer_mut().goto_next_match()
                };
                
                if let Some(pos) = scroll_pos {
                    self.scroll = pos;
                    self.config.auto_scroll = false;
                }
            }
            KeyCode::Char('N') => {
                // Previous search match - need to calculate scroll position while we have buffer access
                let scroll_pos = if self.config.wrap_text {
                    let content_width = self.last_content_width.max(1);
                    // Pre-calculate display line offsets for each chunk
                    let display_offsets: Vec<usize> = buffer.chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_len = chunk.encoded.len();
                            let num_lines = ((content_len + content_width - 1) / content_width).max(1);
                            *acc += num_lines;
                            Some(offset)
                        })
                        .collect();
                    drop(buffer);
                    handle.buffer_mut().goto_prev_match().map(|idx| {
                        display_offsets.get(idx).copied().unwrap_or(0)
                    })
                } else {
                    drop(buffer);
                    handle.buffer_mut().goto_prev_match()
                };
                
                if let Some(pos) = scroll_pos {
                    self.scroll = pos;
                    self.config.auto_scroll = false;
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        let result = handle_config_key(
            key,
            &mut self.config_nav,
            TRAFFIC_CONFIG_SECTIONS,
            &mut self.config,
        );

        match result {
            ConfigKeyResult::Changed => {
                self.sync_config_to_buffer(handle);
                Some(TrafficAction::RequestClear)
            }
            ConfigKeyResult::DropdownClosed => Some(TrafficAction::RequestClear),
            _ => None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                // Confirm search and exit search mode
                // Pattern is already set via incremental search
                self.search_focused = false;
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc => {
                self.search_focused = false;
                self.search_input.clear();
                handle.buffer_mut().clear_search();
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                // Handle the key input first
                self.search_input.handle_key(key);
                
                // Then update search pattern incrementally
                let pattern = &self.search_input.content;
                if !pattern.is_empty() {
                    let mode = if self.config.pattern_mode_index == 1 {
                        PatternMode::Regex
                    } else {
                        PatternMode::Normal
                    };
                    // Ignore errors during incremental search (e.g., incomplete regex)
                    let _ = handle.buffer_mut().set_search_pattern(pattern, mode);
                } else {
                    handle.buffer_mut().clear_search();
                }
            }
        }
        None
    }

    fn handle_filter_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                // Confirm filter and exit filter mode
                // Pattern is already set via incremental filtering
                self.filter_focused = false;
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc => {
                self.filter_focused = false;
                self.filter_input.clear();
                handle.buffer_mut().clear_filter();
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                // Handle the key input first
                self.filter_input.handle_key(key);
                
                // Then update filter pattern incrementally
                let pattern = &self.filter_input.content;
                if !pattern.is_empty() {
                    let mode = if self.config.pattern_mode_index == 1 {
                        PatternMode::Regex
                    } else {
                        PatternMode::Normal
                    };
                    // Ignore errors during incremental filter (e.g., incomplete regex)
                    let _ = handle.buffer_mut().set_filter_pattern(pattern, mode);
                } else {
                    handle.buffer_mut().clear_filter();
                }
            }
        }
        None
    }

    fn handle_send_key(&mut self, key: KeyEvent) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                let data = self.send_input.take();
                if !data.is_empty() {
                    self.send_focused = false;
                    // Add newline for convenience
                    let mut bytes = data.into_bytes();
                    bytes.push(b'\n');
                    return Some(TrafficAction::Send(bytes));
                }
            }
            KeyCode::Esc => {
                self.send_focused = false;
                self.send_input.clear();
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                self.send_input.handle_key(key);
            }
        }
        None
    }
}

impl Default for TrafficView {
    fn default() -> Self {
        Self::new()
    }
}
