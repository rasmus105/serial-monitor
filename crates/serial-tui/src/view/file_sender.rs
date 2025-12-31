//! File sender view: send files with progress tracking.

use std::borrow::Cow;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Gauge, Paragraph, Widget, Wrap},
};
use serial_core::{
    ChunkMode, Delimiter, FileSendConfig, FileSendHandle, FileSendProgress, SerialConfig,
    SessionHandle, SizeUnit, TimeUnit, send_file,
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
    },
};

use crate::{
    app::{FileSenderAction, Focus},
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfigPanel, ConnectionPanel,
        Toast, handle_config_key,
        text_input::{TextInputState, find_path_completions},
    },
};

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
    pub config_nav: ConfigPanelNav,
    /// Active send handle.
    pub send_handle: Option<FileSendHandle>,
    /// Latest progress.
    pub progress: Option<FileSendProgress>,
}

/// Preview of selected file.
#[derive(Debug, Clone)]
pub struct FilePreview {
    pub size: u64,
    pub content: String,
    pub is_binary: bool,
    pub line_count: Option<usize>,
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
}

impl Default for FileSenderConfig {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            chunk_mode_index: 0, // Delimiter
            delimiter_index: 0,  // LF
            include_delimiter: true,
            byte_chunk_value: 64,
            byte_unit_index: 0, // Bytes
            append_suffix: false,
            suffix_delimiter_index: 0, // LF
            delay_value: 10,
            delay_unit_index: 0, // Milliseconds
            repeat: false,
            is_sending: false,
        }
    }
}

const CHUNK_MODE_OPTIONS: &[&str] = &["Delimiter", "Bytes"];

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
                    options: Delimiter::OPTIONS,
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
                    if let FieldValue::Usize(n) = v {
                        if *n == 0 {
                            return Err(Cow::Borrowed("Size must be > 0"));
                        }
                    }
                    Ok(())
                },
            },
            FieldDef {
                id: "byte_unit",
                label: "Unit",
                kind: FieldKind::Select {
                    options: SizeUnit::OPTIONS,
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
                    options: Delimiter::OPTIONS,
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
                    options: TimeUnit::OPTIONS,
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
            file_path_input: TextInputState::new().with_placeholder("Enter file path..."),
            file_path_focused: false,
            file_path_completion: CompletionState::default(),
            selected_path: None,
            preview: None,
            config: FileSenderConfig::default(),
            config_nav: ConfigPanelNav::new(),
            send_handle: None,
            progress: None,
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.file_path_focused
            || self.config_nav.is_text_editing()
            || self.config_nav.is_dropdown_open()
    }

    pub fn is_sending(&self) -> bool {
        self.send_handle.is_some()
    }

    pub fn tick(&mut self) {
        if let Some(ref mut handle) = self.send_handle {
            while let Some(progress) = handle.try_recv_progress() {
                let complete = progress.complete;
                self.progress = Some(progress);
                if complete {
                    self.send_handle = None;
                    self.config.is_sending = false;
                    break;
                }
            }
        }
    }

    pub fn draw(
        &self,
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

            if let Some(lines) = preview.line_count {
                format!(" Preview: {} ({}, {} lines) ", filename, size_str, lines)
            } else {
                format!(" Preview: {} ({}, binary) ", filename, size_str)
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

        if let Some(preview) = &self.preview {
            let content = if preview.is_binary {
                format!("[Binary file - {} bytes]", preview.size)
            } else {
                preview.content.clone()
            };
            Paragraph::new(content)
                .wrap(Wrap { trim: false })
                .style(Theme::muted())
                .render(preview_inner, buf);
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
        let (bytes_sent, total_bytes, chunks_sent, total_chunks, loops, percentage) =
            if let Some(progress) = &self.progress {
                (
                    progress.bytes_sent,
                    progress.total_bytes,
                    progress.chunks_sent,
                    progress.total_chunks,
                    progress.loops_completed,
                    (progress.percentage() * 100.0) as u16,
                )
            } else if let Some(preview) = &self.preview {
                // Preload with file info
                let total = preview.size;
                // For bytes mode, we can estimate chunks. For delimiter mode, we show 0 (unknown).
                let chunks = if self.config.chunk_mode_index == 1 {
                    let chunk_bytes = SizeUnit::from_index(self.config.byte_unit_index)
                        .to_bytes(self.config.byte_chunk_value);
                    (total as usize).div_ceil(chunk_bytes.max(1))
                } else {
                    // Delimiter mode: unknown until we scan
                    0
                };
                (0, total, 0, chunks, 0, 0)
            } else {
                // No file selected
                (0, 0, 0, 0, 0, 0)
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
                format!("Chunks: {} / {}", chunks_sent, total_chunks),
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

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        match key.code {
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

        if is_toggle_key && !self.config_nav.is_dropdown_open() {
            if let Some(field) = self.config_nav.current_field(FILE_SENDER_CONFIG_SECTIONS, &self.config) {
                // Handle text input field (file path)
                if field.kind.is_text_input() && field.id == "file_path" {
                    self.file_path_input.set_content(&self.config.file_path);
                    self.file_path_focused = true;
                    return None;
                }
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
                let path_str = self.file_path_input.content.clone();
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
        let input = &self.file_path_input.content;
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

            // Read first 1KB for preview
            let preview_size = 1024.min(size as usize);
            if let Ok(content) = std::fs::read(path) {
                let preview_bytes = &content[..preview_size.min(content.len())];

                // Check if binary
                let is_binary = preview_bytes.iter().any(|&b| b == 0 || (b < 32 && b != b'\n' && b != b'\r' && b != b'\t'));

                let (content, line_count) = if is_binary {
                    // Show hex dump
                    let hex = preview_bytes
                        .iter()
                        .take(256)
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .chunks(16)
                        .map(|chunk| chunk.join(" "))
                        .collect::<Vec<_>>()
                        .join("\n");
                    (hex, None)
                } else {
                    // Count lines from full file content for accurate count
                    let full_text = String::from_utf8_lossy(&content);
                    let lines = full_text.lines().count();
                    let preview_text = String::from_utf8_lossy(preview_bytes).to_string();
                    (preview_text, Some(lines))
                };

                self.preview = Some(FilePreview {
                    size,
                    content,
                    is_binary,
                    line_count,
                });
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
        self.config.byte_chunk_value = settings.byte_chunk_value;
        self.config.byte_unit_index = settings.byte_unit_index;
        self.config.append_suffix = settings.append_suffix;
        self.config.suffix_delimiter_index = settings.suffix_delimiter_index;
        self.config.delay_value = settings.delay_value;
        self.config.delay_unit_index = settings.delay_unit_index;
        self.config.repeat = settings.repeat;
    }

    /// Extract current settings from this view.
    pub fn to_settings(&self) -> FileSenderSettings {
        FileSenderSettings {
            chunk_mode_index: self.config.chunk_mode_index,
            delimiter_index: self.config.delimiter_index,
            include_delimiter: self.config.include_delimiter,
            byte_chunk_value: self.config.byte_chunk_value,
            byte_unit_index: self.config.byte_unit_index,
            append_suffix: self.config.append_suffix,
            suffix_delimiter_index: self.config.suffix_delimiter_index,
            delay_value: self.config.delay_value,
            delay_unit_index: self.config.delay_unit_index,
            repeat: self.config.repeat,
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}
