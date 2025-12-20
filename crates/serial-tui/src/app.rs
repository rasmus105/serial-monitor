//! Application state and logic

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serial_core::{
    encode, list_ports, send_file, DataBits, Encoding, FileSendConfig, FileSendHandle,
    FileSendProgress, FlowControl, Parity, PortInfo, SerialConfig, Session, SessionEvent,
    SessionHandle, StopBits,
};
use strum::{EnumCount, EnumIter, IntoEnumIterator};

use crate::command::{
    map_dropdown_key, map_port_select_key, map_traffic_key, DropdownCommand, PortSelectCommand,
    TrafficCommand,
};

// =============================================================================
// ConfigOption Trait - Abstraction for serial config enum options
// =============================================================================

/// Trait for config options that can be cycled through in a dropdown
pub trait ConfigOption: Sized + Copy + PartialEq + 'static {
    /// All possible variants in display order
    fn all_variants() -> &'static [Self];

    /// Display name for this variant
    fn display_name(&self) -> &'static str;

    /// Get the index of this variant in the list
    fn index(&self) -> usize {
        Self::all_variants()
            .iter()
            .position(|v| v == self)
            .unwrap_or(0)
    }

    /// Get variant by index (with wrapping)
    fn from_index(idx: usize) -> Self {
        let variants = Self::all_variants();
        variants[idx.min(variants.len() - 1)]
    }

    /// Get display names for all variants
    fn all_display_names() -> Vec<&'static str> {
        Self::all_variants()
            .iter()
            .map(|v| v.display_name())
            .collect()
    }
}

// Implement ConfigOption for serialport types

impl ConfigOption for DataBits {
    fn all_variants() -> &'static [Self] {
        &[
            DataBits::Five,
            DataBits::Six,
            DataBits::Seven,
            DataBits::Eight,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            DataBits::Five => "5",
            DataBits::Six => "6",
            DataBits::Seven => "7",
            DataBits::Eight => "8",
        }
    }
}

impl ConfigOption for Parity {
    fn all_variants() -> &'static [Self] {
        &[Parity::None, Parity::Odd, Parity::Even]
    }

    fn display_name(&self) -> &'static str {
        match self {
            Parity::None => "None",
            Parity::Odd => "Odd",
            Parity::Even => "Even",
        }
    }
}

impl ConfigOption for StopBits {
    fn all_variants() -> &'static [Self] {
        &[StopBits::One, StopBits::Two]
    }

    fn display_name(&self) -> &'static str {
        match self {
            StopBits::One => "1",
            StopBits::Two => "2",
        }
    }
}

impl ConfigOption for FlowControl {
    fn all_variants() -> &'static [Self] {
        &[FlowControl::None, FlowControl::Software, FlowControl::Hardware]
    }

    fn display_name(&self) -> &'static str {
        match self {
            FlowControl::None => "None",
            FlowControl::Software => "XON/XOFF",
            FlowControl::Hardware => "RTS/CTS",
        }
    }
}

// =============================================================================
// Enums
// =============================================================================

/// Current view/screen
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Port selection screen
    PortSelect,
    /// Traffic view (main view)
    Traffic,
}

/// Which panel is focused in port selection view
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PortSelectFocus {
    /// Port list panel (left)
    #[default]
    PortList,
    /// Configuration panel (right)
    Config,
}

/// Which configuration field is selected
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount)]
pub enum ConfigField {
    #[default]
    BaudRate,
    DataBits,
    Parity,
    StopBits,
    FlowControl,
}

impl ConfigField {
    pub fn next(self) -> Self {
        let variants: Vec<_> = Self::iter().collect();
        let idx = variants.iter().position(|&v| v == self).unwrap_or(0);
        variants[(idx + 1) % variants.len()]
    }

    pub fn prev(self) -> Self {
        let variants: Vec<_> = Self::iter().collect();
        let idx = variants.iter().position(|&v| v == self).unwrap_or(0);
        variants[(idx + variants.len() - 1) % variants.len()]
    }

