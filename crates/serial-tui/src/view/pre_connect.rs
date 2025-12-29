//! Pre-connection view: port selection and configuration.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use serial_core::{
    ChunkingStrategy, DataBits, LineDelimiter, SerialConfig, SessionConfig, list_ports,
    buffer::AutoSaveConfig,
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
        serial_config::{
            COMMON_BAUD_RATES, DATA_BITS_VARIANTS, FLOW_CONTROL_VARIANTS, PARITY_VARIANTS,
            STOP_BITS_VARIANTS,
        },
    },
};

use crate::{
    app::{Focus, PreConnectAction},
    theme::Theme,
    widget::{
        ConfigPanel, PortList, TextInput, Toast,
        port_list::PortListState,
        text_input::TextInputState,
    },
};

/// Pre-connection view state.
pub struct PreConnectView {
    /// Port list state.
    pub port_list: PortListState,
    /// Serial configuration.
    pub config: PreConnectConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
    /// Last visible height for port list (for half-page scroll).
    last_visible_height: usize,
}

/// Configuration state for pre-connection.
#[derive(Debug, Clone)]
pub struct PreConnectConfig {
    pub baud_rate_index: usize,
    pub data_bits_index: usize,
    pub parity_index: usize,
    pub stop_bits_index: usize,
    pub flow_control_index: usize,
    // Session settings
    pub line_ending_index: usize,
    pub buffer_size_index: usize,
    // Auto-save settings
    pub auto_save: bool,
    pub auto_save_tx: bool,
    pub auto_save_rx: bool,
    pub auto_save_timestamps: bool,
    pub auto_save_direction: bool,
}

impl Default for PreConnectConfig {
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
            // Default to LF line endings
            line_ending_index: 1, // LF
            // Default to 10MB buffer
            buffer_size_index: 2, // 10 MB
            // Default auto-save settings
            auto_save: true,
            auto_save_tx: false, // TX not saved by default
            auto_save_rx: true,  // RX saved by default
            auto_save_timestamps: true,
            auto_save_direction: false,
        }
    }
}

impl PreConnectConfig {
    pub fn to_serial_config(&self) -> SerialConfig {
        SerialConfig {
            baud_rate: COMMON_BAUD_RATES[self.baud_rate_index],
            data_bits: DATA_BITS_VARIANTS[self.data_bits_index],
            parity: PARITY_VARIANTS[self.parity_index],
            stop_bits: STOP_BITS_VARIANTS[self.stop_bits_index],
            flow_control: FLOW_CONTROL_VARIANTS[self.flow_control_index],
        }
    }

