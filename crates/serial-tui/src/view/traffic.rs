//! Traffic view: main data display with search and send functionality.

use std::time::SystemTime;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
        config::{ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible, always_enabled},
        encoding::{ENCODING_DISPLAY_NAMES, ENCODING_VARIANTS},
    },
};

use crate::{
    app::{Focus, TrafficAction},
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfigKeyResult, ConfigPanel,
        ConnectionPanel, TextInput, handle_config_key,
        text_input::{TextInputState, find_path_completions},
    },
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
    /// Directory path input state.
    pub dir_path_input: TextInputState,
    /// Whether directory path input is focused.
    pub dir_path_focused: bool,
    /// Directory path completion state.
    pub dir_path_completion: CompletionState,
    /// Traffic config.
    pub config: TrafficConfig,
    /// Config panel navigation.
    pub config_nav: ConfigNav,
    /// Session start time for relative timestamps.
    pub session_start: Option<SystemTime>,
    /// Last known visible height (for scroll bounds calculation).
    last_visible_height: usize,
    /// Last known content width (for wrap calculation in key handler).
    last_content_width: usize,
    /// Whether the view is currently scrolled to the bottom.
    at_bottom: bool,
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
    pub lock_to_bottom: bool,
    pub search_mode_index: usize,
    pub filter_mode_index: usize,
    pub wrap_text: bool,
    // File saving settings (can be toggled while connected)
    pub file_save_enabled: bool,
    pub file_save_format_index: usize,
    pub file_save_encoding_index: usize,
    pub file_save_directory: String,
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
            lock_to_bottom: false,
            search_mode_index: 0, // Normal
            filter_mode_index: 0, // Normal
            wrap_text: true, // Wrap by default
            // File saving defaults
            file_save_enabled: false,
            file_save_format_index: 1, // Encoded
            file_save_encoding_index: 1, // ASCII
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
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

/// Slice a string by display width positions, returning byte indices.
///
/// Given a start and end display column, returns the byte range that covers
/// those columns. Handles multi-byte UTF-8 characters and wide characters correctly.
///
/// Returns `(byte_start, byte_end)` where the slice `&s[byte_start..byte_end]`
/// contains the characters that fall within the display range.
fn slice_by_display_width(s: &str, display_start: usize, display_end: usize) -> (usize, usize) {
    let mut current_width = 0;
    let mut byte_start = None;
    let mut byte_end = s.len();

    for (byte_idx, ch) in s.char_indices() {
        let char_width = ch.width().unwrap_or(0);

        // Found the start position
        if byte_start.is_none() && current_width + char_width > display_start {
            byte_start = Some(byte_idx);
        }

        // Found the end position
        if current_width >= display_end {
            byte_end = byte_idx;
            break;
        }

        current_width += char_width;
    }

    (byte_start.unwrap_or(s.len()), byte_end)
}

// Config panel definitions
const ENCODING_OPTIONS: &[&str] = ENCODING_DISPLAY_NAMES;
const TIMESTAMP_FORMAT_OPTIONS: &[&str] = &["Relative", "HH:MM:SS.mmm", "HH:MM:SS"];

