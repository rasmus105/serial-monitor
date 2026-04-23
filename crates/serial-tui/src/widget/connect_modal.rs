//! Connect modal widget for configuring serial port connection.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use serial_core::{
    ChunkingStrategy, DataBits, LineDelimiter, SerialConfig,
    ui::{
        config::{
            ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_enabled, always_valid,
            always_visible,
        },
        serial_config::{
            COMMON_BAUD_RATES, DATA_BITS_VARIANTS, FLOW_CONTROL_VARIANTS, PARITY_VARIANTS,
            STOP_BITS_VARIANTS,
        },
    },
};

use crate::{theme::Theme, widget::ConfigPanel};

/// Action returned from connect modal key handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectModalAction {
    /// No action taken.
    None,
    /// User cancelled the modal (pressed Esc).
    Cancel,
    /// User confirmed connection (pressed Enter).
    Connect,
}

/// Serial configuration for the connect modal.
#[derive(Debug, Clone)]
pub struct ConnectModalConfig {
    pub baud_rate_index: usize,
    pub data_bits_index: usize,
    pub parity_index: usize,
    pub stop_bits_index: usize,
    pub flow_control_index: usize,
    // Data handling settings
    pub line_ending_index: usize,
    // File saving settings
    pub file_save_enabled: bool,
    pub file_save_format_index: usize,
    pub file_save_encoding_index: usize,
    pub file_save_directory: String,
}

