//! File sender view: send files with progress tracking.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Widget, Wrap},
};
use serial_core::{
    FileSendConfig, FileSendHandle, FileSendProgress, SerialConfig, SessionHandle, send_file,
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
    },
};

use crate::{
    app::{FileSenderAction, Focus},
    theme::Theme,
    widget::{ConfigPanel, ConnectionPanel, TextInput, Toast, handle_config_key, text_input::TextInputState},
};

/// File sender view state.
pub struct FileSenderView {
    /// File path input.
    pub path_input: TextInputState,
    /// Whether path input is focused.
    pub path_focused: bool,
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
}

/// File sender configuration.
#[derive(Debug, Clone)]
pub struct FileSenderConfig {
    pub chunk_size: usize,
    pub delay_ms: usize,
    pub repeat: bool,
}

impl Default for FileSenderConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64,
            delay_ms: 10,
            repeat: false,
        }
    }
}

const CHUNK_SIZE_OPTIONS: &[&str] = &["16", "32", "64", "128", "256", "512", "1024"];
const DELAY_OPTIONS: &[&str] = &["0", "1", "5", "10", "20", "50", "100", "200"];

static FILE_SENDER_CONFIG_SECTIONS: &[Section<FileSenderConfig>] = &[Section {
    header: Some("Send Options"),
    fields: &[
        FieldDef {
            id: "chunk_size",
            label: "Chunk Size",
            kind: FieldKind::Select {
                options: CHUNK_SIZE_OPTIONS,
            },
            get: |c| {
                let idx = CHUNK_SIZE_OPTIONS
                    .iter()
                    .position(|&s| s.parse::<usize>().ok() == Some(c.chunk_size))
                    .unwrap_or(2);
                FieldValue::OptionIndex(idx)
            },
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    if let Some(&size_str) = CHUNK_SIZE_OPTIONS.get(i) {
                        if let Ok(size) = size_str.parse::<usize>() {
                            c.chunk_size = size;
                        }
                    }
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "delay_ms",
            label: "Delay (ms)",
            kind: FieldKind::Select {
                options: DELAY_OPTIONS,
            },
            get: |c| {
                let idx = DELAY_OPTIONS
                    .iter()
                    .position(|&s| s.parse::<usize>().ok() == Some(c.delay_ms))
                    .unwrap_or(3);
                FieldValue::OptionIndex(idx)
            },
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    if let Some(&delay_str) = DELAY_OPTIONS.get(i) {
                        if let Ok(delay) = delay_str.parse::<usize>() {
                            c.delay_ms = delay;
                        }
                    }
                }
            },
            visible: always_visible,
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
            validate: always_valid,
        },
    ],
}];

impl FileSenderView {
    pub fn new() -> Self {
        Self {
            path_input: TextInputState::new().with_placeholder("/path/to/file"),
            path_focused: false,
            selected_path: None,
            preview: None,
            config: FileSenderConfig::default(),
            config_nav: ConfigPanelNav::new(),
            send_handle: None,
            progress: None,
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.path_focused
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
        // Main layout: file selection + preview + progress
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Path input
                Constraint::Min(10),    // Preview
                Constraint::Length(6),  // Progress/stats
            ])
            .split(main_area);