    pub fn index(self) -> usize {
        Self::iter().position(|v| v == self).unwrap_or(0)
    }

    /// Get the label for this config field
    pub fn label(&self) -> &'static str {
        match self {
            ConfigField::BaudRate => "Baud Rate",
            ConfigField::DataBits => "Data Bits",
            ConfigField::Parity => "Parity",
            ConfigField::StopBits => "Stop Bits",
            ConfigField::FlowControl => "Flow Ctrl",
        }
    }
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq, Default)]
pub enum InputMode {
    /// Normal navigation mode
    #[default]
    Normal,
    /// Entering a port path manually
    PortInput,
    /// Entering data to send to serial port
    SendInput,
    /// Entering search pattern
    SearchInput,
    /// Entering file path to send
    FilePathInput,
    /// Config dropdown is open
    ConfigDropdown,
}

/// Visual properties for rendering an input mode in the status bar
#[derive(Debug, Clone, Copy)]
pub struct InputModeStyle {
    /// Prefix shown before the input buffer (e.g., ":", "/", "> ")
    pub prefix: &'static str,
    /// Color for the prefix and cursor
    pub color: ratatui::style::Color,
}

impl InputMode {
    /// Get the prompt shown in the status bar when entering this mode
    pub fn entry_prompt(&self) -> &'static str {
        match self {
            InputMode::Normal => "",
            InputMode::PortInput => "Enter port path (e.g., /dev/pts/5):",
            InputMode::SendInput => "Type to send (Enter: send with newline, Esc: cancel)",
            InputMode::SearchInput => "Search: ",
            InputMode::FilePathInput => "Enter file path to send:",
            InputMode::ConfigDropdown => "j/k: navigate, Enter: select, Esc: cancel",
        }
    }

    /// Get the visual style for rendering this input mode
    pub fn style(&self) -> Option<InputModeStyle> {
        use ratatui::style::Color;
        match self {
            InputMode::Normal => None,
            InputMode::PortInput => Some(InputModeStyle {
                prefix: ":",
                color: Color::Yellow,
            }),
            InputMode::SendInput => Some(InputModeStyle {
                prefix: "> ",
                color: Color::Green,
            }),
            InputMode::SearchInput => Some(InputModeStyle {
                prefix: "/",
                color: Color::Magenta,
            }),
            InputMode::FilePathInput => Some(InputModeStyle {
                prefix: "File: ",
                color: Color::Blue,
            }),
            InputMode::ConfigDropdown => None, // Uses special rendering
        }
    }
}

/// Connection state
#[derive(Debug)]
pub enum ConnectionState {
    Disconnected,
    Connected(SessionHandle),
}

// =============================================================================
// State Sub-structs
// =============================================================================

/// State for port selection view
#[derive(Debug)]
pub struct PortSelectState {
    /// Available serial ports
    pub ports: Vec<PortInfo>,
    /// Selected port index
    pub selected_port: usize,
    /// Which panel is focused
    pub focus: PortSelectFocus,
    /// Which config field is selected
    pub config_field: ConfigField,
    /// Whether config panel is visible
    pub config_panel_visible: bool,
    /// Serial port configuration
    pub serial_config: SerialConfig,
    /// Dropdown selection index (when dropdown is open)
    pub dropdown_index: usize,
}

impl Default for PortSelectState {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            selected_port: 0,
            focus: PortSelectFocus::default(),
            config_field: ConfigField::default(),
            config_panel_visible: true,
            serial_config: SerialConfig::default(),
            dropdown_index: 0,
        }
    }
}