impl Default for ConnectModalConfig {
    fn default() -> Self {
        Self {
            // Default to 115200
            baud_rate_index: COMMON_BAUD_RATES
                .iter()
                .position(|&r| r == 115200)
                .unwrap_or(8),
            // Default to 8 data bits
            data_bits_index: DATA_BITS_VARIANTS
                .iter()
                .position(|&d| d == DataBits::Eight)
                .unwrap_or(3),
            // Default to no parity
            parity_index: 0,
            // Default to 1 stop bit
            stop_bits_index: 0,
            // Default to no flow control
            flow_control_index: 0,
            // Default to LF line endings (same as config panel)
            line_ending_index: 1, // LF
            // File saving defaults
            file_save_enabled: false,
            file_save_format_index: 1,   // Encoded
            file_save_encoding_index: 0, // UTF-8
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

impl ConnectModalConfig {
    /// Convert to SerialConfig for the core.
    pub fn to_serial_config(&self) -> SerialConfig {
        SerialConfig {
            baud_rate: COMMON_BAUD_RATES[self.baud_rate_index],
            data_bits: DATA_BITS_VARIANTS[self.data_bits_index],
            parity: PARITY_VARIANTS[self.parity_index],
            stop_bits: STOP_BITS_VARIANTS[self.stop_bits_index],
            flow_control: FLOW_CONTROL_VARIANTS[self.flow_control_index],
        }
    }

    /// Get the RX chunking strategy based on line ending selection.
    pub fn rx_chunking(&self) -> ChunkingStrategy {
        match self.line_ending_index {
            0 => ChunkingStrategy::Raw, // None (Raw)
            1 => ChunkingStrategy::with_delimiter(LineDelimiter::Newline), // LF
            2 => ChunkingStrategy::with_delimiter(LineDelimiter::Cr), // CR
            3 => ChunkingStrategy::with_delimiter(LineDelimiter::CrLf), // CRLF
            _ => ChunkingStrategy::Raw,
        }
    }
}

// Config panel field definitions
const BAUD_RATE_OPTIONS: &[&str] = &[
    "300", "1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "230400", "460800",
    "921600",
];
const DATA_BITS_OPTIONS: &[&str] = &["5", "6", "7", "8"];
const PARITY_OPTIONS: &[&str] = &["None", "Odd", "Even"];
const STOP_BITS_OPTIONS: &[&str] = &["1", "2"];
const FLOW_CONTROL_OPTIONS: &[&str] = &["None", "Software (XON/XOFF)", "Hardware (RTS/CTS)"];

// Data handling options
const RX_CHUNKING_OPTIONS: &[&str] = &["None (Raw)", "LF (\\n)", "CR (\\r)", "CRLF (\\r\\n)"];

// File saving options
const FILE_SAVE_FORMAT_OPTIONS: &[&str] = &["Raw Binary", "Encoded Text"];
const FILE_SAVE_ENCODING_OPTIONS: &[&str] = &["UTF-8", "ASCII", "Hex", "Binary"];

static CONNECT_MODAL_SECTIONS: &[Section<ConnectModalConfig>] = &[
    Section {
        header: Some("Serial Settings"),
        fields: &[
            FieldDef {
                id: "baud_rate",
                label: "Baud Rate",
                kind: FieldKind::Select {
                    options: BAUD_RATE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.baud_rate_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.baud_rate_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "data_bits",
                label: "Data Bits",
                kind: FieldKind::Select {
                    options: DATA_BITS_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.data_bits_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.data_bits_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "parity",
                label: "Parity",
                kind: FieldKind::Select {
                    options: PARITY_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.parity_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.parity_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "stop_bits",
                label: "Stop Bits",
                kind: FieldKind::Select {
                    options: STOP_BITS_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.stop_bits_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.stop_bits_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "flow_control",
                label: "Flow Control",
                kind: FieldKind::Select {
                    options: FLOW_CONTROL_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.flow_control_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.flow_control_index = i;
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
        header: Some("Data Handling"),
        fields: &[FieldDef {
            id: "rx_chunking",
            label: "RX Chunking",
            kind: FieldKind::Select {
                options: RX_CHUNKING_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.line_ending_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.line_ending_index = i;
                }
            },
            visible: always_visible,
            enabled: always_enabled,
            parent_id: None,
            validate: always_valid,
        }],
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
                enabled: |c| c.file_save_enabled,
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
                enabled: |c| c.file_save_enabled,
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
                enabled: |c| c.file_save_enabled,
                parent_id: Some("file_save_enabled"),
                validate: always_valid,
            },
        ],
    },
];

/// State for the connect modal.
#[derive(Debug)]
pub struct ConnectModalState {
    /// Whether the modal is visible.
    pub visible: bool,
    /// The port path to connect to.
    pub port_path: String,
    /// Serial configuration.
    pub config: ConnectModalConfig,
    /// Config panel navigation.
    pub nav: ConfigNav,
}

impl Default for ConnectModalState {
    fn default() -> Self {
        Self {
            visible: false,
            port_path: String::new(),
            config: ConnectModalConfig::default(),
            nav: ConfigNav::new(),
        }
    }
}

impl ConnectModalState {
    /// Show the modal with a port pre-filled.
    pub fn show(&mut self, port_path: String) {
        self.visible = true;
        self.port_path = port_path;
        self.config = ConnectModalConfig::default();
        self.nav = ConfigNav::new();
    }

    /// Hide the modal.
    pub fn hide(&mut self) {
        self.visible = false;
        self.port_path.clear();
    }

    /// Handle key input, returning the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> ConnectModalAction {
        if !self.visible {
            return ConnectModalAction::None;
        }

        // Handle text editing mode
        if self.nav.edit_mode.is_text_input() {
            return self.handle_text_edit_key(key);
        }

        // Handle dropdown mode separately
        if self.nav.edit_mode.is_dropdown() {
            return self.handle_dropdown_key(key);
        }

        // Ignore j/k with CTRL modifier
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => {
                self.hide();
                ConnectModalAction::Cancel
            }
            // Ctrl+g ("go") to connect
            // Note: Don't call hide() here - let caller extract port_path first
            KeyCode::Char('g') if has_ctrl => ConnectModalAction::Connect,
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Open dropdown for select fields, toggle for toggles, text edit for text inputs
                if let Some(field) = self.nav.current_field(CONNECT_MODAL_SECTIONS, &self.config) {
                    if field.kind.is_select() {
                        self.nav.open_dropdown(CONNECT_MODAL_SECTIONS, &self.config);
                    } else if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .nav
                            .toggle_current(CONNECT_MODAL_SECTIONS, &mut self.config);
                    } else if field.kind.is_editable() {
                        self.nav
                            .start_text_edit(CONNECT_MODAL_SECTIONS, &self.config);
                    }
                }
                ConnectModalAction::None
            }
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.nav.next_field(CONNECT_MODAL_SECTIONS, &self.config);
                ConnectModalAction::None
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.nav.prev_field(CONNECT_MODAL_SECTIONS, &self.config);
                ConnectModalAction::None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(field) = self.nav.current_field(CONNECT_MODAL_SECTIONS, &self.config) {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .nav
                            .toggle_current(CONNECT_MODAL_SECTIONS, &mut self.config);
                    } else if field.kind.is_select() {
                        self.nav.dropdown_prev(CONNECT_MODAL_SECTIONS, &self.config);
                        let _ = self
                            .nav
                            .apply_dropdown(CONNECT_MODAL_SECTIONS, &mut self.config);
                    }
                }
                ConnectModalAction::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(field) = self.nav.current_field(CONNECT_MODAL_SECTIONS, &self.config) {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .nav
                            .toggle_current(CONNECT_MODAL_SECTIONS, &mut self.config);
                    } else if field.kind.is_select() {
                        self.nav.dropdown_next(CONNECT_MODAL_SECTIONS, &self.config);
                        let _ = self
                            .nav
                            .apply_dropdown(CONNECT_MODAL_SECTIONS, &mut self.config);
                    }
                }
                ConnectModalAction::None
            }
            _ => ConnectModalAction::None,
        }
    }

