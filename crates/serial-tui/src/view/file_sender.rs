//! File sender view: send files with progress tracking.

use std::borrow::Cow;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};
use serial_core::{
    ChunkMode, Delimiter, FileSendConfig, FileSendHandle, FileSendProgress, SerialConfig,
    SessionHandle, send_file,
    ui::{
        SizeUnit, TimeUnit,
        config::{ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_enabled, always_valid, always_visible},
    },
};

use crate::{
    app::{FileSenderAction, Focus},
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfigPanel, ConnectionPanel,
        Toast, format_bytes, handle_config_key,
        text_input::{TextInputState, find_path_completions},
    },
};

// Highlight colors for progress indication (foreground only for subtlety)
const SENT_COLOR: Color = Color::Green;         // Green text for already-sent data
const CURRENT_COLOR: Color = Color::Yellow;     // Yellow text for the current chunk

/// File sender view state.
pub struct FileSenderView {
    /// File path input (from config panel).
    pub file_path_input: TextInputState,
    /// Whether file path input is focused (from config panel).
    pub file_path_focused: bool,
    /// File path completion state (from config panel).
    pub file_path_completion: CompletionState,
    /// Selected file path.
    pub selected_path: Option<PathBuf>,
    /// File preview content.
    pub preview: Option<FilePreview>,
    /// Sender config.
    pub config: FileSenderConfig,
    /// Config panel navigation.
    pub config_nav: ConfigNav,
    /// Active send handle.
    pub send_handle: Option<FileSendHandle>,
    /// Latest progress.
    pub progress: Option<FileSendProgress>,
    /// Current scroll position (in display lines).
    pub scroll: usize,
    /// Cached visible height for scroll calculations.
    pub last_visible_height: usize,
}

/// Preview of selected file.
#[derive(Debug, Clone)]
pub struct FilePreview {
    pub size: u64,
    /// Raw bytes of the file (up to limit)
    pub raw_bytes: Vec<u8>,
    pub is_binary: bool,
    pub line_count: Option<usize>,
    /// Whether the file was truncated due to size limit
    pub truncated: bool,
}

/// File sender configuration.
#[derive(Debug, Clone)]
pub struct FileSenderConfig {
    pub file_path: String,
    /// Chunking mode: 0 = Delimiter, 1 = Bytes
    pub chunk_mode_index: usize,
    /// Delimiter index (for delimiter mode)
    pub delimiter_index: usize,
    /// Whether to include delimiter in sent chunks
    pub include_delimiter: bool,
    /// Number of lines per chunk (for delimiter mode)
    pub lines_per_chunk: usize,
    /// Byte chunk size value (for bytes mode)
    pub byte_chunk_value: usize,
    /// Byte chunk unit index
    pub byte_unit_index: usize,
    /// Whether to append a suffix to each chunk
    pub append_suffix: bool,
    /// Suffix delimiter index
    pub suffix_delimiter_index: usize,
    /// Delay value
    pub delay_value: usize,
    /// Delay unit index
    pub delay_unit_index: usize,
    pub repeat: bool,
    pub is_sending: bool,
    /// Preview size limit value
    pub preview_limit_value: usize,
    /// Preview size limit unit index (0=KB, 1=MB)
    pub preview_limit_unit_index: usize,
    /// Auto-follow current chunk during sending
    pub auto_follow: bool,
}

impl Default for FileSenderConfig {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            chunk_mode_index: 0, // Delimiter
            delimiter_index: 0,  // LF
            include_delimiter: true,
            lines_per_chunk: 1,
            byte_chunk_value: 64,
            byte_unit_index: 0, // Bytes
            append_suffix: false,
            suffix_delimiter_index: 0, // LF
            delay_value: 10,
            delay_unit_index: 0, // Milliseconds
            repeat: false,
            is_sending: false,
            preview_limit_value: 1,     // 1 MB default
            preview_limit_unit_index: 1, // MB
            auto_follow: true,
        }
    }
}

const CHUNK_MODE_OPTIONS: &[&str] = &["Delimiter", "Bytes"];
const PREVIEW_LIMIT_UNIT_OPTIONS: &[&str] = &["KiB", "MiB"];

// These must match the display_name() output from serial-core enums
const DELIMITER_OPTIONS: &[&str] = &["LF (\\n)", "CRLF (\\r\\n)", "CR (\\r)"];
const TIME_UNIT_OPTIONS: &[&str] = &["ms", "s", "min", "h"];
const SIZE_UNIT_OPTIONS: &[&str] = &["B", "KiB", "MiB"];