impl PortSelectState {
    /// Common baud rates for dropdown
    pub const BAUD_RATES: [u32; 10] = [
        300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400,
    ];

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) -> String {
        self.ports = list_ports().unwrap_or_default();
        self.selected_port = 0;
        if self.ports.is_empty() {
            "No serial ports found. Press ':' to enter path manually.".to_string()
        } else {
            format!("Found {} port(s).", self.ports.len())
        }
    }

    /// Get string options for dropdown (including baud rates as strings)
    pub fn get_config_option_strings(&self) -> Vec<String> {
        match self.config_field {
            ConfigField::BaudRate => Self::BAUD_RATES.iter().map(|b| b.to_string()).collect(),
            ConfigField::DataBits => DataBits::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            ConfigField::Parity => Parity::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            ConfigField::StopBits => StopBits::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            ConfigField::FlowControl => FlowControl::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    /// Get the current index in the options list for the selected config field
    pub fn get_current_config_index(&self) -> usize {
        match self.config_field {
            ConfigField::BaudRate => Self::BAUD_RATES
                .iter()
                .position(|&b| b == self.serial_config.baud_rate)
                .unwrap_or(8), // Default to 115200
            ConfigField::DataBits => self.serial_config.data_bits.index(),
            ConfigField::Parity => self.serial_config.parity.index(),
            ConfigField::StopBits => self.serial_config.stop_bits.index(),
            ConfigField::FlowControl => self.serial_config.flow_control.index(),
        }
    }

    /// Get the display value for a config field
    pub fn get_config_display(&self, field: ConfigField) -> String {
        match field {
            ConfigField::BaudRate => self.serial_config.baud_rate.to_string(),
            ConfigField::DataBits => self.serial_config.data_bits.display_name().to_string(),
            ConfigField::Parity => self.serial_config.parity.display_name().to_string(),
            ConfigField::StopBits => self.serial_config.stop_bits.display_name().to_string(),
            ConfigField::FlowControl => self.serial_config.flow_control.display_name().to_string(),
        }
    }

    /// Open the dropdown for the current config field
    pub fn open_dropdown(&mut self) {
        self.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value to the config
    pub fn apply_dropdown_selection(&mut self) {
        match self.config_field {
            ConfigField::BaudRate => {
                if let Some(&baud) = Self::BAUD_RATES.get(self.dropdown_index) {
                    self.serial_config.baud_rate = baud;
                }
            }
            ConfigField::DataBits => {
                self.serial_config.data_bits = DataBits::from_index(self.dropdown_index);
            }
            ConfigField::Parity => {
                self.serial_config.parity = Parity::from_index(self.dropdown_index);
            }
            ConfigField::StopBits => {
                self.serial_config.stop_bits = StopBits::from_index(self.dropdown_index);
            }
            ConfigField::FlowControl => {
                self.serial_config.flow_control = FlowControl::from_index(self.dropdown_index);
            }
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config_field {
            ConfigField::BaudRate => Self::BAUD_RATES.len(),
            ConfigField::DataBits => DataBits::all_variants().len(),
            ConfigField::Parity => Parity::all_variants().len(),
            ConfigField::StopBits => StopBits::all_variants().len(),
            ConfigField::FlowControl => FlowControl::all_variants().len(),
        }
    }
}

/// State for traffic view
#[derive(Debug, Default)]
pub struct TrafficState {
    /// Scroll offset for traffic view
    pub scroll_offset: usize,
    /// Current display encoding
    pub encoding: Encoding,
    /// Target chunk to scroll to (resolved to physical row during render)
    pub scroll_to_chunk: Option<usize>,
}

/// State for search functionality
#[derive(Debug, Default)]
pub struct SearchState {
    /// Current search pattern (if any)
    pub pattern: Option<String>,
    /// Current search match index (line index in the displayed data)
    pub match_index: Option<usize>,
    /// Total number of search matches
    pub match_count: usize,
}

impl SearchState {
    /// Clear search state
    pub fn clear(&mut self) {
        self.pattern = None;
        self.match_index = None;
        self.match_count = 0;
    }
}

/// State for file sending
#[derive(Default)]
pub struct FileSendState {
    /// Active file send operation
    pub handle: Option<FileSendHandle>,
    /// Latest file send progress
    pub progress: Option<FileSendProgress>,
}

/// State for text input
#[derive(Debug, Default)]
pub struct InputState {
    /// Input mode
    pub mode: InputMode,
    /// Input buffer for text entry
    pub buffer: String,
}

/// Result of handling a text input key event
#[derive(Debug, PartialEq)]
pub enum TextInputResult {
    /// User submitted the input (Enter pressed with non-empty buffer)
    Submit(String),
    /// User cancelled the input (Esc pressed)
    Cancel,
    /// Input buffer was modified, continue editing
    Continue,
}

impl InputState {
    /// Handle a key event for generic text input.
    /// Returns what action should be taken by the caller.
    pub fn handle_text_input(&mut self, key: KeyEvent) -> TextInputResult {
        match key.code {
            KeyCode::Enter => {
                if !self.buffer.is_empty() {
                    let value = self.buffer.clone();
                    self.buffer.clear();
                    self.mode = InputMode::Normal;
                    TextInputResult::Submit(value)
                } else {
                    self.mode = InputMode::Normal;
                    TextInputResult::Cancel
                }
            }
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                self.buffer.clear();
                TextInputResult::Cancel
            }
            KeyCode::Backspace => {
                self.buffer.pop();
                TextInputResult::Continue
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.buffer.push(c);
                }
                TextInputResult::Continue
            }
            _ => TextInputResult::Continue,
        }
    }
}

// =============================================================================
// Main App Struct
// =============================================================================

/// Application state
pub struct App {
    /// Should the application quit?
    pub should_quit: bool,
    /// Should the terminal be fully cleared on next render?
    pub needs_full_clear: bool,
    /// Current view
    pub view: View,
    /// Connection state
    pub connection: ConnectionState,
    /// Status message
    pub status: String,

    /// Input state
    pub input: InputState,
    /// Port selection state
    pub port_select: PortSelectState,
    /// Traffic view state
    pub traffic: TrafficState,
    /// Search state
    pub search: SearchState,
    /// File send state
    pub file_send: FileSendState,

    /// Tokio runtime handle for async operations
    runtime: tokio::runtime::Handle,
}

impl App {
    /// Create a new application
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        let mut port_select = PortSelectState::default();
        let _ = port_select.refresh_ports();
        let status = if port_select.ports.is_empty() {
            "No serial ports found. Press ':' to enter path manually, 'r' to refresh.".to_string()
        } else {
            format!(
                "Found {} port(s). Select and press Enter, or ':' to enter path manually.",
                port_select.ports.len()
            )
        };

        Self {
            should_quit: false,
            needs_full_clear: false,
            view: View::PortSelect,
            connection: ConnectionState::Disconnected,
            status,
            input: InputState::default(),
            port_select,
            traffic: TrafficState::default(),
            search: SearchState::default(),
            file_send: FileSendState::default(),
            runtime,
        }
    }

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) {
        self.status = self.port_select.refresh_ports();
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.input.mode {
            InputMode::Normal => match self.view {
                View::PortSelect => self.handle_key_port_select(key),
                View::Traffic => self.handle_key_traffic(key),
            },
            InputMode::PortInput => self.handle_key_port_input(key),
            InputMode::SendInput => self.handle_key_send_input(key),
            InputMode::SearchInput => self.handle_key_search_input(key),
            InputMode::FilePathInput => self.handle_key_file_path_input(key),
            InputMode::ConfigDropdown => self.handle_key_config_dropdown(key),
        }
    }

    fn handle_key_port_select(&mut self, key: KeyEvent) {
        let cmd = map_port_select_key(key, self.port_select.config_panel_visible);

        let Some(cmd) = cmd else { return };

        match cmd {
            PortSelectCommand::Quit => self.should_quit = true,
            PortSelectCommand::RefreshPorts => self.refresh_ports(),
            PortSelectCommand::EnterPortPath => {
                self.input.mode = InputMode::PortInput;
                self.input.buffer.clear();
                self.status = InputMode::PortInput.entry_prompt().to_string();
            }
            PortSelectCommand::ToggleConfigPanel => {
                self.port_select.config_panel_visible = !self.port_select.config_panel_visible;
            }
            PortSelectCommand::FocusPortList => {
                self.port_select.focus = PortSelectFocus::PortList;
            }
            PortSelectCommand::FocusConfig => {
                self.port_select.focus = PortSelectFocus::Config;
            }
            PortSelectCommand::MoveUp => match self.port_select.focus {
                PortSelectFocus::PortList => {
                    if self.port_select.selected_port > 0 {
                        self.port_select.selected_port -= 1;
                    }
                }
                PortSelectFocus::Config => {
                    self.port_select.config_field = self.port_select.config_field.prev();
                }
            },
            PortSelectCommand::MoveDown => match self.port_select.focus {
                PortSelectFocus::PortList => {
                    if !self.port_select.ports.is_empty()
                        && self.port_select.selected_port < self.port_select.ports.len() - 1
                    {
                        self.port_select.selected_port += 1;
                    }
                }
                PortSelectFocus::Config => {
                    self.port_select.config_field = self.port_select.config_field.next();
                }
            },
            PortSelectCommand::Confirm => match self.port_select.focus {
                PortSelectFocus::PortList => {
                    if !self.port_select.ports.is_empty() {
                        self.connect_to_selected_port();
                    }
                }
                PortSelectFocus::Config => {
                    self.port_select.open_dropdown();
                    self.input.mode = InputMode::ConfigDropdown;
                }
            },
        }
    }

    fn handle_key_config_dropdown(&mut self, key: KeyEvent) {
        let Some(cmd) = map_dropdown_key(key) else {
            return;
        };

        let options_count = self.port_select.get_options_count();

        match cmd {
            DropdownCommand::MoveUp => {
                if self.port_select.dropdown_index > 0 {
                    self.port_select.dropdown_index -= 1;
                }
            }
            DropdownCommand::MoveDown => {
                if self.port_select.dropdown_index < options_count - 1 {
                    self.port_select.dropdown_index += 1;
                }
            }
            DropdownCommand::Confirm => {
                self.port_select.apply_dropdown_selection();
                self.input.mode = InputMode::Normal;
            }
            DropdownCommand::Cancel => {
                self.input.mode = InputMode::Normal;
            }
        }
    }

    fn handle_key_port_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(port_path) => {
                self.connect_to_port(&port_path);
            }
            TextInputResult::Cancel => {
                self.status = "Cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    fn handle_key_traffic(&mut self, key: KeyEvent) {
        let Some(cmd) = map_traffic_key(key) else {
            return;
        };

        match cmd {
            TrafficCommand::Disconnect => {
                self.disconnect();
                self.view = View::PortSelect;
                self.needs_full_clear = true;
                self.status = "Disconnected.".to_string();
            }
            TrafficCommand::ScrollUp => {
                self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_sub(1);
            }
            TrafficCommand::ScrollDown => {
                self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_add(1);
            }
            TrafficCommand::ScrollToTop => {
                self.traffic.scroll_offset = 0;
            }
            TrafficCommand::ScrollToBottom => {
                self.traffic.scroll_offset = usize::MAX;
            }
            TrafficCommand::PageUp => {
                self.traffic.scroll_offset =
                    self.traffic.scroll_offset.saturating_sub(self.page_size());
            }
            TrafficCommand::PageDown => {
                self.traffic.scroll_offset =
                    self.traffic.scroll_offset.saturating_add(self.page_size());
            }
            TrafficCommand::CycleEncoding => {
                self.traffic.encoding = self.traffic.encoding.cycle_next();
                self.status = format!("Encoding: {}", self.traffic.encoding);
                self.needs_full_clear = true;
                if self.search.pattern.is_some() {
                    self.update_search_matches();
                }
            }
            TrafficCommand::EnterSendMode => {
                self.input.mode = InputMode::SendInput;
                self.input.buffer.clear();
                self.status = InputMode::SendInput.entry_prompt().to_string();
            }
            TrafficCommand::EnterSearchMode => {
                self.input.mode = InputMode::SearchInput;
                self.input.buffer.clear();
                self.status = InputMode::SearchInput.entry_prompt().to_string();
            }
            TrafficCommand::NextMatch => {
                self.goto_next_match();
            }
            TrafficCommand::PrevMatch => {
                self.goto_prev_match();
            }
            TrafficCommand::ToggleFileSend => {
                if self.file_send.handle.is_some() {
                    self.cancel_file_send();
                } else {
                    self.input.mode = InputMode::FilePathInput;
                    self.input.buffer.clear();
                    self.status = InputMode::FilePathInput.entry_prompt().to_string();
                }
            }
            TrafficCommand::EscapeOrClear => {
                if self.search.pattern.is_some() {
                    self.search.clear();
                    self.status = "Search cleared.".to_string();
                } else {
                    self.disconnect();
                    self.view = View::PortSelect;
                    self.needs_full_clear = true;
                    self.status = "Disconnected.".to_string();
                }
            }
        }
    }

    fn handle_key_search_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(pattern) => {
                self.search.pattern = Some(pattern);
                self.update_search_matches();
                self.goto_next_match();
            }
            TextInputResult::Cancel => {
                self.status = "Search cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    fn find_matching_lines(&self) -> Vec<usize> {
        let pattern = match &self.search.pattern {
            Some(p) => p,
            None => return vec![],
        };

        let mut matches = Vec::new();

        if let ConnectionState::Connected(ref handle) = self.connection {
            let buffer = handle.buffer();
            for (idx, chunk) in buffer.chunks().enumerate() {
                let encoded = encode(&chunk.data, self.traffic.encoding);
                if encoded.to_lowercase().contains(&pattern.to_lowercase()) {
                    matches.push(idx);
                }
            }
        }

        matches
    }

    fn update_search_matches(&mut self) {
        let matches = self.find_matching_lines();
        self.search.match_count = matches.len();

        if matches.is_empty() {
            self.search.match_index = None;
            if let Some(ref pattern) = self.search.pattern {
                self.status = format!("Pattern not found: {}", pattern);
            }
        } else {
            self.status = format!(
                "Found {} match{}",
                self.search.match_count,
                if self.search.match_count == 1 { "" } else { "es" }
            );
        }
    }

    fn goto_next_match(&mut self) {
        let matches = self.find_matching_lines();
        if matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        let next_idx = match self.search.match_index {
            Some(current) => matches
                .iter()
                .position(|&m| m > current)
                .unwrap_or(0),
            None => 0,
        };

        self.search.match_index = Some(matches[next_idx]);
        self.traffic.scroll_to_chunk = Some(matches[next_idx]);
        self.status = format!(
            "Match {}/{}: {}",
            next_idx + 1,
            matches.len(),
            self.search.pattern.as_deref().unwrap_or("")
        );
    }

    fn goto_prev_match(&mut self) {
        let matches = self.find_matching_lines();
        if matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        let prev_idx = match self.search.match_index {
            Some(current) => matches
                .iter()
                .rposition(|&m| m < current)
                .unwrap_or(matches.len() - 1),
            None => matches.len() - 1,
        };

        self.search.match_index = Some(matches[prev_idx]);
        self.traffic.scroll_to_chunk = Some(matches[prev_idx]);
        self.status = format!(
            "Match {}/{}: {}",
            prev_idx + 1,
            matches.len(),
            self.search.pattern.as_deref().unwrap_or("")
        );
    }

    fn handle_key_file_path_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(path) => {
                self.start_file_send(&path);
            }
            TextInputResult::Cancel => {
                self.status = "File send cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    fn start_file_send(&mut self, path: &str) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let config = FileSendConfig::default()
                .with_chunk_size(64)
                .with_delay(std::time::Duration::from_millis(10));

            match self.runtime.block_on(send_file(handle, path, config)) {
                Ok(file_handle) => {
                    self.file_send.handle = Some(file_handle);
                    self.file_send.progress = None;
                    self.status = format!("Sending file: {}", path);
                }
                Err(e) => {
                    self.status = format!("Failed to send file: {}", e);
                }
            }
        } else {
            self.status = "Not connected.".to_string();
        }
    }

    fn cancel_file_send(&mut self) {
        if let Some(ref handle) = self.file_send.handle {
            self.runtime.block_on(handle.cancel());
        }
        self.file_send.handle = None;
        self.file_send.progress = None;
        self.status = "File send cancelled.".to_string();
    }

    /// Poll for file send progress
    pub fn poll_file_send(&mut self) {
        if let Some(ref mut handle) = self.file_send.handle {
            while let Some(progress) = handle.try_recv_progress() {
                let complete = progress.complete;
                let error = progress.error.clone();
                self.file_send.progress = Some(progress);

                if complete {
                    if let Some(err) = error {
                        self.status = format!("File send failed: {}", err);
                    } else {
                        self.status = "File send complete.".to_string();
                    }
                    self.file_send.handle = None;
                    break;
                }
            }
        }
    }

    fn handle_key_send_input(&mut self, key: KeyEvent) {
        // Send input is special: Enter sends with newline but stays in input mode,
        // Ctrl+J sends without newline, Esc exits
        match key.code {
            KeyCode::Enter => {
                if !self.input.buffer.is_empty() {
                    let mut data = self.input.buffer.clone();
                    data.push('\n');
                    self.send_data(data.into_bytes());
                    self.input.buffer.clear();
                }
            }
            KeyCode::Esc => {
                self.input.mode = InputMode::Normal;
                self.input.buffer.clear();
                self.status = "Send cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input.buffer.pop();
            }
            KeyCode::Char(c) if c == 'j' && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+J: send without newline
                if !self.input.buffer.is_empty() {
                    let data = self.input.buffer.clone();
                    self.send_data(data.into_bytes());
                    self.input.buffer.clear();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.buffer.push(c);
            }
            _ => {}
        }
    }

    fn send_data(&mut self, data: Vec<u8>) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let len = data.len();
            match self.runtime.block_on(handle.send(data)) {
                Ok(()) => {
                    self.status = format!("Sent {} bytes", len);
                }
                Err(e) => {
                    self.status = format!("Send failed: {}", e);
                }
            }
        }
    }

    fn connect_to_selected_port(&mut self) {
        if let Some(port) = self.port_select.ports.get(self.port_select.selected_port) {
            let port_name = port.name.clone();
            self.connect_to_port(&port_name);
        }
    }

    fn connect_to_port(&mut self, port_name: &str) {
        let config = self.port_select.serial_config.clone();

        self.status = format!("Connecting to {}...", port_name);

        match self.runtime.block_on(Session::connect(port_name, config)) {
            Ok(handle) => {
                self.connection = ConnectionState::Connected(handle);
                self.view = View::Traffic;
                self.traffic.scroll_offset = 0;
                self.status = format!(
                    "Connected to {} @ {} baud",
                    port_name, self.port_select.serial_config.baud_rate
                );
            }
            Err(e) => {
                self.status = format!("Failed to connect: {}", e);
            }
        }
    }

    fn disconnect(&mut self) {
        if let ConnectionState::Connected(handle) =
            std::mem::replace(&mut self.connection, ConnectionState::Disconnected)
        {
            let _ = self.runtime.block_on(handle.disconnect());
        }
    }

    /// Poll for session events (non-blocking)
    pub fn poll_session_events(&mut self) {
        if let ConnectionState::Connected(ref mut handle) = self.connection {
            while let Some(event) = handle.try_recv_event() {
                match event {
                    SessionEvent::Disconnected { error } => {
                        self.status = match error {
                            Some(e) => format!("Disconnected: {}", e),
                            None => "Disconnected.".to_string(),
                        };
                        self.connection = ConnectionState::Disconnected;
                        self.view = View::PortSelect;
                        self.needs_full_clear = true;
                        break;
                    }
                    SessionEvent::Error(e) => {
                        self.status = format!("Error: {}", e);
                    }
                    SessionEvent::DataReceived(_) | SessionEvent::DataSent(_) => {}
                    SessionEvent::Connected => {}
                }
            }
        }
    }

    /// Get the tick rate for the event loop
    pub fn tick_rate(&self) -> Duration {
        Duration::from_millis(50)
    }

    /// Get page size for Ctrl-d/u scrolling (half screen)
    fn page_size(&self) -> usize {
        15
    }
}