    pub fn to_session_config(&self) -> SessionConfig {
        use serial_core::buffer::{DirectionFilter, SaveFormat, Encoding};
        
        // Map line ending index to chunking strategy
        let rx_chunking = match self.line_ending_index {
            0 => ChunkingStrategy::Raw, // None (Raw)
            1 => ChunkingStrategy::with_delimiter(LineDelimiter::Newline), // LF
            2 => ChunkingStrategy::with_delimiter(LineDelimiter::Cr), // CR
            3 => ChunkingStrategy::with_delimiter(LineDelimiter::CrLf), // CRLF
            _ => ChunkingStrategy::Raw,
        };

        // Get buffer size
        let buffer_size = BUFFER_SIZES
            .get(self.buffer_size_index)
            .copied()
            .flatten();

        // Build auto-save config
        let auto_save = AutoSaveConfig {
            enabled: self.auto_save,
            directions: DirectionFilter {
                tx: self.auto_save_tx,
                rx: self.auto_save_rx,
            },
            format: SaveFormat::Encoded {
                encoding: Encoding::Ascii,
                include_timestamps: self.auto_save_timestamps,
                include_direction: self.auto_save_direction,
            },
            ..Default::default()
        };

        // Build session config
        let mut config = SessionConfig {
            rx_chunking,
            tx_chunking: ChunkingStrategy::Raw, // TX is always raw
            buffer_size,
            auto_save,
        };

        // If buffer size is set, apply it
        if let Some(size) = buffer_size {
            config = config.with_buffer_size(size);
        }

        config
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

// Session settings options
const LINE_ENDING_OPTIONS: &[&str] = &["None (Raw)", "LF (\\n)", "CR (\\r)", "CRLF (\\r\\n)"];
const BUFFER_SIZE_OPTIONS: &[&str] = &["1 MB", "5 MB", "10 MB", "50 MB", "100 MB", "Unlimited"];

/// Buffer sizes in bytes corresponding to BUFFER_SIZE_OPTIONS
const BUFFER_SIZES: &[Option<usize>] = &[
    Some(1 * 1024 * 1024),
    Some(5 * 1024 * 1024),
    Some(10 * 1024 * 1024),
    Some(50 * 1024 * 1024),
    Some(100 * 1024 * 1024),
    None, // Unlimited
];

static PRECONNECT_CONFIG_SECTIONS: &[Section<PreConnectConfig>] = &[
    Section {
        header: Some("Serial Port"),
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
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Data Handling"),
        fields: &[
            FieldDef {
                id: "line_ending",
                label: "Line Ending",
                kind: FieldKind::Select {
                    options: LINE_ENDING_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.line_ending_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.line_ending_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "buffer_size",
                label: "Buffer Size",
                kind: FieldKind::Select {
                    options: BUFFER_SIZE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.buffer_size_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.buffer_size_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Auto-Save"),
        fields: &[
            FieldDef {
                id: "auto_save",
                label: "Enabled",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_rx",
                label: "Save RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_rx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_rx = b;
                    }
                },
                visible: |c| c.auto_save,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_tx",
                label: "Save TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_tx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_tx = b;
                    }
                },
                visible: |c| c.auto_save,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_timestamps",
                label: "Timestamps",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_timestamps),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_timestamps = b;
                    }
                },
                visible: |c| c.auto_save,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_direction",
                label: "Direction",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_direction),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_direction = b;
                    }
                },
                visible: |c| c.auto_save,
                validate: always_valid,
            },
        ],
    },
];

impl PreConnectView {
    pub fn new() -> Self {
        Self {
            port_list: PortListState::new(),
            config: PreConnectConfig::default(),
            config_nav: ConfigPanelNav::new(),
            search_input: TextInputState::new().with_placeholder("Search ports..."),
            search_focused: false,
            last_visible_height: 20, // Reasonable default
        }
    }