// File saving options
const FILE_SAVE_FORMAT_OPTIONS: &[&str] = &["Raw Binary", "Encoded Text"];
const FILE_SAVE_ENCODING_OPTIONS: &[&str] = &["UTF-8", "ASCII", "Hex", "Binary"];

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
                enabled: always_enabled,
                parent_id: None,
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
                enabled: always_enabled,
                parent_id: None,
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
                visible: always_visible,
                enabled: |c| c.show_timestamps,
                parent_id: Some("show_timestamps"),
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
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "lock_to_bottom",
                label: "Lock to Bottom",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.lock_to_bottom),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.lock_to_bottom = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
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
                enabled: always_enabled,
                parent_id: None,
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
                enabled: always_enabled,
                parent_id: None,
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
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("File Saving"),
        fields: &[
            FieldDef {
                id: "file_save_enabled",
                label: "Save to File",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.file_save_enabled),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.file_save_enabled = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_format",
                label: "Format",
                kind: FieldKind::Select {
                    options: FILE_SAVE_FORMAT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.file_save_format_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.file_save_format_index = i;
                    }
                },
                visible: always_visible,
                // Only enabled when file saving is NOT active (can't change format while saving)
                enabled: |c| !c.file_save_enabled,
                parent_id: Some("file_save_enabled"),
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_encoding",
                label: "Encoding",
                kind: FieldKind::Select {
                    options: FILE_SAVE_ENCODING_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.file_save_encoding_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.file_save_encoding_index = i;
                    }
                },
                // Only visible when format is Encoded (index 1)
                visible: |c| c.file_save_format_index == 1,
                // Only enabled when file saving is NOT active
                enabled: |c| !c.file_save_enabled,
                parent_id: Some("file_save_format"),
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_directory",
                label: "Directory",
                kind: FieldKind::TextInput {
                    placeholder: "Enter directory path...",
                },
                get: |c| FieldValue::string(c.file_save_directory.clone()),
                set: |c, v| {
                    if let FieldValue::String(s) = v {
                        c.file_save_directory = s.into_owned();
                    }
                },
                visible: always_visible,
                // Only enabled when file saving is NOT active
                enabled: |c| !c.file_save_enabled,
                parent_id: Some("file_save_enabled"),
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
            dir_path_input: TextInputState::new().with_placeholder("Enter directory path..."),
            dir_path_focused: false,
            dir_path_completion: CompletionState::default(),
            config: TrafficConfig::default(),
            config_nav: ConfigNav::new(),
            session_start: None,
            last_visible_height: 20, // Conservative default
            last_content_width: 80,  // Conservative default
            at_bottom: true,         // Start at bottom
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused || self.filter_focused || self.send_focused || self.dir_path_focused
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
        // Main area layout: traffic + optional search/filter/send/dir bar
        let show_input_bar = self.search_focused || self.filter_focused || self.send_focused || self.dir_path_focused;
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
            
            // Render directory path completion popup (above the input bar)
            if self.dir_path_focused && self.dir_path_completion.visible {
                let input_inner = Block::default()
                    .borders(Borders::ALL)
                    .inner(main_chunks[1]);
                CompletionPopup::new(
                    &self.dir_path_completion,
                    input_inner.y,
                    input_inner.x,
                )
                .render(main_area, buf);
            }
        }

        // Draw config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, handle, serial_config, focus);
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

        // Clear the entire inner area BEFORE rendering any content.
        // This is critical because:
        // 1. Timestamp width can change as time progresses (+9.999s -> +10.000s), causing
        //    content_width to change and all wrap positions to shift
        // 2. Different amounts of content may be rendered each frame
        // 3. Paragraph::render() only writes actual content, not trailing spaces
        // Without this, old content bleeds through when lines get shorter or positions shift.
        for y in inner.y..inner.y + inner.height {
            buf.set_string(inner.x, y, " ".repeat(inner.width as usize), ratatui::style::Style::default());
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

    #[allow(clippy::too_many_arguments)]
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
        // Add 10% bottom padding so user can clearly see when they've hit the bottom
        // Only apply padding when there's actually scrollable content
        let max_scroll_content = total.saturating_sub(visible_height);
        let bottom_padding = if max_scroll_content > 0 { visible_height / 10 } else { 0 };
        let max_scroll = max_scroll_content + bottom_padding;
        let should_auto_scroll = self.config.lock_to_bottom || (self.config.auto_scroll && self.at_bottom);
        let scroll = if should_auto_scroll && total > 0 {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        self.scroll = scroll;
        // Update at_bottom based on final scroll position (using padded max)
        self.at_bottom = total == 0 || scroll >= max_scroll;

        // Render block with title
        let filter_info = if buffer.filter_pattern().is_some() {
            let total_unfiltered = buffer.total_len();
            format!(" | filter: {}/{}", total, total_unfiltered)
        } else {
            String::new()
        };
        let lock_indicator = if self.config.lock_to_bottom {
            " [LOCKED]"
        } else {
            ""
        };
        let save_indicator = if buffer.is_saving() {
            " [SAVING]"
        } else {
            ""
        };
        // Cap displayed scroll position at content max (don't show padding in title)
        let display_scroll = scroll.min(max_scroll_content);
        let block = block.title(format!(
            " Traffic [{}/{}]{}{}{} ",
            display_scroll + 1,
            total.max(1),
            filter_info,
            lock_indicator,
            save_indicator,
        ));
        block.render(area, buf);

        // Render chunks
        let mut y = inner.y;
        for (visible_idx, chunk) in buffer.chunks().enumerate().skip(scroll).take(visible_height) {
            if y >= inner.y + inner.height {
                break;
            }

            // Get matches for this chunk
            let matches = buffer.matches_in_chunk(visible_idx);
            let line = self.format_chunk_line_highlighted(
                &chunk, timestamp_width, content_width, prefix_width, true,
                matches, current_match,
            );
            let line_area = Rect::new(inner.x, y, inner.width, 1);
            Paragraph::new(line).render(line_area, buf);
            y += 1;
        }

        // Help text
        if total == 0 {
            let help = "No data yet. Waiting for traffic...";
            Paragraph::new(help)
                .style(Theme::muted())
                .render(Rect::new(inner.x + 1, inner.y, inner.width - 2, 1), buf);
        }

        // Scrollbar (use content max, not padded max, so it shows "at bottom" when content ends)
        if total > visible_height {
            self.render_scrollbar(area, buf, max_scroll_content + 1, display_scroll);
        }
    }

    #[allow(clippy::too_many_arguments)]
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
            // Use display width, not byte length, for line wrapping calculations
            let content_display_width = chunk.encoded.width();
            let num_lines = if content_width > 0 {
                (content_display_width + content_width - 1) / content_width.max(1)
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
        // Add 10% bottom padding so user can clearly see when they've hit the bottom
        // Only apply padding when there's actually scrollable content
        let max_scroll_content = total_display_lines.saturating_sub(visible_height);
        let bottom_padding = if max_scroll_content > 0 { visible_height / 10 } else { 0 };
        let max_scroll = max_scroll_content + bottom_padding;
        let should_auto_scroll = self.config.lock_to_bottom || (self.config.auto_scroll && self.at_bottom);
        let scroll = if should_auto_scroll && total_display_lines > 0 {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        self.scroll = scroll;
        // Update at_bottom based on final scroll position (using padded max)
        self.at_bottom = total_display_lines == 0 || scroll >= max_scroll;

        // Render block with title showing display line position
        // Cap displayed scroll position at content max (don't show padding in title)
        let display_scroll = scroll.min(max_scroll_content);
        let filter_info = if buffer.filter_pattern().is_some() {
            let total_unfiltered = buffer.total_len();
            let filtered_chunks = buffer.len();
            format!(" | filter: {}/{}", filtered_chunks, total_unfiltered)
        } else {
            String::new()
        };
        let lock_indicator = if self.config.lock_to_bottom {
            " [LOCKED]"
        } else {
            ""
        };
        let save_indicator = if buffer.is_saving() {
            " [SAVING]"
        } else {
            ""
        };
        let block = block.title(format!(
            " Traffic [{}/{}]{}{}{} ",
            display_scroll + 1,
            total_display_lines.max(1),
            filter_info,
            lock_indicator,
            save_indicator,
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
            let matches = buffer.matches_in_chunk(chunk_idx);
            
            // Calculate which part of content to show (using display width, not bytes)
            let display_start = line_within_chunk * content_width;
            let display_end = display_start + content_width;
            let (start, end) = slice_by_display_width(content, display_start, display_end);

            let line_area = Rect::new(inner.x, y, inner.width, 1);

            // Only show prefix on first line of chunk
            if line_within_chunk == 0 {
                let line = self.format_chunk_line_with_content_highlighted(
                    chunk, timestamp_width, content, start, end, matches, current_match
                );
                Paragraph::new(line).render(line_area, buf);
            } else {
                // Continuation line - indent to align with content
                let indent = " ".repeat(prefix_width);
                let content_spans = self.create_highlighted_spans(content, start, end, matches, current_match);
                let mut spans = vec![Span::raw(indent)];
                spans.extend(content_spans);
                let line = Line::from(spans);
                Paragraph::new(line).render(line_area, buf);
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

        // Scrollbar (use content max, not padded max, so it shows "at bottom" when content ends)
        if total_display_lines > visible_height {
            self.render_scrollbar(area, buf, max_scroll_content + 1, display_scroll);
        }
    }

    #[allow(clippy::too_many_arguments)]
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
        let content_display_width = content.width();
        
        // Calculate display bounds - use display width for comparison, then convert to bytes
        let byte_end = if truncate && content_display_width > content_width {
            // Truncate to content_width - 3 (for "...") display columns
            let (_, end) = slice_by_display_width(content, 0, content_width.saturating_sub(3));
            end
        } else {
            content.len()
        };
        
        // Add highlighted content spans
        let content_spans = self.create_highlighted_spans(content, 0, byte_end, matches, current_match);
        spans.extend(content_spans);
        
        // Add ellipsis if truncated
        if truncate && content_display_width > content_width {
            spans.push(Span::raw("...".to_string()));
        }

        Line::from(spans)
    }

    #[allow(clippy::too_many_arguments)]
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
        } else if self.dir_path_focused {
            ("Save Directory".to_string(), &self.dir_path_input)
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
        handle: &SessionHandle,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(5)])
            .split(area);

        // Connection info with statistics
        let conn_block = Block::default()
            .title(" Connection ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        ConnectionPanel::new(handle.port_name(), serial_config, handle.statistics())
            .block(conn_block)
            .render(chunks[0], buf);

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
        if self.dir_path_focused {
            return self.handle_dir_path_key(key);
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
            // Count display lines (wrapped) using display width
            let content_width = self.last_content_width.max(1);
            buffer.chunks().map(|chunk| {
                let content_display_width = chunk.encoded.width();
                content_display_width.div_ceil(content_width).max(1)
            }).sum()
        } else {
            // Count chunks
            buffer.len()
        };
        
        // Use the last known visible height for accurate scroll bounds
        // Add 10% bottom padding to clearly show when at the very bottom
        // Only apply padding when there's actually scrollable content
        let max_scroll_content = total.saturating_sub(self.last_visible_height);
        let bottom_padding = if max_scroll_content > 0 { self.last_visible_height / 10 } else { 0 };
        let max_scroll = max_scroll_content + bottom_padding;

        match key.code {
            // Toggle lock_to_bottom with Ctrl+b
            KeyCode::Char('b') if has_ctrl => {
                self.config.lock_to_bottom = !self.config.lock_to_bottom;
                // When enabling lock, also enable auto_scroll and go to bottom
                if self.config.lock_to_bottom {
                    self.config.auto_scroll = true;
                    self.scroll = max_scroll;
                    self.at_bottom = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                // Lock mode: ignore scroll down (we're already at bottom)
                if !self.config.lock_to_bottom {
                    self.scroll = self.scroll.saturating_add(1).min(max_scroll);
                    self.at_bottom = self.scroll >= max_scroll;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                // Lock mode: ignore scroll up
                if !self.config.lock_to_bottom {
                    self.scroll = self.scroll.saturating_sub(1);
                    self.at_bottom = self.scroll >= max_scroll;
                }
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down - lock mode ignores this
                if !self.config.lock_to_bottom {
                    self.scroll = self.scroll.saturating_add(half_page).min(max_scroll);
                    self.at_bottom = self.scroll >= max_scroll;
                }
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up - lock mode ignores this
                if !self.config.lock_to_bottom {
                    self.scroll = self.scroll.saturating_sub(half_page);
                    self.at_bottom = self.scroll >= max_scroll;
                }
            }
            KeyCode::Char('g') => {
                // Go to top - lock mode ignores this
                if !self.config.lock_to_bottom {
                    self.scroll = 0;
                    self.at_bottom = false;
                }
            }
            KeyCode::Char('G') => {
                // Go to bottom - always works, re-enables auto_scroll
                self.scroll = max_scroll;
                self.config.auto_scroll = true;
                self.at_bottom = true;
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
                // Lock mode still allows navigating to search matches
                let scroll_pos = if self.config.wrap_text {
                    let content_width = self.last_content_width.max(1);
                    // Pre-calculate display line offsets for each chunk using display width
                    let display_offsets: Vec<usize> = buffer.chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_display_width = chunk.encoded.width();
                            let num_lines = content_display_width.div_ceil(content_width).max(1);
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
                    // Only navigate if not in lock mode, or if match is visible from bottom
                    if !self.config.lock_to_bottom {
                        // Position match with 20% of visible height above it (like vim scrolloff)
                        let offset = self.last_visible_height / 5;
                        self.scroll = pos.saturating_sub(offset);
                        self.at_bottom = self.scroll >= max_scroll;
                    }
                }
            }
            KeyCode::Char('N') => {
                // Previous search match - need to calculate scroll position while we have buffer access
                let scroll_pos = if self.config.wrap_text {
                    let content_width = self.last_content_width.max(1);
                    // Pre-calculate display line offsets for each chunk using display width
                    let display_offsets: Vec<usize> = buffer.chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_display_width = chunk.encoded.width();
                            let num_lines = content_display_width.div_ceil(content_width).max(1);
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
                    // Only navigate if not in lock mode
                    if !self.config.lock_to_bottom {
                        // Position match with 20% of visible height above it (like vim scrolloff)
                        let offset = self.last_visible_height / 5;
                        self.scroll = pos.saturating_sub(offset);
                        self.at_bottom = self.scroll >= max_scroll;
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        // Keys that can trigger a toggle: Enter, Space, h, l, Left, Right
        let is_toggle_key = matches!(
            key.code,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('h') | KeyCode::Char('l') 
            | KeyCode::Left | KeyCode::Right
        );
        
        // Track if we're about to toggle file_save_enabled
        let mut toggling_file_save = false;
        let file_save_was_enabled = self.config.file_save_enabled;
        
        if is_toggle_key && !self.config_nav.edit_mode.is_dropdown()
            && let Some(field) = self.config_nav.current_field(TRAFFIC_CONFIG_SECTIONS, &self.config)
        {
            // Handle text input field (directory)
            if field.kind.is_text_input() && field.id == "file_save_directory"
                && matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
            {
                self.dir_path_input.set_content(&self.config.file_save_directory);
                self.dir_path_focused = true;
                return Some(TrafficAction::RequestClear);
            }
            
            // Intercept file_save_enabled toggle
            if field.id == "file_save_enabled" && matches!(field.kind, FieldKind::Toggle) {
                if !self.config.file_save_enabled {
                    // About to enable - validate directory first
                    if let Some(error) = self.validate_save_directory() {
                        return Some(TrafficAction::Toast(crate::widget::Toast::error(error)));
                    }
                }
                toggling_file_save = true;
            }
        }

        let result = handle_config_key(
            key,
            &mut self.config_nav,
            TRAFFIC_CONFIG_SECTIONS,
            &mut self.config,
        );

        match result {
            ConfigKeyResult::Changed => {
                self.sync_config_to_buffer(handle);
                
                // Check if file_save_enabled was toggled
                if toggling_file_save && self.config.file_save_enabled != file_save_was_enabled {
                    if self.config.file_save_enabled {
                        // Just enabled - start file saving
                        return Some(TrafficAction::StartFileSaving);
                    } else {
                        // Just disabled - stop file saving
                        return Some(TrafficAction::StopFileSaving);
                    }
                }
                
                Some(TrafficAction::RequestClear)
            }
            ConfigKeyResult::EditClosed => Some(TrafficAction::RequestClear),
            _ => None,
        }
    }
    
    /// Validate the save directory. Returns Some(error_message) if invalid, None if valid.
    fn validate_save_directory(&self) -> Option<String> {
        let path = std::path::Path::new(&self.config.file_save_directory);
        
        // Check if directory exists
        if !path.exists() {
            // Try to create it
            if let Err(e) = std::fs::create_dir_all(path) {
                return Some(format!("Cannot create directory: {}", e));
            }
        }
        
        // Check if it's actually a directory
        if !path.is_dir() {
            return Some("Path is not a directory".to_string());
        }
        
        // Check if writable by trying to create a temp file
        let test_file = path.join(".serial-monitor-test");
        match std::fs::File::create(&test_file) {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_file);
            }
            Err(e) => {
                return Some(format!("Directory not writable: {}", e));
            }
        }
        
        None // Valid
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
                let pattern = self.search_input.content();
                if !pattern.is_empty() {
                    let mode = if self.config.search_mode_index == 1 {
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
                let pattern = self.filter_input.content();
                if !pattern.is_empty() {
                    let mode = if self.config.filter_mode_index == 1 {
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

    fn handle_dir_path_key(&mut self, key: KeyEvent) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                // If completion is visible, apply the selected completion
                if self.dir_path_completion.visible {
                    self.apply_dir_path_completion();
                    self.dir_path_completion.hide();
                    return None;
                }
                // Apply directory path and exit input mode
                self.config.file_save_directory = self.dir_path_input.content().to_string();
                self.dir_path_focused = false;
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc => {
                if self.dir_path_completion.visible {
                    self.dir_path_completion.hide();
                } else {
                    // Cancel and exit without saving
                    self.dir_path_focused = false;
                    self.dir_path_input.clear();
                    // Layout changed - request clear to avoid artifacts
                    return Some(TrafficAction::RequestClear);
                }
            }
            KeyCode::Tab => {
                if !self.dir_path_completion.visible {
                    self.update_dir_path_completions();
                } else {
                    self.dir_path_completion.next();
                }
                self.apply_dir_path_completion();
            }
            KeyCode::BackTab => {
                if self.dir_path_completion.visible {
                    self.dir_path_completion.prev();
                    self.apply_dir_path_completion();
                }
            }
            _ => {
                self.dir_path_input.handle_key(key);
                self.dir_path_completion.hide();
            }
        }
        None
    }

    fn update_dir_path_completions(&mut self) {
        let input = self.dir_path_input.content();
        let completions = find_path_completions(input);
        self.dir_path_completion.show(completions, CompletionKind::Argument);
    }

    fn apply_dir_path_completion(&mut self) {
        if let Some(value) = self.dir_path_completion.selected_value() {
            self.dir_path_input.set_content(value.to_string());
        }
    }
}

impl Default for TrafficView {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Settings integration
// =============================================================================

use crate::settings::TrafficSettings;

impl TrafficView {
    /// Apply settings loaded from disk.
    pub fn apply_settings(&mut self, settings: &TrafficSettings) {
        self.config.encoding_index = settings.encoding_index;
        self.config.show_tx = settings.show_tx;
        self.config.show_rx = settings.show_rx;
        self.config.show_timestamps = settings.show_timestamps;
        self.config.timestamp_format_index = settings.timestamp_format_index;
        self.config.auto_scroll = settings.auto_scroll;
        self.config.lock_to_bottom = settings.lock_to_bottom;
        self.config.search_mode_index = settings.search_mode_index;
        self.config.filter_mode_index = settings.filter_mode_index;
        self.config.wrap_text = settings.wrap_text;
        self.config.file_save_enabled = settings.file_save_enabled;
        self.config.file_save_format_index = settings.file_save_format_index;
        self.config.file_save_encoding_index = settings.file_save_encoding_index;
        self.config.file_save_directory = settings.file_save_directory.clone();
    }
    
    /// Extract settings for saving to disk.
    pub fn to_settings(&self) -> TrafficSettings {
        TrafficSettings {
            encoding_index: self.config.encoding_index,
            show_tx: self.config.show_tx,
            show_rx: self.config.show_rx,
            show_timestamps: self.config.show_timestamps,
            timestamp_format_index: self.config.timestamp_format_index,
            auto_scroll: self.config.auto_scroll,
            lock_to_bottom: self.config.lock_to_bottom,
            search_mode_index: self.config.search_mode_index,
            filter_mode_index: self.config.filter_mode_index,
            wrap_text: self.config.wrap_text,
            file_save_enabled: self.config.file_save_enabled,
            file_save_format_index: self.config.file_save_format_index,
            file_save_encoding_index: self.config.file_save_encoding_index,
            file_save_directory: self.config.file_save_directory.clone(),
        }
    }
}
