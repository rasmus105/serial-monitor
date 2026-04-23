//! Pre-connection view: port selection and configuration.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use serial_core::{
    ChunkingStrategy, DataBits, LineDelimiter, SerialConfig, list_ports,
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

use crate::{
    app::{Focus, PreConnectAction},
    keybind::PRECONNECT_HINTS,
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfigPanel, PortList, TextInput, Toast,
        port_list::PortListState,
        text_input::{TextInputState, find_path_completions},
        util::build_help_line,
    },
};

/// Pre-connection view state.
pub struct PreConnectView {
    /// Port list state.
    pub port_list: PortListState,
    /// Serial configuration.
    pub config: PreConnectConfig,
    /// Config panel navigation.
    pub config_nav: ConfigNav,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
    /// Directory path input state.
    pub dir_path_input: TextInputState,
    /// Whether directory path input is focused.
    pub dir_path_focused: bool,
    /// Directory path completion state.
    pub dir_path_completion: CompletionState,
    /// Last visible height for port list (for half-page scroll).
    last_visible_height: usize,
    /// Last time ports were auto-refreshed.
    last_port_refresh: Instant,
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
    // File saving settings
    pub file_save_enabled: bool,
    pub file_save_format_index: usize,
    pub file_save_encoding_index: usize,
    pub file_save_directory: String,
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

// Session settings options
const RX_CHUNKING_OPTIONS: &[&str] = &["None (Raw)", "LF (\\n)", "CR (\\r)", "CRLF (\\r\\n)"];

// File saving options
const FILE_SAVE_FORMAT_OPTIONS: &[&str] = &["Raw Binary", "Encoded Text"];
const FILE_SAVE_ENCODING_OPTIONS: &[&str] = &["UTF-8", "ASCII", "Hex", "Binary"];

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

impl PreConnectView {
    pub fn new() -> Self {
        Self {
            port_list: PortListState::default(),
            config: PreConnectConfig::default(),
            config_nav: ConfigNav::new(),
            search_input: TextInputState::default().with_placeholder("Search ports..."),
            search_focused: false,
            dir_path_input: TextInputState::default().with_placeholder("Enter directory path..."),
            dir_path_focused: false,
            dir_path_completion: CompletionState::default(),
            last_visible_height: 20, // Reasonable default
            last_port_refresh: Instant::now(),
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

    /// Auto-refreshes ports every 500ms. Returns a toast if ports changed.
    pub fn tick_auto_refresh(&mut self) -> Option<Toast> {
        use std::time::Duration;
        const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

        if self.last_port_refresh.elapsed() < REFRESH_INTERVAL {
            return None;
        }

        self.last_port_refresh = Instant::now();

        let old_ports: Vec<String> = self
            .port_list
            .ports
            .iter()
            .map(|p| p.name.clone())
            .collect();

        self.refresh_ports();

        let new_ports: Vec<String> = self
            .port_list
            .ports
            .iter()
            .map(|p| p.name.clone())
            .collect();

        // Detect changes
        let added: Vec<_> = new_ports
            .iter()
            .filter(|p| !old_ports.contains(p))
            .collect();
        let removed: Vec<_> = old_ports
            .iter()
            .filter(|p| !new_ports.contains(p))
            .collect();

        if !added.is_empty() {
            return Some(Toast::info(format!(
                "Port connected: {}",
                added
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if !removed.is_empty() {
            return Some(Toast::info(format!(
                "Port disconnected: {}",
                removed
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        None
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused || self.dir_path_focused
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        focus: Focus,
    ) {
        // Main area: port list + optional search/directory input bar
        let show_search_bar = self.search_focused || self.port_list.has_search();
        let show_dir_bar = self.dir_path_focused;

        let main_chunks = if show_search_bar || show_dir_bar {
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
                Theme::border_disconnected()
            } else {
                Theme::border()
            });

        PortList::default()
            .block(port_block)
            .focused(focus == Focus::Main && !self.search_focused)
            .render(main_chunks[0], buf, &mut self.port_list);

        // Search bar if active
        if self.search_focused || self.port_list.has_search() {
            let search_block = Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(if self.search_focused {
                    Theme::border_disconnected()
                } else {
                    Theme::border()
                });

            TextInput::new(&mut self.search_input)
                .block(search_block)
                .focused(self.search_focused)
                .render(main_chunks[1], buf);
        } else if self.dir_path_focused {
            // Directory path input bar
            let dir_block = Block::default()
                .title(" Save Directory ")
                .borders(Borders::ALL)
                .border_style(Theme::border_disconnected());

            TextInput::new(&mut self.dir_path_input)
                .block(dir_block)
                .focused(true)
                .render(main_chunks[1], buf);

            // Render completion popup (above the input bar)
            if self.dir_path_completion.visible {
                let input_inner = Block::default().borders(Borders::ALL).inner(main_chunks[1]);
                CompletionPopup::new(&self.dir_path_completion, input_inner.y, input_inner.x)
                    .disconnected(true)
                    .render(main_area, buf);
            }
        }

        // Help text at bottom of port list
        if main_chunks[0].height > 2 {
            let help_y = main_chunks[0].y + main_chunks[0].height - 2;
            let help_line = build_help_line(PRECONNECT_HINTS, Theme::keybind_disconnected());
            Paragraph::new(help_line).style(Theme::muted()).render(
                Rect::new(main_chunks[0].x + 2, help_y, main_chunks[0].width - 4, 1),
                buf,
            );
        }

        // Config panel
        if let Some(config_area) = config_area {
            let config_block = Block::default()
                .title(" Configuration ")
                .borders(Borders::ALL)
                .border_style(if focus == Focus::Config {
                    Theme::border_disconnected()
                } else {
                    Theme::border()
                });

            ConfigPanel::new(PRECONNECT_CONFIG_SECTIONS, &self.config, &self.config_nav)
                .block(config_block)
                .focused(focus == Focus::Config)
                .disconnected(true)
                .render(config_area, buf);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus) -> Option<PreConnectAction> {
        // Handle search input mode
        if self.search_focused {
            match key.code {
                KeyCode::Enter => {
                    // Apply search and exit search mode
                    let pattern = self.search_input.content().to_string();
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
                    self.port_list.set_search(self.search_input.content());
                }
            }
            return None;
        }

        // Handle directory path input mode
        if self.dir_path_focused {
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
                }
                KeyCode::Esc => {
                    if self.dir_path_completion.visible {
                        self.dir_path_completion.hide();
                    } else {
                        // Cancel and exit without saving
                        self.dir_path_focused = false;
                        self.dir_path_input.clear();
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
                        rx_chunking: self.config.rx_chunking(),
                        file_save_enabled: self.config.file_save_enabled,
                        file_save_format_index: self.config.file_save_format_index,
                        file_save_encoding_index: self.config.file_save_encoding_index,
                        file_save_directory: self.config.file_save_directory.clone(),
                    });
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        // Handle dropdown mode separately
        if self.config_nav.edit_mode.is_dropdown() {
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
                            .apply_dropdown(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
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
                            .apply_dropdown(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Open dropdown for select fields, toggle for toggle fields, open input for text fields
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
                    } else if field.kind.is_text_input() {
                        // Open text input bar for text fields
                        if field.id == "file_save_directory" {
                            self.dir_path_input
                                .set_content(&self.config.file_save_directory);
                            self.dir_path_focused = true;
                        }
                    }
                }
            }
            _ => {}
        }
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
                    .apply_dropdown(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                self.config_nav.close_dropdown();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                // Close dropdown without applying
                self.config_nav.close_dropdown();
            }
            _ => {}
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

impl Default for PreConnectView {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Settings integration
// =============================================================================

use crate::settings::PreConnectSettings;

impl PreConnectView {
    /// Apply settings loaded from disk.
    pub fn apply_settings(&mut self, settings: &PreConnectSettings) {
        self.config.baud_rate_index = settings.baud_rate_index;
        self.config.data_bits_index = settings.data_bits_index;
        self.config.parity_index = settings.parity_index;
        self.config.stop_bits_index = settings.stop_bits_index;
        self.config.flow_control_index = settings.flow_control_index;
        self.config.line_ending_index = settings.line_ending_index;
        self.config.file_save_enabled = settings.file_save_enabled;
        self.config.file_save_format_index = settings.file_save_format_index;
        self.config.file_save_encoding_index = settings.file_save_encoding_index;
        self.config.file_save_directory = settings.file_save_directory.clone();
    }

    /// Extract settings for saving to disk.
    pub fn to_settings(&self) -> PreConnectSettings {
        PreConnectSettings {
            baud_rate_index: self.config.baud_rate_index,
            data_bits_index: self.config.data_bits_index,
            parity_index: self.config.parity_index,
            stop_bits_index: self.config.stop_bits_index,
            flow_control_index: self.config.flow_control_index,
            line_ending_index: self.config.line_ending_index,
            file_save_enabled: self.config.file_save_enabled,
            file_save_format_index: self.config.file_save_format_index,
            file_save_encoding_index: self.config.file_save_encoding_index,
            file_save_directory: self.config.file_save_directory.clone(),
        }
    }
}