    fn handle_text_edit_key(&mut self, key: KeyEvent) -> ConnectModalAction {
        match key.code {
            KeyCode::Enter => {
                let _ = self
                    .nav
                    .apply_text_edit(CONNECT_MODAL_SECTIONS, &mut self.config);
                ConnectModalAction::None
            }
            KeyCode::Esc => {
                self.nav.cancel_text_edit();
                ConnectModalAction::None
            }
            KeyCode::Char(c) => {
                // Handle Ctrl+<key> sequences
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if let Some(buf) = self.nav.edit_mode.text_buffer_mut() {
                        match c {
                            'u' => buf.delete_to_start(),
                            'w' => buf.delete_word_before(),
                            'a' => buf.move_start(),
                            'e' => buf.move_end(),
                            'k' => buf.delete_to_end(),
                            _ => {}
                        }
                    }
                } else if let Some(buf) = self.nav.edit_mode.text_buffer_mut() {
                    buf.insert_char(c);
                }
                ConnectModalAction::None
            }
            KeyCode::Backspace => {
                if let Some(buf) = self.nav.edit_mode.text_buffer_mut() {
                    buf.delete_char_before();
                }
                ConnectModalAction::None
            }
            _ => ConnectModalAction::None,
        }
    }

    fn handle_dropdown_key(&mut self, key: KeyEvent) -> ConnectModalAction {
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.nav.dropdown_next(CONNECT_MODAL_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.nav.dropdown_prev(CONNECT_MODAL_SECTIONS, &self.config);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self
                    .nav
                    .apply_dropdown(CONNECT_MODAL_SECTIONS, &mut self.config);
                self.nav.close_dropdown();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.nav.close_dropdown();
            }
            _ => {}
        }
        ConnectModalAction::None
    }
}

/// Connect modal widget.
pub struct ConnectModal<'a> {
    state: &'a ConnectModalState,
}

impl<'a> ConnectModal<'a> {
    pub fn new(state: &'a ConnectModalState) -> Self {
        Self { state }
    }
}

impl Widget for ConnectModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate overlay area (centered, reasonable size for serial config)
        let width = (area.width * 60 / 100).clamp(40, 55);
        let height = (area.height * 80 / 100).clamp(22, 30);
        let x = area.x + (area.width - width) / 2;
        let y = area.y + (area.height - height) / 2;
        let overlay_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(overlay_area, buf);

        // Outer block with port name in title
        let title = format!(" Connect to {} ", self.state.port_path);
        let block = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Theme::border_disconnected());

        let inner = block.inner(overlay_area);
        block.render(overlay_area, buf);

        if inner.height < 4 {
            return;
        }

        // Reserve space for footer hints
        let footer_height = 2u16;
        let content_height = inner.height.saturating_sub(footer_height);
        let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
        let footer_area = Rect::new(
            inner.x,
            inner.y + content_height,
            inner.width,
            footer_height,
        );

        // Render config panel
        ConfigPanel::new(CONNECT_MODAL_SECTIONS, &self.state.config, &self.state.nav)
            .focused(true)
            .disconnected(true)
            .render(content_area, buf);

        // Render footer hints
        if footer_area.height >= 1 {
            let footer_line = Line::from(vec![
                Span::styled("[Ctrl+g]", Theme::keybind_disconnected()),
                Span::raw(" Connect  "),
                Span::styled("[Esc]", Theme::keybind_disconnected()),
                Span::raw(" Cancel"),
            ]);
            Paragraph::new(footer_line)
                .alignment(Alignment::Center)
                .render(
                    Rect::new(
                        footer_area.x,
                        footer_area.y + footer_area.height - 1,
                        footer_area.width,
                        1,
                    ),
                    buf,
                );
        }
    }
}