        // Path input
        let path_block = Block::default()
            .title(" File Path ")
            .borders(Borders::ALL)
            .border_style(if self.path_focused {
                Theme::border_focused()
            } else if focus == Focus::Main {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let mut path_state = self.path_input.clone();
        TextInput::new(&mut path_state)
            .block(path_block)
            .focused(self.path_focused)
            .render(main_chunks[0], buf);

        // Preview
        let preview_block = Block::default()
            .title(format!(
                " Preview {} ",
                self.selected_path
                    .as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                    .unwrap_or_default()
            ))
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let preview_inner = preview_block.inner(main_chunks[1]);
        preview_block.render(main_chunks[1], buf);

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
            Paragraph::new("No file selected. Press 'o' to enter a file path.")
                .style(Theme::muted())
                .render(preview_inner, buf);
        }

        // Progress/stats
        let stats_block = Block::default()
            .title(" Status ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let stats_inner = stats_block.inner(main_chunks[2]);
        stats_block.render(main_chunks[2], buf);

        if let Some(progress) = &self.progress {
            // Progress bar
            let percent = (progress.percentage() * 100.0) as u16;
            let gauge = Gauge::default()
                .percent(percent)
                .gauge_style(Style::default().fg(Theme::PRIMARY));

            let gauge_area = Rect::new(stats_inner.x, stats_inner.y, stats_inner.width, 1);
            gauge.render(gauge_area, buf);

            // Stats
            let stats_lines = vec![
                Line::from(vec![
                    Span::styled("Sent: ", Theme::muted()),
                    Span::raw(format!(
                        "{} / {} bytes",
                        progress.bytes_sent, progress.total_bytes
                    )),
                ]),
                Line::from(vec![
                    Span::styled("Chunks: ", Theme::muted()),
                    Span::raw(format!("{}", progress.chunks_sent)),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Theme::muted()),
                    if progress.complete {
                        Span::styled("Complete", Theme::success())
                    } else if progress.error.is_some() {
                        Span::styled(
                            format!("Error: {}", progress.error.as_ref().unwrap()),
                            Theme::error(),
                        )
                    } else {
                        Span::styled("Sending...", Theme::info())
                    },
                ]),
            ];

            for (i, line) in stats_lines.into_iter().enumerate() {
                if stats_inner.y + 1 + i as u16 >= stats_inner.y + stats_inner.height {
                    break;
                }
                Paragraph::new(line).render(
                    Rect::new(
                        stats_inner.x,
                        stats_inner.y + 1 + i as u16,
                        stats_inner.width,
                        1,
                    ),
                    buf,
                );
            }
        } else {
            let help = if self.selected_path.is_some() {
                "Press Enter to start sending, 'o' to change file"
            } else {
                "Press 'o' to select a file"
            };
            Paragraph::new(help)
                .style(Theme::muted())
                .render(stats_inner, buf);
        }

        // Config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, handle, serial_config, focus);
        }
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
        if self.path_focused {
            return self.handle_path_key(key);
        }

        match focus {
            Focus::Main => self.handle_main_key(key),
            Focus::Config => self.handle_config_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        match key.code {
            KeyCode::Char('o') => {
                self.path_focused = true;
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
        let _ = handle_config_key(
            key,
            &mut self.config_nav,
            FILE_SENDER_CONFIG_SECTIONS,
            &mut self.config,
        );
        // File sender doesn't need to sync to buffer or request clear
        None
    }

    fn handle_path_key(&mut self, key: KeyEvent) -> Option<FileSenderAction> {
        match key.code {
            KeyCode::Enter => {
                let path_str = self.path_input.take();
                if !path_str.is_empty() {
                    let path = PathBuf::from(&path_str);
                    if path.exists() && path.is_file() {
                        self.load_preview(&path);
                        self.selected_path = Some(path);
                    } else {
                        return Some(FileSenderAction::Toast(Toast::error(format!(
                            "File not found: {}",
                            path_str
                        ))));
                    }
                }
                self.path_focused = false;
            }
            KeyCode::Esc => {
                self.path_focused = false;
                self.path_input.clear();
            }
            _ => {
                self.path_input.handle_key(key);
            }
        }
        None
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

                let content = if is_binary {
                    // Show hex dump
                    preview_bytes
                        .iter()
                        .take(256)
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .chunks(16)
                        .map(|chunk| chunk.join(" "))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::from_utf8_lossy(preview_bytes).to_string()
                };

                self.preview = Some(FilePreview {
                    size,
                    content,
                    is_binary,
                });
            }
        }
    }

    pub async fn start_sending(&mut self, handle: &SessionHandle) -> Result<(), serial_core::Error> {
        if let Some(path) = &self.selected_path {
            let config = FileSendConfig {
                chunk_size: self.config.chunk_size,
                chunk_delay: Duration::from_millis(self.config.delay_ms as u64),
                repeat: self.config.repeat,
            };

            let send_handle = send_file(handle, path, config).await?;
            self.send_handle = Some(send_handle);
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
        self.progress = None;
    }
}

impl Default for FileSenderView {
    fn default() -> Self {
        Self::new()
    }
}