    pub fn refresh_ports(&mut self) {
        match list_ports() {
            Ok(ports) => {
                self.port_list.set_ports(ports);
            }
            Err(_) => {
                self.port_list.set_ports(vec![]);
            }
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        focus: Focus,
    ) {
        // Main area: port list + optional search bar
        let main_chunks = if self.search_focused || self.port_list.has_search() {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(3)])
                .split(main_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5)])
                .split(main_area)
        };

        // Track visible height for half-page scrolling (subtract borders)
        self.last_visible_height = main_chunks[0].height.saturating_sub(2) as usize;

        // Port list
        let port_title = if self.port_list.has_search() {
            let status = self.port_list.search_status();
            format!(" Available Ports [{}] ", status)
        } else {
            " Available Ports ".to_string()
        };

        let port_block = Block::default()
            .title(port_title)
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main && !self.search_focused {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        PortList::new()
            .block(port_block)
            .focused(focus == Focus::Main && !self.search_focused)
            .render(main_chunks[0], buf, &mut self.port_list);

        // Search bar if active
        if self.search_focused || self.port_list.has_search() {
            let search_block = Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(if self.search_focused {
                    Theme::border_focused()
                } else {
                    Theme::border()
                });

            TextInput::new(&mut self.search_input)
                .block(search_block)
                .focused(self.search_focused)
                .render(main_chunks[1], buf);
        }

        // Help text at bottom of port list
        if main_chunks[0].height > 2 {
            let help_y = main_chunks[0].y + main_chunks[0].height - 2;
            let help_line = Line::from(vec![
                Span::styled("Enter", Theme::keybind()),
                Span::raw(" connect  "),
                Span::styled("r", Theme::keybind()),
                Span::raw(" refresh  "),
                Span::styled("/", Theme::keybind()),
                Span::raw(" search  "),
                Span::styled("Ctrl+h/l", Theme::keybind()),
                Span::raw(" panels  "),
                Span::styled("?", Theme::keybind()),
                Span::raw(" help"),
            ]);
            Paragraph::new(help_line)
                .style(Theme::muted())
                .render(Rect::new(main_chunks[0].x + 2, help_y, main_chunks[0].width - 4, 1), buf);
        }

        // Config panel
        if let Some(config_area) = config_area {
            let config_block = Block::default()
                .title(" Configuration ")
                .borders(Borders::ALL)
                .border_style(if focus == Focus::Config {
                    Theme::border_focused()
                } else {
                    Theme::border()
                });

            ConfigPanel::new(PRECONNECT_CONFIG_SECTIONS, &self.config, &self.config_nav)
                .block(config_block)
                .focused(focus == Focus::Config)
                .render(config_area, buf);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus) -> Option<PreConnectAction> {
        // Handle search input mode
        if self.search_focused {
            match key.code {
                KeyCode::Enter => {
                    // Apply search and exit search mode
                    let pattern = self.search_input.content.clone();
                    self.port_list.set_search(&pattern);
                    self.search_focused = false;
                }
                KeyCode::Esc => {
                    // Clear search and exit
                    self.search_focused = false;
                    self.search_input.clear();
                    self.port_list.clear_search();
                }
                _ => {
                    self.search_input.handle_key(key);
                    // Live search as user types
                    self.port_list.set_search(&self.search_input.content);
                }
            }
            return None;
        }

        match focus {
            Focus::Main => self.handle_main_key(key),
            Focus::Config => self.handle_config_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        // Half-page scroll amount based on visible height
        let half_page = self.last_visible_height / 2;

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.port_list.select_next();
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.port_list.select_prev();
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down
                for _ in 0..half_page {
                    self.port_list.select_next();
                }
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up
                for _ in 0..half_page {
                    self.port_list.select_prev();
                }
            }
            KeyCode::Char('r') => {
                self.refresh_ports();
                return Some(PreConnectAction::Toast(Toast::info("Ports refreshed")));
            }
            KeyCode::Char('/') => {
                self.search_focused = true;
            }
            KeyCode::Char('n') => {
                // Next search match
                self.port_list.goto_next_match();
            }
            KeyCode::Char('N') => {
                // Previous search match
                self.port_list.goto_prev_match();
            }
            KeyCode::Enter => {
                if let Some(port) = self.port_list.selected_name() {
                    return Some(PreConnectAction::Connect {
                        port: port.to_string(),
                        serial_config: self.config.to_serial_config(),
                        session_config: self.config.to_session_config(),
                    });
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        // Handle dropdown mode separately
        if self.config_nav.is_dropdown_open() {
            return self.handle_dropdown_key(key);
        }

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.config_nav
                    .next_field(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.config_nav
                    .prev_field(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                // For toggle fields, toggle; for select, cycle prev
                if let Some(field) = self
                    .config_nav
                    .current_field(PRECONNECT_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    } else if field.kind.is_select() {
                        self.config_nav
                            .dropdown_prev(PRECONNECT_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // For toggle fields, toggle; for select, cycle next
                if let Some(field) = self
                    .config_nav
                    .current_field(PRECONNECT_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    } else if field.kind.is_select() {
                        self.config_nav
                            .dropdown_next(PRECONNECT_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Open dropdown for select fields, toggle for toggle fields
                if let Some(field) = self
                    .config_nav
                    .current_field(PRECONNECT_CONFIG_SECTIONS, &self.config)
                {
                    if field.kind.is_select() {
                        self.config_nav
                            .open_dropdown(PRECONNECT_CONFIG_SECTIONS, &self.config);
                    } else if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            _ => {}
        }
        // Sync dropdown index with current selection
        self.config_nav
            .sync_dropdown_index(PRECONNECT_CONFIG_SECTIONS, &self.config);
        None
    }

    fn handle_dropdown_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.config_nav
                    .dropdown_next(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.config_nav
                    .dropdown_prev(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Apply selection and close dropdown
                let _ = self
                    .config_nav
                    .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                self.config_nav.close_dropdown();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                // Close dropdown without applying
                self.config_nav.close_dropdown();
                // Restore original value
                self.config_nav
                    .sync_dropdown_index(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            _ => {}
        }
        None
    }
}

impl Default for PreConnectView {
    fn default() -> Self {
        Self::new()
    }
}
