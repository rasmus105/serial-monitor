//! Traffic view: main data display with search and send functionality.

use std::time::SystemTime;

use unicode_width::UnicodeWidthStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Widget,
    },
};
use serial_core::{
    Direction as DataDirection, SerialConfig, SessionHandle,
    buffer::{PatternMode, SearchMatch},
    ui::{
        TimestampFormat,
        config::{
            ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_enabled, always_valid,
            always_visible,
        },
        encoding::{ENCODING_DISPLAY_NAMES, ENCODING_VARIANTS},
        parse_escape_sequences, slice_by_display_width,
    },
};

use crate::{
    app::{Focus, TrafficAction},
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfigKeyResult, ConfigPanel,
        ConnectionPanel, InputHistory, TextInput, handle_config_key,
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
    /// Search direction (true = forward with '/', false = backward with '?').
    pub search_forward: bool,
    /// Search input history.
    pub search_history: InputHistory,
    /// Filter input state.
    pub filter_input: TextInputState,
    /// Whether filter input is focused.
    pub filter_focused: bool,
    /// Filter input history.
    pub filter_history: InputHistory,
    /// Send input state.
    pub send_input: TextInputState,
    /// Whether send input is focused.
    pub send_focused: bool,
    /// Send input history.
    pub send_history: InputHistory,
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
    /// Visual mode state: whether visual selection is active.
    pub visual_mode: bool,
    /// Visual mode anchor: the chunk index where selection started.
    pub visual_anchor: usize,
    /// Visual mode cursor: the current chunk index in selection.
    pub visual_cursor: usize,
}

/// Traffic view configuration.
#[derive(Debug, Clone)]
pub struct TrafficConfig {
    pub encoding_index: usize,
    pub show_tx: bool,
    pub show_rx: bool,
    pub show_delimiter: bool,
    /// Whether we're in raw mode (no delimiter). Used to gray out show_delimiter toggle.
    /// This is synced from the buffer, not persisted.
    pub is_raw_mode: bool,
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
    // Send settings
    pub send_suffix_enabled: bool,
    pub send_suffix: String,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            encoding_index: 0, // UTF-8
            show_tx: true,
            show_rx: true,
            show_delimiter: true,
            is_raw_mode: true, // Default to raw mode (no delimiter)
            show_timestamps: true,
            timestamp_format_index: 0, // Relative
            auto_scroll: true,
            lock_to_bottom: false,
            search_mode_index: 0, // Normal
            filter_mode_index: 0, // Normal
            wrap_text: true,      // Wrap by default
            // File saving defaults
            file_save_enabled: false,
            file_save_format_index: 1,   // Encoded
            file_save_encoding_index: 0, // UTF-8
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
            // Send defaults
            send_suffix_enabled: true,
            send_suffix: r"\r\n".to_string(),
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
            FieldDef {
                id: "show_delimiter",
                label: "Delimiter",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_delimiter),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_delimiter = b;
                    }
                },
                visible: always_visible,
                // Disabled (grayed out) in raw mode since there's no delimiter to show/hide
                enabled: |c| !c.is_raw_mode,
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
    Section {
        header: Some("Send"),
        fields: &[
            FieldDef {
                id: "send_suffix_enabled",
                label: "Append Suffix",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.send_suffix_enabled),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.send_suffix_enabled = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "send_suffix",
                label: "Suffix",
                kind: FieldKind::TextInput {
                    placeholder: r"e.g. \r\n",
                },
                get: |c| FieldValue::string(c.send_suffix.clone()),
                set: |c, v| {
                    if let FieldValue::String(s) = v {
                        c.send_suffix = s.into_owned();
                    }
                },
                visible: always_visible,
                enabled: |c| c.send_suffix_enabled,
                parent_id: Some("send_suffix_enabled"),
                validate: always_valid,
            },
        ],
    },
];