static FILE_SENDER_CONFIG_SECTIONS: &[Section<FileSenderConfig>] = &[
    Section {
        header: Some("File"),
        fields: &[
            FieldDef {
                id: "file_path",
                label: "Path",
                kind: FieldKind::TextInput {
                    placeholder: "Enter file path...",
                },
                get: |c| FieldValue::string(c.file_path.clone()),
                set: |c, v| {
                    if let FieldValue::String(s) = v {
                        c.file_path = s.into_owned();
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Chunking"),
        fields: &[
            FieldDef {
                id: "chunk_mode",
                label: "Mode",
                kind: FieldKind::Select {
                    options: CHUNK_MODE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.chunk_mode_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.chunk_mode_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "delimiter",
                label: "Delimiter",
                kind: FieldKind::Select {
                    options: DELIMITER_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.delimiter_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.delimiter_index = i;
                    }
                },
                visible: |c| c.chunk_mode_index == 0, // Only for delimiter mode
                enabled: |c| !c.is_sending,
                parent_id: Some("chunk_mode"),
                validate: always_valid,
            },
            FieldDef {
                id: "include_delimiter",
                label: "Include Delimiter",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.include_delimiter),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.include_delimiter = b;
                    }
                },
                visible: |c| c.chunk_mode_index == 0, // Only for delimiter mode
                enabled: |c| !c.is_sending,
                parent_id: Some("delimiter"),
                validate: always_valid,
            },
            FieldDef {
                id: "lines_per_chunk",
                label: "Lines per Chunk",
                kind: FieldKind::NumericInput { min: Some(1), max: None },
                get: |c| FieldValue::Usize(c.lines_per_chunk),
                set: |c, v| {
                    if let FieldValue::Usize(n) = v {
                        c.lines_per_chunk = n;
                    }
                },
                visible: |c| c.chunk_mode_index == 0, // Only for delimiter mode
                enabled: |c| !c.is_sending,
                parent_id: Some("delimiter"),
                validate: |v| {
                    if let FieldValue::Usize(n) = v
                        && *n == 0
                    {
                        return Err(Cow::Borrowed("Must be >= 1"));
                    }
                    Ok(())
                },
            },
            FieldDef {
                id: "byte_chunk_value",
                label: "Size",
                kind: FieldKind::NumericInput { min: Some(1), max: None },
                get: |c| FieldValue::Usize(c.byte_chunk_value),
                set: |c, v| {
                    if let FieldValue::Usize(n) = v {
                        c.byte_chunk_value = n;
                    }
                },
                visible: |c| c.chunk_mode_index == 1, // Only for bytes mode
                enabled: |c| !c.is_sending,
                parent_id: Some("chunk_mode"),
                validate: |v| {
                    if let FieldValue::Usize(n) = v
                        && *n == 0
                    {
                        return Err(Cow::Borrowed("Size must be > 0"));
                    }
                    Ok(())
                },
            },
            FieldDef {
                id: "byte_unit",
                label: "Unit",
                kind: FieldKind::Select {
                    options: SIZE_UNIT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.byte_unit_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.byte_unit_index = i;
                    }
                },
                visible: |c| c.chunk_mode_index == 1, // Only for bytes mode
                enabled: |c| !c.is_sending,
                parent_id: Some("byte_chunk_value"),
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Send Options"),
        fields: &[
            FieldDef {
                id: "append_suffix",
                label: "Append Suffix",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.append_suffix),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.append_suffix = b;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "suffix_delimiter",
                label: "Suffix",
                kind: FieldKind::Select {
                    options: DELIMITER_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.suffix_delimiter_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.suffix_delimiter_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending && c.append_suffix,
                parent_id: Some("append_suffix"),
                validate: always_valid,
            },
            FieldDef {
                id: "delay_value",
                label: "Delay",
                kind: FieldKind::NumericInput { min: Some(0), max: None },
                get: |c| FieldValue::Usize(c.delay_value),
                set: |c, v| {
                    if let FieldValue::Usize(n) = v {
                        c.delay_value = n;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "delay_unit",
                label: "Unit",
                kind: FieldKind::Select {
                    options: TIME_UNIT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.delay_unit_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.delay_unit_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: Some("delay_value"),
                validate: always_valid,
            },
            FieldDef {
                id: "repeat",
                label: "Repeat",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.repeat),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.repeat = b;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Preview"),
        fields: &[
            FieldDef {
                id: "preview_limit_value",
                label: "Max Size",
                kind: FieldKind::NumericInput { min: Some(1), max: None },
                get: |c| FieldValue::Usize(c.preview_limit_value),
                set: |c, v| {
                    if let FieldValue::Usize(n) = v {
                        c.preview_limit_value = n;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: None,
                validate: |v| {
                    if let FieldValue::Usize(n) = v
                        && *n == 0
                    {
                        return Err(Cow::Borrowed("Max size must be > 0"));
                    }
                    Ok(())
                },
            },
            FieldDef {
                id: "preview_limit_unit",
                label: "Unit",
                kind: FieldKind::Select {
                    options: PREVIEW_LIMIT_UNIT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.preview_limit_unit_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.preview_limit_unit_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.is_sending,
                parent_id: Some("preview_limit_value"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_follow",
                label: "Auto-follow",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_follow),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_follow = b;
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
        header: Some("Control"),
        fields: &[
            FieldDef {
                id: "send_active",
                label: "Send",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.is_sending),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.is_sending = b;
                    }
                },
                visible: always_visible,
                enabled: |c| !c.file_path.is_empty() || c.is_sending,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
];

impl FileSenderView {
    pub fn new() -> Self {
        Self {
            file_path_input: TextInputState::default().with_placeholder("Enter file path..."),
            file_path_focused: false,
            file_path_completion: CompletionState::default(),
            selected_path: None,
            preview: None,
            config: FileSenderConfig::default(),
            config_nav: ConfigNav::new(),
            send_handle: None,
            progress: None,
            scroll: 0,
            last_visible_height: 0,
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.file_path_focused
            || self.config_nav.edit_mode.is_text_input()
            || self.config_nav.edit_mode.is_dropdown()
    }

    pub fn is_sending(&self) -> bool {
        self.send_handle.is_some()
    }

    pub fn tick(&mut self) -> Option<FileSenderAction> {
        let handle = self.send_handle.as_ref()?;

        let progress = handle.progress();
        let complete = progress.complete;
        let bytes_sent = progress.bytes_sent;
        let error = progress.error.clone();

        // Auto-follow: scroll to keep current chunk visible
        if self.config.auto_follow && !complete {
            let current_line = self.byte_offset_to_line(bytes_sent);
            let visible_height = self.last_visible_height;

            // Keep current chunk roughly centered or at least visible
            if current_line >= self.scroll + visible_height.saturating_sub(2) {
                // Scroll down to keep current chunk visible
                self.scroll = current_line.saturating_sub(visible_height / 3);
            }
        }

        self.progress = Some(progress);

        if complete {
            self.send_handle = None;
            self.config.is_sending = false;

            // Return error toast if there was an error (but not for cancellation)
            if let Some(err) = error
                && err != "Cancelled"
            {
                return Some(FileSenderAction::Toast(Toast::error(format!(
                    "File send failed: {err}"
                ))));
            }
        }

        None
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
        // Main layout: preview + progress (+ optional input bar)
        let show_input_bar = self.file_path_focused;
        let main_chunks = if show_input_bar {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(6),     // Preview
                    Constraint::Length(4),  // Progress
                    Constraint::Length(3),  // Input bar
                ])
                .split(main_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(6),     // Preview
                    Constraint::Length(4),  // Progress
                ])
                .split(main_area)
        };

        // Preview
        let preview_title = if let (Some(path), Some(preview)) = (&self.selected_path, &self.preview) {
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let size_str = format_bytes(preview.size);

            let truncated_indicator = if preview.truncated { " [truncated]" } else { "" };

            if let Some(lines) = preview.line_count {
                format!(" Preview: {} ({}, {} lines){} ", filename, size_str, lines, truncated_indicator)
            } else {
                format!(" Preview: {} ({}, binary){} ", filename, size_str, truncated_indicator)
            }
        } else {
            " Preview ".to_string()
        };

        let preview_block = Block::default()
            .title(preview_title)
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main && !self.file_path_focused {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let preview_inner = preview_block.inner(main_chunks[0]);
        preview_block.render(main_chunks[0], buf);

        // Store visible height for scroll calculations
        self.last_visible_height = preview_inner.height as usize;

        if let Some(preview) = &self.preview {
            // Pass both outer area (for scrollbar in border) and inner area (for content)
            self.draw_preview(main_chunks[0], preview_inner, buf, preview);
        } else {
            Paragraph::new("Select a file using the config panel →")
                .style(Theme::muted())
                .render(preview_inner, buf);
        }

        // Progress/stats area
        let stats_block = Block::default()
            .title(" Progress ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let stats_inner = stats_block.inner(main_chunks[1]);
        stats_block.render(main_chunks[1], buf);

        // Calculate progress values (use preview size if no progress yet)
        let (bytes_sent, total_bytes, chunks_sent, loops, percentage) =
            if let Some(progress) = &self.progress {
                (
                    progress.bytes_sent,
                    progress.total_bytes,
                    progress.chunks_sent,
                    progress.loops_completed,
                    (progress.percentage() * 100.0) as u16,
                )
            } else if let Some(preview) = &self.preview {
                (0, preview.size, 0, 0, 0)
            } else {
                // No file selected
                (0, 0, 0, 0, 0)
            };

        // Row 1: Progress bar with percentage inside
        let gauge_label = format!("{}%", percentage);
        let gauge = Gauge::default()
            .ratio(percentage as f64 / 100.0)
            .label(gauge_label)
            .gauge_style(Style::default().fg(Theme::PRIMARY).bg(Theme::GAUGE_BG));

        if stats_inner.height > 0 {
            let gauge_area = Rect::new(stats_inner.x, stats_inner.y, stats_inner.width, 1);
            gauge.render(gauge_area, buf);
        }

        // Row 2: Stats line with separators: "Bytes: X / Y │ Chunks: X / Y │ Loops: X"
        if stats_inner.height > 1 {
            let stats_y = stats_inner.y + 1;

            let mut parts = vec![
                format!(
                    "Bytes: {} / {}",
                    format_bytes(bytes_sent),
                    format_bytes(total_bytes)
                ),
                format!("Chunks: {}", chunks_sent),
            ];

            // Only show loops if repeat is enabled
            if self.config.repeat {
                parts.push(format!("Loops: {}", loops));
            }

            let stats_line = parts.join(" │ ");

            Paragraph::new(stats_line)
                .style(Theme::muted())
                .render(Rect::new(stats_inner.x, stats_y, stats_inner.width, 1), buf);
        }

        // Draw input bar if file path input is focused
        if show_input_bar {
            self.draw_input_bar(main_chunks[2], buf);
            
            // Render file path completion popup (above the input bar)
            if self.file_path_completion.visible {
                let input_inner = Block::default()
                    .borders(Borders::ALL)
                    .inner(main_chunks[2]);
                CompletionPopup::new(
                    &self.file_path_completion,
                    input_inner.y,
                    input_inner.x,
                )
                .render(main_area, buf);
            }
        }

        // Config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, handle, serial_config, focus);
        }
    }

    fn draw_input_bar(&self, area: Rect, buf: &mut Buffer) {
        use crate::widget::TextInput;
        
        let block = Block::default()
            .title(" File Path ")
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let mut state = self.file_path_input.clone();
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

        // File sender config
        let config_block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Config {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        ConfigPanel::new(FILE_SENDER_CONFIG_SECTIONS, &self.config, &self.config_nav)
            .block(config_block)
            .focused(focus == Focus::Config)
            .render(chunks[1], buf);
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus) -> Option<FileSenderAction> {
        if self.file_path_focused {
            return self.handle_file_path_key(key);
        }

        match focus {
            Focus::Main => self.handle_main_key(key),
            Focus::Config => self.handle_config_key(key),
        }
    }

    /// Draw the file preview with scrolling and progress highlighting.
    fn draw_preview(&self, outer_area: Rect, inner_area: Rect, buf: &mut Buffer, preview: &FilePreview) {
        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let visible_height = inner_area.height as usize;

        // Calculate chunk boundaries for highlighting
        let (bytes_sent, current_chunk_start, current_chunk_end) = self.get_chunk_boundaries(preview);

        // Build display lines with highlighting
        let display_lines = if preview.is_binary {
            self.build_hex_lines(preview, bytes_sent, current_chunk_start, current_chunk_end)
        } else {
            self.build_text_lines(preview, bytes_sent, current_chunk_start, current_chunk_end)
        };

        // Add truncation indicator if needed
        let mut all_lines = display_lines;
        if preview.truncated {
            all_lines.push(Line::from(Span::styled(
                format!("... [truncated - showing {} of {}]", 
                    format_bytes(preview.raw_bytes.len() as u64),
                    format_bytes(preview.size)),
                Style::default().fg(Theme::WARNING).bold()
            )));
        }

        let total_lines = all_lines.len();
        let max_scroll = total_lines.saturating_sub(visible_height);
        let scroll = self.scroll.min(max_scroll);

        // Render visible lines
        let visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .collect();

        // Render content
        for (i, line) in visible_lines.iter().enumerate() {
            if i < visible_height {
                let y = inner_area.y + i as u16;
                buf.set_line(inner_area.x, y, line, inner_area.width);
            }
        }

        // Render scrollbar in the right border (like traffic view)
        if total_lines > visible_height {
            let scrollbar_area = Rect::new(
                outer_area.x + outer_area.width - 1,
                outer_area.y + 1,
                1,
                outer_area.height.saturating_sub(2),
            );
            let mut scrollbar_state = ScrollbarState::new(max_scroll.max(1))
                .position(scroll);
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }

    /// Get the byte boundaries for highlighting based on progress.
    /// Returns (bytes_sent, current_chunk_start, current_chunk_end)
    fn get_chunk_boundaries(&self, preview: &FilePreview) -> (u64, u64, u64) {
        let Some(progress) = &self.progress else {
            return (0, 0, 0);
        };

        if !self.is_sending() && progress.complete {
            // Sending complete - all data is "sent"
            return (preview.raw_bytes.len() as u64, 0, 0);
        }

        let bytes_sent = progress.bytes_sent;
        
        // Current chunk starts where we've sent up to
        let current_chunk_start = bytes_sent;
        
        // Calculate where the current chunk ends based on chunking mode
        let current_chunk_end = if self.config.chunk_mode_index == 1 {
            // Bytes mode - fixed chunk size
            let chunk_size = SizeUnit::from_index(self.config.byte_unit_index)
                .to_bytes(self.config.byte_chunk_value) as u64;
            (bytes_sent + chunk_size).min(preview.size)
        } else {
            // Delimiter mode - find the next delimiter in the preview data
            let delimiter = Delimiter::from_index(self.config.delimiter_index);
            let delimiter_bytes = delimiter.as_bytes();
            
            let start_idx = bytes_sent as usize;
            if start_idx >= preview.raw_bytes.len() {
                preview.size
            } else {
                // Search for delimiter starting from bytes_sent
                let search_slice = &preview.raw_bytes[start_idx..];
                if let Some(pos) = find_delimiter(search_slice, delimiter_bytes) {
                    let end = start_idx + pos;
                    // Include delimiter if configured
                    if self.config.include_delimiter {
                        (end + delimiter_bytes.len()).min(preview.raw_bytes.len()) as u64
                    } else {
                        end as u64
                    }
                } else {
                    // No delimiter found - chunk extends to end of file
                    preview.size
                }
            }
        };

        (bytes_sent, current_chunk_start, current_chunk_end)
    }

    /// Build display lines for text content with highlighting.
    fn build_text_lines(
        &self,
        preview: &FilePreview,
        bytes_sent: u64,
        current_chunk_start: u64,
        current_chunk_end: u64,
    ) -> Vec<Line<'static>> {
        let text = String::from_utf8_lossy(&preview.raw_bytes);
        let mut lines = Vec::new();
        let mut byte_offset: u64 = 0;

        for line_text in text.lines() {
            let line_start = byte_offset;
            let line_end = byte_offset + line_text.len() as u64;

            let mut spans = Vec::new();
            let mut char_offset = line_start;

            for ch in line_text.chars() {
                let ch_len = ch.len_utf8() as u64;
                let style = if self.is_sending() || self.progress.as_ref().is_some_and(|p| p.bytes_sent > 0) {
                    if char_offset < bytes_sent {
                        // Already sent - green foreground
                        Style::default().fg(SENT_COLOR)
                    } else if char_offset >= current_chunk_start && char_offset < current_chunk_end && self.is_sending() {
                        // Current chunk - yellow foreground
                        Style::default().fg(CURRENT_COLOR)
                    } else {
                        // Not yet sent
                        Theme::muted()
                    }
                } else {
                    Theme::muted()
                };

                spans.push(Span::styled(ch.to_string(), style));
                char_offset += ch_len;
            }

            lines.push(Line::from(spans));
            byte_offset = line_end + 1; // +1 for newline
        }

        lines
    }

    /// Build display lines for hex (binary) content with highlighting.
    fn build_hex_lines(
        &self,
        preview: &FilePreview,
        bytes_sent: u64,
        current_chunk_start: u64,
        current_chunk_end: u64,
    ) -> Vec<Line<'static>> {
        let bytes_per_line = 16;
        let mut lines = Vec::new();

        for (line_idx, chunk) in preview.raw_bytes.chunks(bytes_per_line).enumerate() {
            let line_start = (line_idx * bytes_per_line) as u64;
            let mut spans = Vec::new();

            // Address prefix
            spans.push(Span::styled(
                format!("{:08X}  ", line_start),
                Theme::muted()
            ));

            // Hex bytes
            for (i, &byte) in chunk.iter().enumerate() {
                let byte_offset = line_start + i as u64;
                let style = if self.is_sending() || self.progress.as_ref().is_some_and(|p| p.bytes_sent > 0) {
                    if byte_offset < bytes_sent {
                        Style::default().fg(SENT_COLOR)
                    } else if byte_offset >= current_chunk_start && byte_offset < current_chunk_end && self.is_sending() {
                        Style::default().fg(CURRENT_COLOR)
                    } else {
                        Theme::muted()
                    }
                } else {
                    Theme::muted()
                };

                let separator = if i == 7 { "  " } else { " " };
                spans.push(Span::styled(format!("{:02X}{}", byte, separator), style));
            }

            // Pad if line is short
            let missing = bytes_per_line - chunk.len();
            if missing > 0 {
                let padding = "   ".repeat(missing) + if chunk.len() <= 7 { "  " } else { "" };
                spans.push(Span::styled(padding, Theme::muted()));
            }

            // ASCII representation
            spans.push(Span::styled(" |", Theme::muted()));
            for (i, &byte) in chunk.iter().enumerate() {
                let byte_offset = line_start + i as u64;
                let ch = if byte.is_ascii_graphic() || byte == b' ' {
                    byte as char
                } else {
                    '.'
                };
                let style = if self.is_sending() || self.progress.as_ref().is_some_and(|p| p.bytes_sent > 0) {
                    if byte_offset < bytes_sent {
                        Style::default().fg(SENT_COLOR)
                    } else if byte_offset >= current_chunk_start && byte_offset < current_chunk_end && self.is_sending() {
                        Style::default().fg(CURRENT_COLOR)
                    } else {
                        Theme::muted()
                    }
                } else {
                    Theme::muted()
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            spans.push(Span::styled("|", Theme::muted()));

            lines.push(Line::from(spans));
        }

        lines
    }

    /// Calculate the total number of display lines for scroll calculations.
    fn total_display_lines(&self) -> usize {
        let Some(preview) = &self.preview else {
            return 0;
        };

        let base_lines = if preview.is_binary {
            // Hex view: 16 bytes per line
            preview.raw_bytes.len().div_ceil(16)
        } else {
            // Text view: count actual lines
            let text = String::from_utf8_lossy(&preview.raw_bytes);
            text.lines().count()
        };

        // Add 1 for truncation indicator if needed
        if preview.truncated {
            base_lines + 1
        } else {
            base_lines
        }
    }

    /// Calculate the display line for a given byte offset (for auto-follow).
    fn byte_offset_to_line(&self, byte_offset: u64) -> usize {
        let Some(preview) = &self.preview else {
            return 0;
        };

        if preview.is_binary {
            // Hex view: 16 bytes per line
            (byte_offset as usize) / 16
        } else {
            // Text view: count newlines up to offset
            let text = String::from_utf8_lossy(&preview.raw_bytes);
            let mut line = 0;
            let mut current_offset = 0u64;
            for line_text in text.lines() {
                let line_end = current_offset + line_text.len() as u64 + 1;
                if byte_offset < line_end {
                    return line;
                }
                line += 1;
                current_offset = line_end;
            }
            line
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        let has_ctrl = key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL);
        let half_page = self.last_visible_height / 2;
        let total_lines = self.total_display_lines();
        let max_scroll = total_lines.saturating_sub(self.last_visible_height);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.scroll = self.scroll.saturating_add(1).min(max_scroll);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Char('d') if has_ctrl => {
                self.scroll = self.scroll.saturating_add(half_page).min(max_scroll);
            }
            KeyCode::Char('u') if has_ctrl => {
                self.scroll = self.scroll.saturating_sub(half_page);
            }
            KeyCode::Char('g') => {
                self.scroll = 0;
            }
            KeyCode::Char('G') => {
                self.scroll = max_scroll;
            }
            KeyCode::Enter => {
                if self.selected_path.is_some() && !self.is_sending() {
                    return Some(FileSenderAction::StartSending);
                }
            }
            KeyCode::Char('x') => {
                if self.is_sending() {
                    return Some(FileSenderAction::CancelSending);
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        // Keys that can trigger text input activation: Enter, Space
        let is_toggle_key = matches!(
            key.code,
            KeyCode::Enter | KeyCode::Char(' ')
        );

        if is_toggle_key && !self.config_nav.edit_mode.is_dropdown()
            && let Some(field) = self.config_nav.current_field(FILE_SENDER_CONFIG_SECTIONS, &self.config)
        {
            // Handle text input field (file path)
            if field.kind.is_text_input() && field.id == "file_path" {
                self.file_path_input.set_content(&self.config.file_path);
                self.file_path_focused = true;
                return None;
            }
        }

        // Track send_active toggle state before key handling
        let was_sending = self.config.is_sending;

        let _ = handle_config_key(
            key,
            &mut self.config_nav,
            FILE_SENDER_CONFIG_SECTIONS,
            &mut self.config,
        );

        // Check if send_active toggle changed
        if self.config.is_sending != was_sending {
            if self.config.is_sending {
                // Validate file exists before starting
                if self.selected_path.is_none() {
                    self.config.is_sending = false; // Revert
                    return Some(FileSenderAction::Toast(Toast::error("No file selected")));
                }
                return Some(FileSenderAction::StartSending);
            } else {
                return Some(FileSenderAction::CancelSending);
            }
        }

        None
    }

    fn handle_file_path_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        match key.code {
            KeyCode::Enter => {
                // If completion is visible, apply the selected completion
                if self.file_path_completion.visible {
                    self.apply_file_path_completion();
                    self.file_path_completion.hide();
                    return None;
                }
                // Otherwise, confirm the path
                let path_str = self.file_path_input.content().to_string();
                if !path_str.is_empty() {
                    let path = PathBuf::from(&path_str);
                    if path.exists() && path.is_file() {
                        self.config.file_path = path_str;
                        self.load_preview(&path);
                        self.selected_path = Some(path);
                    } else {
                        return Some(FileSenderAction::Toast(Toast::error(format!(
                            "File not found: {}",
                            path_str
                        ))));
                    }
                }
                self.file_path_focused = false;
            }
            KeyCode::Esc => {
                if self.file_path_completion.visible {
                    self.file_path_completion.hide();
                } else {
                    self.file_path_focused = false;
                    self.file_path_input.clear();
                }
            }
            KeyCode::Tab => {
                if !self.file_path_completion.visible {
                    self.update_file_path_completions();
                } else {
                    self.file_path_completion.next();
                }
                self.apply_file_path_completion();
            }
            KeyCode::BackTab => {
                if self.file_path_completion.visible {
                    self.file_path_completion.prev();
                    self.apply_file_path_completion();
                }
            }
            _ => {
                self.file_path_input.handle_key(key);
                self.file_path_completion.hide();
            }
        }
        None
    }

    fn update_file_path_completions(&mut self) {
        let input = self.file_path_input.content();
        let completions = find_path_completions(input);
        self.file_path_completion.show(completions, CompletionKind::Argument);
    }

    fn apply_file_path_completion(&mut self) {
        if let Some(value) = self.file_path_completion.selected_value() {
            self.file_path_input.set_content(value.to_string());
        }
    }

    fn load_preview(&mut self, path: &PathBuf) {
        if let Ok(metadata) = std::fs::metadata(path) {
            let size = metadata.len();

            // Calculate preview limit in bytes
            let limit_bytes = match self.config.preview_limit_unit_index {
                0 => self.config.preview_limit_value * 1024,        // KB
                _ => self.config.preview_limit_value * 1024 * 1024, // MB
            };

            let read_size = limit_bytes.min(size as usize);
            let truncated = size as usize > limit_bytes;

            if let Ok(mut file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut raw_bytes = vec![0u8; read_size];
                if file.read_exact(&mut raw_bytes).is_err() {
                    // If exact read fails, try reading what we can
                    raw_bytes = std::fs::read(path).unwrap_or_default();
                    raw_bytes.truncate(limit_bytes);
                }

                // Check if binary (null bytes or control chars except newline/tab/cr)
                let is_binary = raw_bytes.iter().any(|&b| b == 0 || (b < 32 && b != b'\n' && b != b'\r' && b != b'\t'));

                let line_count = if is_binary {
                    None
                } else {
                    // Count lines in the loaded portion
                    let text = String::from_utf8_lossy(&raw_bytes);
                    Some(text.lines().count())
                };

                self.preview = Some(FilePreview {
                    size,
                    raw_bytes,
                    is_binary,
                    line_count,
                    truncated,
                });
                
                // Reset scroll when loading new file
                self.scroll = 0;
            }
        }
    }

    pub async fn start_sending(&mut self, handle: &SessionHandle) -> Result<(), serial_core::Error> {
        if let Some(path) = &self.selected_path {
            // Build chunk mode
            let chunk_mode = if self.config.chunk_mode_index == 0 {
                // Delimiter mode
                ChunkMode::Delimiter(Delimiter::from_index(self.config.delimiter_index))
            } else {
                // Bytes mode
                let bytes = SizeUnit::from_index(self.config.byte_unit_index)
                    .to_bytes(self.config.byte_chunk_value);
                ChunkMode::Bytes(bytes.max(1))
            };

            // Build chunk suffix
            let chunk_suffix = if self.config.append_suffix {
                let delimiter = Delimiter::from_index(self.config.suffix_delimiter_index);
                Some(Cow::Borrowed(delimiter.as_bytes()))
            } else {
                None
            };

            // Build delay
            let chunk_delay = TimeUnit::from_index(self.config.delay_unit_index)
                .to_duration(self.config.delay_value as u64);

            let config = FileSendConfig {
                chunk_mode,
                include_delimiter: self.config.include_delimiter,
                units_per_chunk: self.config.lines_per_chunk,
                chunk_suffix,
                chunk_delay,
                repeat: self.config.repeat,
            };

            let send_handle = send_file(handle, path, config).await?;
            self.send_handle = Some(send_handle);
            self.config.is_sending = true;
            self.progress = None;
        }
        Ok(())
    }

    pub fn cancel_sending(&mut self) {
        if let Some(handle) = self.send_handle.take() {
            // Spawn task to cancel - we don't need to wait for it
            tokio::spawn(async move {
                handle.cancel().await;
            });
        }
        self.config.is_sending = false;
        self.progress = None;
    }
}

impl Default for FileSenderView {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Settings persistence
// =============================================================================

use crate::settings::FileSenderSettings;

impl FileSenderView {
    /// Apply saved settings to this view.
    pub fn apply_settings(&mut self, settings: &FileSenderSettings) {
        self.config.chunk_mode_index = settings.chunk_mode_index;
        self.config.delimiter_index = settings.delimiter_index;
        self.config.include_delimiter = settings.include_delimiter;
        self.config.lines_per_chunk = settings.lines_per_chunk;
        self.config.byte_chunk_value = settings.byte_chunk_value;
        self.config.byte_unit_index = settings.byte_unit_index;
        self.config.append_suffix = settings.append_suffix;
        self.config.suffix_delimiter_index = settings.suffix_delimiter_index;
        self.config.delay_value = settings.delay_value;
        self.config.delay_unit_index = settings.delay_unit_index;
        self.config.repeat = settings.repeat;
        self.config.preview_limit_value = settings.preview_limit_value;
        self.config.preview_limit_unit_index = settings.preview_limit_unit_index;
        self.config.auto_follow = settings.auto_follow;
    }

    /// Extract current settings from this view.
    pub fn to_settings(&self) -> FileSenderSettings {
        FileSenderSettings {
            chunk_mode_index: self.config.chunk_mode_index,
            delimiter_index: self.config.delimiter_index,
            include_delimiter: self.config.include_delimiter,
            lines_per_chunk: self.config.lines_per_chunk,
            byte_chunk_value: self.config.byte_chunk_value,
            byte_unit_index: self.config.byte_unit_index,
            append_suffix: self.config.append_suffix,
            suffix_delimiter_index: self.config.suffix_delimiter_index,
            delay_value: self.config.delay_value,
            delay_unit_index: self.config.delay_unit_index,
            repeat: self.config.repeat,
            preview_limit_value: self.config.preview_limit_value,
            preview_limit_unit_index: self.config.preview_limit_unit_index,
            auto_follow: self.config.auto_follow,
        }
    }
}

/// Find the position of a delimiter in a byte slice.
fn find_delimiter(data: &[u8], delimiter: &[u8]) -> Option<usize> {
    data.windows(delimiter.len()).position(|w| w == delimiter)
}