impl TrafficView {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            search_input: TextInputState::default().with_placeholder("Search pattern..."),
            search_focused: false,
            search_forward: true,
            search_history: InputHistory::default(),
            filter_input: TextInputState::default().with_placeholder("Filter pattern..."),
            filter_focused: false,
            filter_history: InputHistory::default(),
            send_input: TextInputState::default().with_placeholder("Data to send..."),
            send_focused: false,
            send_history: InputHistory::default(),
            dir_path_input: TextInputState::default().with_placeholder("Enter directory path..."),
            dir_path_focused: false,
            dir_path_completion: CompletionState::default(),
            config: TrafficConfig::default(),
            config_nav: ConfigNav::new(),
            session_start: None,
            last_visible_height: 20, // Conservative default
            last_content_width: 80,  // Conservative default
            at_bottom: true,         // Start at bottom
            visual_mode: false,
            visual_anchor: 0,
            visual_cursor: 0,
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused || self.filter_focused || self.send_focused || self.dir_path_focused
    }

    /// Enter visual mode at the current scroll position.
    fn enter_visual_mode(&mut self, handle: &SessionHandle) {
        let chunk_idx = self.scroll_to_chunk_index(handle);
        self.visual_mode = true;
        self.visual_anchor = chunk_idx;
        self.visual_cursor = chunk_idx;
    }

    /// Exit visual mode.
    fn exit_visual_mode(&mut self) {
        self.visual_mode = false;
    }

    /// Move visual cursor and update scroll position.
    fn visual_move(&mut self, new_cursor: usize, handle: &SessionHandle) {
        let chunk_count = handle.buffer().len();
        self.visual_cursor = new_cursor.min(chunk_count.saturating_sub(1));

        // Update scroll to keep cursor visible
        let display_line = self.chunk_to_scroll_position(self.visual_cursor, handle);
        let visible_start = self.scroll;
        let visible_end = self.scroll + self.last_visible_height;

        if display_line < visible_start {
            self.scroll = display_line;
        } else if display_line >= visible_end {
            self.scroll = display_line.saturating_sub(self.last_visible_height - 1);
        }
    }

    /// Get the selected chunk range (start, end) inclusive.
    fn visual_selection_range(&self) -> (usize, usize) {
        let start = self.visual_anchor.min(self.visual_cursor);
        let end = self.visual_anchor.max(self.visual_cursor);
        (start, end)
    }

    /// Convert scroll position to chunk index at the center of visible area.
    fn scroll_to_chunk_index(&self, handle: &SessionHandle) -> usize {
        // Target the center of the visible area, not the top
        let center_line = self.scroll + self.last_visible_height / 2;

        if !self.config.wrap_text {
            return center_line;
        }

        // In wrap mode, need to find which chunk contains this display line
        let buffer = handle.buffer();
        let content_width = self.last_content_width.max(1);
        let mut display_line = 0;

        for (idx, chunk) in buffer.chunks().enumerate() {
            let chunk_lines = chunk.encoded.width().div_ceil(content_width).max(1);
            if display_line + chunk_lines > center_line {
                return idx;
            }
            display_line += chunk_lines;
        }

        buffer.len().saturating_sub(1)
    }

    /// Convert chunk index to scroll position (display line in wrap mode).
    fn chunk_to_scroll_position(&self, chunk_idx: usize, handle: &SessionHandle) -> usize {
        if !self.config.wrap_text {
            return chunk_idx;
        }

        // In wrap mode, sum display lines of all chunks before this one
        let buffer = handle.buffer();
        let content_width = self.last_content_width.max(1);
        let mut display_line = 0;

        for (idx, chunk) in buffer.chunks().enumerate() {
            if idx >= chunk_idx {
                break;
            }
            display_line += chunk.encoded.width().div_ceil(content_width).max(1);
        }

        display_line
    }

    /// Yank selected chunks to clipboard and return success message.
    fn yank_visual_selection(&mut self, handle: &SessionHandle) -> Option<String> {
        let (start, end) = self.visual_selection_range();
        let buffer = handle.buffer();

        // Collect content from selected chunks (just the data, no timestamps/direction)
        let mut content = String::new();
        for (idx, chunk) in buffer.chunks().enumerate() {
            if idx >= start && idx <= end {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(chunk.encoded);
            }
        }

        let chunk_count = end - start + 1;

        match crate::clipboard::copy_to_clipboard(&content) {
            Ok(()) => {
                self.exit_visual_mode();
                Some(format!(
                    "{} chunk{} yanked",
                    chunk_count,
                    if chunk_count == 1 { "" } else { "s" }
                ))
            }
            Err(e) => {
                self.exit_visual_mode();
                Some(e.to_string())
            }
        }
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

        // Sync show_delimiter
        buffer.set_show_delimiter(self.config.show_delimiter);
    }

    /// Update is_raw_mode from the buffer (call after session creation or when needed).
    pub fn update_raw_mode_from_buffer(&mut self, handle: &SessionHandle) {
        self.config.is_raw_mode = handle.buffer().is_raw_mode();
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
        let show_input_bar = self.search_focused
            || self.filter_focused
            || self.send_focused
            || self.dir_path_focused;
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
                let input_inner = Block::default().borders(Borders::ALL).inner(main_chunks[1]);
                CompletionPopup::new(&self.dir_path_completion, input_inner.y, input_inner.x)
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

        let block = Block::default().borders(Borders::ALL).border_style(
            if focus == Focus::Main && !self.is_input_mode() {
                Theme::border_focused()
            } else {
                Theme::border()
            },
        );

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
            buf.set_string(
                inner.x,
                y,
                " ".repeat(inner.width as usize),
                ratatui::style::Style::default(),
            );
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
                            let elapsed = chunk
                                .timestamp
                                .duration_since(session_start)
                                .unwrap_or_default();
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

        let prefix_width = base_prefix_width
            + if self.config.show_timestamps {
                timestamp_width + 1
            } else {
                0
            };
        let content_width = inner_width.saturating_sub(prefix_width);

        // Store for key handler calculations
        self.last_content_width = content_width;

        if self.config.wrap_text {
            self.draw_traffic_wrapped(
                area,
                buf,
                handle,
                &buffer,
                inner,
                visible_height,
                content_width,
                prefix_width,
                timestamp_width,
                block,
                current_match.as_ref(),
            );
        } else {
            self.draw_traffic_truncated(
                area,
                buf,
                handle,
                &buffer,
                inner,
                visible_height,
                content_width,
                prefix_width,
                timestamp_width,
                block,
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
        let bottom_padding = if max_scroll_content > 0 {
            visible_height / 10
        } else {
            0
        };
        let max_scroll = max_scroll_content + bottom_padding;
        let should_auto_scroll =
            self.config.lock_to_bottom || (self.config.auto_scroll && self.at_bottom);
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
        let save_indicator = if buffer.is_saving() { " [SAVING]" } else { "" };
        let search_info = if buffer.search_pattern().is_some() {
            let match_total = buffer.match_count();
            match buffer.current_match_index() {
                Some(idx) => format!(" [{}/{}]", idx + 1, match_total),
                None if match_total > 0 => format!(" [-/{}]", match_total),
                None => String::new(),
            }
        } else {
            String::new()
        };
        // Cap displayed scroll position at content max (don't show padding in title)
        let display_scroll = scroll.min(max_scroll_content);
        let visual_indicator = if self.visual_mode {
            let (start, end) = self.visual_selection_range();
            format!(" [VISUAL {}-{}]", start + 1, end + 1)
        } else {
            String::new()
        };
        let block = block.title(format!(
            " Traffic [{}/{}]{}{}{}{}{} ",
            display_scroll + 1,
            total.max(1),
            filter_info,
            search_info,
            lock_indicator,
            save_indicator,
            visual_indicator,
        ));
        block.render(area, buf);

        // Render chunks
        let mut y = inner.y;
        // Get visual selection range if in visual mode
        let visual_range = if self.visual_mode {
            Some(self.visual_selection_range())
        } else {
            None
        };

        for (visible_idx, chunk) in buffer
            .chunks()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
        {
            if y >= inner.y + inner.height {
                break;
            }

            // Apply visual selection/cursor background if this chunk is selected
            if let Some((start, end)) = visual_range
                && visible_idx >= start
                && visible_idx <= end
            {
                // Use brighter color for cursor line, darker for rest of selection
                let bg_color = if visible_idx == self.visual_cursor {
                    Theme::VISUAL_CURSOR
                } else {
                    Theme::VISUAL_SELECTION
                };
                for x in inner.x..inner.x + inner.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
            }

            // Get matches for this chunk
            let matches = buffer.matches_in_chunk(visible_idx);
            let line = self.format_chunk_line_highlighted(
                &chunk,
                timestamp_width,
                content_width,
                prefix_width,
                true,
                matches,
                current_match,
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
            }
            .max(1); // At least one line per chunk

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
        let bottom_padding = if max_scroll_content > 0 {
            visible_height / 10
        } else {
            0
        };
        let max_scroll = max_scroll_content + bottom_padding;
        let should_auto_scroll =
            self.config.lock_to_bottom || (self.config.auto_scroll && self.at_bottom);
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
        let save_indicator = if buffer.is_saving() { " [SAVING]" } else { "" };
        let search_info = if buffer.search_pattern().is_some() {
            let match_total = buffer.match_count();
            match buffer.current_match_index() {
                Some(idx) => format!(" [{}/{}]", idx + 1, match_total),
                None if match_total > 0 => format!(" [-/{}]", match_total),
                None => String::new(),
            }
        } else {
            String::new()
        };
        let visual_indicator = if self.visual_mode {
            let (start, end) = self.visual_selection_range();
            format!(" [VISUAL {}-{}]", start + 1, end + 1)
        } else {
            String::new()
        };
        let block = block.title(format!(
            " Traffic [{}/{}]{}{}{}{}{} ",
            display_scroll + 1,
            total_display_lines.max(1),
            filter_info,
            search_info,
            lock_indicator,
            save_indicator,
            visual_indicator,
        ));
        block.render(area, buf);

        // Render display lines starting from scroll position
        let mut y = inner.y;
        // Get visual selection range if in visual mode
        let visual_range = if self.visual_mode {
            Some(self.visual_selection_range())
        } else {
            None
        };

        for &(chunk_idx, line_within_chunk) in
            display_lines.iter().skip(scroll).take(visible_height)
        {
            if y >= inner.y + inner.height {
                break;
            }

            // Apply visual selection/cursor background if this chunk is selected
            if let Some((start, end)) = visual_range
                && chunk_idx >= start
                && chunk_idx <= end
            {
                // Use brighter color for cursor line, darker for rest of selection
                let bg_color = if chunk_idx == self.visual_cursor {
                    Theme::VISUAL_CURSOR
                } else {
                    Theme::VISUAL_SELECTION
                };
                for x in inner.x..inner.x + inner.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(bg_color);
                    }
                }
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
                    chunk,
                    timestamp_width,
                    content,
                    start,
                    end,
                    matches,
                    current_match,
                );
                Paragraph::new(line).render(line_area, buf);
            } else {
                // Continuation line - indent to align with content
                let indent = " ".repeat(prefix_width);
                let content_spans =
                    self.create_highlighted_spans(content, start, end, matches, current_match);
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
            let formatted = self
                .config
                .timestamp_format()
                .format(chunk.timestamp, session_start);
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
        let content_spans =
            self.create_highlighted_spans(content, 0, byte_end, matches, current_match);
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
            let formatted = self
                .config
                .timestamp_format()
                .format(chunk.timestamp, session_start);
            let padded = format!("{:>width$} ", formatted, width = timestamp_width);
            spans.push(Span::styled(padded, Theme::muted()));
        }

        let content_spans = self.create_highlighted_spans(
            full_content,
            byte_start,
            byte_end,
            matches,
            current_match,
        );
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

    fn render_scrollbar(
        &self,
        area: Rect,
        buf: &mut Buffer,
        content_length: usize,
        position: usize,
    ) {
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
            let direction = if self.search_forward { "/" } else { "?" };
            let title = if status.is_empty() {
                format!("Search {}", direction)
            } else {
                format!("Search {} [{}]", direction, status)
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
        // Handle visual mode keys first
        if self.visual_mode {
            return self.handle_visual_mode_key(key, handle);
        }

        // Half-page scroll amount based on actual visible height
        let half_page = self.last_visible_height / 2;

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        let buffer = handle.buffer();

        // Calculate total scrollable items based on wrap mode
        let total = if self.config.wrap_text {
            // Count display lines (wrapped) using display width
            let content_width = self.last_content_width.max(1);
            buffer
                .chunks()
                .map(|chunk| {
                    let content_display_width = chunk.encoded.width();
                    content_display_width.div_ceil(content_width).max(1)
                })
                .sum()
        } else {
            // Count chunks
            buffer.len()
        };

        // Use the last known visible height for accurate scroll bounds
        // Add 10% bottom padding to clearly show when at the very bottom
        // Only apply padding when there's actually scrollable content
        let max_scroll_content = total.saturating_sub(self.last_visible_height);
        let bottom_padding = if max_scroll_content > 0 {
            self.last_visible_height / 10
        } else {
            0
        };
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
                self.search_forward = true;
            }
            KeyCode::Char('?') => {
                self.search_focused = true;
                self.search_forward = false;
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
                    let display_offsets: Vec<usize> = buffer
                        .chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_display_width = chunk.encoded.width();
                            let num_lines = content_display_width.div_ceil(content_width).max(1);
                            *acc += num_lines;
                            Some(offset)
                        })
                        .collect();
                    drop(buffer);
                    handle
                        .buffer_mut()
                        .goto_next_match()
                        .map(|idx| display_offsets.get(idx).copied().unwrap_or(0))
                } else {
                    drop(buffer);
                    handle.buffer_mut().goto_next_match()
                };

                if let Some(pos) = scroll_pos {
                    // Only navigate if not in lock mode, or if match is visible from bottom
                    if !self.config.lock_to_bottom {
                        // Center the match in the middle of the visible area (like vim's nzz)
                        let offset = self.last_visible_height / 2;
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
                    let display_offsets: Vec<usize> = buffer
                        .chunks()
                        .scan(0usize, |acc, chunk| {
                            let offset = *acc;
                            let content_display_width = chunk.encoded.width();
                            let num_lines = content_display_width.div_ceil(content_width).max(1);
                            *acc += num_lines;
                            Some(offset)
                        })
                        .collect();
                    drop(buffer);
                    handle
                        .buffer_mut()
                        .goto_prev_match()
                        .map(|idx| display_offsets.get(idx).copied().unwrap_or(0))
                } else {
                    drop(buffer);
                    handle.buffer_mut().goto_prev_match()
                };

                if let Some(pos) = scroll_pos {
                    // Only navigate if not in lock mode
                    if !self.config.lock_to_bottom {
                        // Center the match in the middle of the visible area (like vim's nzz)
                        let offset = self.last_visible_height / 2;
                        self.scroll = pos.saturating_sub(offset);
                        self.at_bottom = self.scroll >= max_scroll;
                    }
                }
            }
            KeyCode::Char('v') => {
                // Enter visual mode
                drop(buffer);
                self.enter_visual_mode(handle);
            }
            _ => {}
        }
        None
    }

    fn handle_visual_mode_key(
        &mut self,
        key: KeyEvent,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let chunk_count = handle.buffer().len();
        let half_page = self.last_visible_height / 2;

        match key.code {
            KeyCode::Esc | KeyCode::Char('v') => {
                self.exit_visual_mode();
            }
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                let new_cursor = self.visual_cursor.saturating_add(1);
                self.visual_move(new_cursor, handle);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                let new_cursor = self.visual_cursor.saturating_sub(1);
                self.visual_move(new_cursor, handle);
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down
                let new_cursor = self
                    .visual_cursor
                    .saturating_add(half_page)
                    .min(chunk_count.saturating_sub(1));
                self.visual_move(new_cursor, handle);
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up
                let new_cursor = self.visual_cursor.saturating_sub(half_page);
                self.visual_move(new_cursor, handle);
            }
            KeyCode::Char('g') => {
                // Go to top
                self.visual_move(0, handle);
            }
            KeyCode::Char('G') => {
                // Go to bottom
                self.visual_move(chunk_count.saturating_sub(1), handle);
            }
            KeyCode::Char('y') => {
                // Yank selection to clipboard
                if let Some(msg) = self.yank_visual_selection(handle) {
                    return Some(TrafficAction::Toast(crate::widget::Toast::info(msg)));
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(
        &mut self,
        key: KeyEvent,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        // Keys that can trigger a toggle: Enter, Space, h, l, Left, Right
        let is_toggle_key = matches!(
            key.code,
            KeyCode::Enter
                | KeyCode::Char(' ')
                | KeyCode::Char('h')
                | KeyCode::Char('l')
                | KeyCode::Left
                | KeyCode::Right
        );

        // Track if we're about to toggle file_save_enabled
        let mut toggling_file_save = false;
        let file_save_was_enabled = self.config.file_save_enabled;

        // Track if we're about to toggle a filter option (show_tx, show_rx)
        let mut toggling_filter = false;
        let (show_tx_was, show_rx_was) = (self.config.show_tx, self.config.show_rx);

        if is_toggle_key
            && !self.config_nav.edit_mode.is_dropdown()
            && let Some(field) = self
                .config_nav
                .current_field(TRAFFIC_CONFIG_SECTIONS, &self.config)
        {
            // Handle text input field (directory)
            if field.kind.is_text_input()
                && field.id == "file_save_directory"
                && matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
            {
                self.dir_path_input
                    .set_content(&self.config.file_save_directory);
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

            // Check if toggling a filter option
            if matches!(field.id, "show_tx" | "show_rx") && matches!(field.kind, FieldKind::Toggle)
            {
                toggling_filter = true;
            }
        }

        // If toggling filter, capture the middle visible line's raw index before the change
        let middle_raw_index = if toggling_filter {
            self.calculate_middle_raw_index(handle)
        } else {
            None
        };

        let result = handle_config_key(
            key,
            &mut self.config_nav,
            TRAFFIC_CONFIG_SECTIONS,
            &mut self.config,
        );

        match result {
            ConfigKeyResult::Changed => {
                self.sync_config_to_buffer(handle);

                // If filter was toggled, adjust scroll to keep the same line centered
                if toggling_filter
                    && (self.config.show_tx != show_tx_was || self.config.show_rx != show_rx_was)
                    && let Some(raw_idx) = middle_raw_index
                {
                    self.scroll_to_center_raw_index(raw_idx, handle);
                }

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

    /// Calculate the raw chunk index at the middle of the visible area.
    fn calculate_middle_raw_index(&self, handle: &SessionHandle) -> Option<usize> {
        let buffer = handle.buffer();
        if buffer.is_empty() {
            return None;
        }

        let content_width = self.last_content_width.max(1);
        let middle_display_line = self.scroll + self.last_visible_height / 2;

        let middle_visible_idx = if self.config.wrap_text {
            // In wrapped mode, find which chunk the middle display line belongs to
            let mut acc = 0usize;
            let mut found_idx = 0;
            for (i, chunk) in buffer.chunks().enumerate() {
                let content_display_width = chunk.encoded.width();
                let num_lines = content_display_width.div_ceil(content_width).max(1);
                let next_acc = acc + num_lines;
                if acc <= middle_display_line && middle_display_line < next_acc {
                    found_idx = i;
                    break;
                }
                if next_acc > middle_display_line {
                    break;
                }
                found_idx = i;
                acc = next_acc;
            }
            found_idx
        } else {
            // In truncated mode, scroll position is the visible chunk index
            (self.scroll + self.last_visible_height / 2).min(buffer.len().saturating_sub(1))
        };

        buffer.visible_to_raw_index(middle_visible_idx)
    }

    /// Scroll to center a raw chunk index in the view.
    fn scroll_to_center_raw_index(&mut self, raw_index: usize, handle: &SessionHandle) {
        let buffer = handle.buffer();

        // Find where this raw index ends up in the new filtered view
        let visible_idx = match buffer.nearest_visible_from_raw(raw_index) {
            Some(idx) => idx,
            None => {
                // Filtered view is empty
                self.scroll = 0;
                return;
            }
        };

        let content_width = self.last_content_width.max(1);

        if self.config.wrap_text {
            // Calculate display line offset for this visible index
            let mut display_line = 0;
            for (i, chunk) in buffer.chunks().enumerate() {
                if i == visible_idx {
                    break;
                }
                let content_display_width = chunk.encoded.width();
                let num_lines = content_display_width.div_ceil(content_width).max(1);
                display_line += num_lines;
            }
            // Center it: position with half the visible height above
            self.scroll = display_line.saturating_sub(self.last_visible_height / 2);
        } else {
            // In truncated mode, visible index = scroll position for centering
            self.scroll = visible_idx.saturating_sub(self.last_visible_height / 2);
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

    fn handle_search_key(
        &mut self,
        key: KeyEvent,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        // Handle history navigation with Ctrl+p/n
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('p') => {
                    if let Some(entry) = self.search_history.prev(self.search_input.content()) {
                        self.search_input.set_content(entry.to_string());
                        // Update search pattern
                        let pattern = self.search_input.content();
                        if !pattern.is_empty() {
                            let mode = if self.config.search_mode_index == 1 {
                                PatternMode::Regex
                            } else {
                                PatternMode::Normal
                            };
                            let _ = handle.buffer_mut().set_search_pattern(pattern, mode);
                        } else {
                            handle.buffer_mut().clear_search();
                        }
                    }
                    return None;
                }
                KeyCode::Char('n') => {
                    if let Some(entry) = self.search_history.next_entry() {
                        self.search_input.set_content(entry.to_string());
                        // Update search pattern
                        let pattern = self.search_input.content();
                        if !pattern.is_empty() {
                            let mode = if self.config.search_mode_index == 1 {
                                PatternMode::Regex
                            } else {
                                PatternMode::Normal
                            };
                            let _ = handle.buffer_mut().set_search_pattern(pattern, mode);
                        } else {
                            handle.buffer_mut().clear_search();
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                // Add to history before confirming
                self.search_history.push(self.search_input.content());
                self.search_history.reset_navigation();

                // Confirm search and exit search mode
                // Pattern is already set via incremental search
                self.search_focused = false;

                // Jump to the nearest match forward from the middle visible line
                let buffer = handle.buffer();
                let content_width = self.last_content_width.max(1);

                // Calculate visible chunk index at middle of screen
                let middle_display_line = self.scroll + self.last_visible_height / 2;

                let (middle_chunk_idx, display_offsets) = if self.config.wrap_text {
                    // Build display line offsets and find chunk at middle
                    let mut offsets = Vec::with_capacity(buffer.len());
                    let mut middle_idx = 0;
                    let mut acc = 0usize;
                    for (i, chunk) in buffer.chunks().enumerate() {
                        offsets.push(acc);
                        let content_display_width = chunk.encoded.width();
                        let num_lines = content_display_width.div_ceil(content_width).max(1);
                        let next_acc = acc + num_lines;
                        // Check if middle_display_line falls within this chunk
                        if acc <= middle_display_line && middle_display_line < next_acc {
                            middle_idx = i;
                        }
                        acc = next_acc;
                    }
                    // Handle case where middle_display_line is past all content
                    if middle_display_line >= acc && !offsets.is_empty() {
                        middle_idx = offsets.len().saturating_sub(1);
                    }
                    (middle_idx, Some(offsets))
                } else {
                    // In truncated mode, scroll position = chunk index
                    let middle_idx = (self.scroll + self.last_visible_height / 2)
                        .min(buffer.len().saturating_sub(1));
                    (middle_idx, None)
                };

                drop(buffer);

                // Jump to the first match in the appropriate direction from the middle chunk
                let scroll_pos = if self.search_forward {
                    handle
                        .buffer_mut()
                        .goto_match_from(middle_chunk_idx)
                        .map(|idx| {
                            if let Some(offsets) = &display_offsets {
                                offsets.get(idx).copied().unwrap_or(0)
                            } else {
                                idx
                            }
                        })
                } else {
                    handle
                        .buffer_mut()
                        .goto_match_before(middle_chunk_idx)
                        .map(|idx| {
                            if let Some(offsets) = &display_offsets {
                                offsets.get(idx).copied().unwrap_or(0)
                            } else {
                                idx
                            }
                        })
                };

                if let Some(pos) = scroll_pos {
                    // Position match with 20% of visible height above it (like vim scrolloff)
                    let offset = self.last_visible_height / 5;
                    self.scroll = pos.saturating_sub(offset);
                }

                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc => {
                self.search_history.reset_navigation();
                self.search_focused = false;
                self.search_input.clear();
                handle.buffer_mut().clear_search();
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                // Reset history navigation when typing
                self.search_history.reset_navigation();

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

    fn handle_filter_key(
        &mut self,
        key: KeyEvent,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        // Handle history navigation with Ctrl+p/n
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('p') => {
                    // Capture middle line before filter change
                    let middle_raw_index = self.calculate_middle_raw_index(handle);

                    if let Some(entry) = self.filter_history.prev(self.filter_input.content()) {
                        self.filter_input.set_content(entry.to_string());
                        // Update filter pattern
                        let pattern = self.filter_input.content();
                        if !pattern.is_empty() {
                            let mode = if self.config.filter_mode_index == 1 {
                                PatternMode::Regex
                            } else {
                                PatternMode::Normal
                            };
                            let _ = handle.buffer_mut().set_filter_pattern(pattern, mode);
                        } else {
                            handle.buffer_mut().clear_filter();
                        }
                        // Restore scroll position
                        if let Some(raw_idx) = middle_raw_index {
                            self.scroll_to_center_raw_index(raw_idx, handle);
                        }
                    }
                    return None;
                }
                KeyCode::Char('n') => {
                    // Capture middle line before filter change
                    let middle_raw_index = self.calculate_middle_raw_index(handle);

                    if let Some(entry) = self.filter_history.next_entry() {
                        self.filter_input.set_content(entry.to_string());
                        // Update filter pattern
                        let pattern = self.filter_input.content();
                        if !pattern.is_empty() {
                            let mode = if self.config.filter_mode_index == 1 {
                                PatternMode::Regex
                            } else {
                                PatternMode::Normal
                            };
                            let _ = handle.buffer_mut().set_filter_pattern(pattern, mode);
                        } else {
                            handle.buffer_mut().clear_filter();
                        }
                        // Restore scroll position
                        if let Some(raw_idx) = middle_raw_index {
                            self.scroll_to_center_raw_index(raw_idx, handle);
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                // Add to history before confirming
                self.filter_history.push(self.filter_input.content());
                self.filter_history.reset_navigation();

                // Confirm filter and exit filter mode
                // Pattern is already set via incremental filtering
                self.filter_focused = false;
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc => {
                self.filter_history.reset_navigation();

                // Capture middle line before clearing filter
                let middle_raw_index = self.calculate_middle_raw_index(handle);

                self.filter_focused = false;
                self.filter_input.clear();
                handle.buffer_mut().clear_filter();

                // Restore scroll position to keep same line centered
                if let Some(raw_idx) = middle_raw_index {
                    self.scroll_to_center_raw_index(raw_idx, handle);
                }

                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                // Reset history navigation when typing
                self.filter_history.reset_navigation();

                // Capture middle line before filter change
                let middle_raw_index = self.calculate_middle_raw_index(handle);

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

                // Restore scroll position to keep same line centered
                if let Some(raw_idx) = middle_raw_index {
                    self.scroll_to_center_raw_index(raw_idx, handle);
                }
            }
        }
        None
    }

    fn handle_send_key(&mut self, key: KeyEvent) -> Option<TrafficAction> {
        // Handle history navigation with Ctrl+p/n
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('p') => {
                    if let Some(entry) = self.send_history.prev(self.send_input.content()) {
                        self.send_input.set_content(entry.to_string());
                    }
                    return None;
                }
                KeyCode::Char('n') => {
                    if let Some(entry) = self.send_history.next_entry() {
                        self.send_input.set_content(entry.to_string());
                    }
                    return None;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                let data = self.send_input.take();
                if !data.is_empty() {
                    // Add to history before sending
                    self.send_history.push(&data);
                    self.send_history.reset_navigation();

                    self.send_focused = false;

                    // Parse escape sequences in user input
                    let mut bytes = parse_escape_sequences(&data);

                    // Append suffix if enabled
                    if self.config.send_suffix_enabled && !self.config.send_suffix.is_empty() {
                        let suffix_bytes = parse_escape_sequences(&self.config.send_suffix);
                        bytes.extend(suffix_bytes);
                    }

                    return Some(TrafficAction::Send(bytes));
                }
            }
            KeyCode::Esc => {
                self.send_history.reset_navigation();
                self.send_focused = false;
                self.send_input.clear();
                // Layout changed - request clear to avoid artifacts
                return Some(TrafficAction::RequestClear);
            }
            _ => {
                // Reset history navigation when typing
                self.send_history.reset_navigation();
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
        self.dir_path_completion
            .show(completions, CompletionKind::Argument);
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
        self.config.show_delimiter = settings.show_delimiter;
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
        self.config.send_suffix_enabled = settings.send_suffix_enabled;
        self.config.send_suffix = settings.send_suffix.clone();
    }

    /// Extract settings for saving to disk.
    pub fn to_settings(&self) -> TrafficSettings {
        TrafficSettings {
            encoding_index: self.config.encoding_index,
            show_tx: self.config.show_tx,
            show_rx: self.config.show_rx,
            show_delimiter: self.config.show_delimiter,
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
            send_suffix_enabled: self.config.send_suffix_enabled,
            send_suffix: self.config.send_suffix.clone(),
        }
    }
}
