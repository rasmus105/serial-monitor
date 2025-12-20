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

impl ConfigOption for Encoding {
    fn all_variants() -> &'static [Self] {
        Encoding::all()
    }

    fn display_name(&self) -> &'static str {
        match self {
            Encoding::Hex => "HEX",
            Encoding::Utf8 => "UTF-8",
            Encoding::Ascii => "ASCII",
            Encoding::Binary => "Binary",
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

/// Which panel is focused in traffic view
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TrafficFocus {
    /// Traffic content panel (left)
    #[default]
    Traffic,
    /// Configuration panel (right)
    Config,
}

/// Which configuration field is selected in port selection
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount)]
pub enum ConfigField {
    #[default]
    BaudRate,
    DataBits,
    Parity,
    StopBits,
    FlowControl,
}

/// Which configuration field is selected in traffic view config panel
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount)]
pub enum TrafficConfigField {
    #[default]
    LineNumbers,
    Timestamps,
    TimestampFormat,
    AutoScroll,
    LockToBottom,
    Encoding,
    WrapMode,
    ShowTx,
    ShowRx,
    HexGrouping,
}

impl TrafficConfigField {
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
            TrafficConfigField::LineNumbers => "Line Numbers",
            TrafficConfigField::Timestamps => "Timestamps",
            TrafficConfigField::TimestampFormat => "Time Format",
            TrafficConfigField::AutoScroll => "Auto-scroll",
            TrafficConfigField::LockToBottom => "Lock Bottom",
            TrafficConfigField::Encoding => "Encoding",
            TrafficConfigField::WrapMode => "Wrap Mode",
            TrafficConfigField::ShowTx => "Show TX",
            TrafficConfigField::ShowRx => "Show RX",
            TrafficConfigField::HexGrouping => "Hex Grouping",
        }
    }

    /// Whether this field is a simple toggle (vs a dropdown)
    pub fn is_toggle(&self) -> bool {
        matches!(
            self,
            TrafficConfigField::LineNumbers
                | TrafficConfigField::Timestamps
                | TrafficConfigField::AutoScroll
                | TrafficConfigField::LockToBottom
                | TrafficConfigField::ShowTx
                | TrafficConfigField::ShowRx
        )
    }

    /// Get the associated TrafficCommand for this field (if any)
    /// This is used to look up the keyboard shortcut from the command system
    pub fn associated_command(&self) -> Option<TrafficCommand> {
        match self {
            TrafficConfigField::LineNumbers => Some(TrafficCommand::ToggleLineNumbers),
            TrafficConfigField::Timestamps => Some(TrafficCommand::ToggleTimestamps),
            TrafficConfigField::Encoding => Some(TrafficCommand::CycleEncoding),
            _ => None,
        }
    }
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
    /// Config dropdown is open (port selection)
    ConfigDropdown,
    /// Traffic config dropdown is open
    TrafficConfigDropdown,
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
            InputMode::TrafficConfigDropdown => "j/k: navigate, Enter: select, Esc: cancel",
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
            InputMode::ConfigDropdown => None,         // Uses special rendering
            InputMode::TrafficConfigDropdown => None,  // Uses special rendering
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

/// Format for displaying timestamps in traffic view
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimestampFormat {
    /// Relative time since session start (e.g., "+1.234s")
    #[default]
    Relative,
    /// Absolute time with milliseconds (e.g., "12:34:56.789")
    AbsoluteMillis,
    /// Absolute time without milliseconds (e.g., "12:34:56")
    Absolute,
}

impl ConfigOption for TimestampFormat {
    fn all_variants() -> &'static [Self] {
        &[
            TimestampFormat::Relative,
            TimestampFormat::AbsoluteMillis,
            TimestampFormat::Absolute,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            TimestampFormat::Relative => "Relative",
            TimestampFormat::AbsoluteMillis => "HH:MM:SS.mmm",
            TimestampFormat::Absolute => "HH:MM:SS",
        }
    }
}

impl TimestampFormat {
    /// Format a SystemTime according to this format
    pub fn format(&self, time: std::time::SystemTime, session_start: std::time::SystemTime) -> String {
        match self {
            TimestampFormat::Relative => {
                let elapsed = time
                    .duration_since(session_start)
                    .unwrap_or_default();
                let secs = elapsed.as_secs_f64();
                if secs < 10.0 {
                    format!("+{:.3}s", secs)
                } else if secs < 100.0 {
                    format!("+{:.2}s", secs)
                } else if secs < 1000.0 {
                    format!("+{:.1}s", secs)
                } else {
                    format!("+{:.0}s", secs)
                }
            }
            TimestampFormat::AbsoluteMillis => {
                use std::time::UNIX_EPOCH;
                let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
                let secs = duration.as_secs();
                let millis = duration.subsec_millis();
                let hours = (secs / 3600) % 24;
                let minutes = (secs / 60) % 60;
                let seconds = secs % 60;
                format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
            }
            TimestampFormat::Absolute => {
                use std::time::UNIX_EPOCH;
                let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
                let secs = duration.as_secs();
                let hours = (secs / 3600) % 24;
                let minutes = (secs / 60) % 60;
                let seconds = secs % 60;
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
        }
    }

    /// Get the display width of timestamps in this format (for gutter sizing)
    pub fn width(&self) -> usize {
        match self {
            TimestampFormat::Relative => 8,       // "+123.4s " - max reasonable width
            TimestampFormat::AbsoluteMillis => 13, // "12:34:56.789 "
            TimestampFormat::Absolute => 9,        // "12:34:56 "
        }
    }
}

/// How to handle long lines in traffic view
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WrapMode {
    /// Wrap long lines to fit the terminal width
    #[default]
    Wrap,
    /// Truncate long lines (with ellipsis indicator)
    Truncate,
}

impl ConfigOption for WrapMode {
    fn all_variants() -> &'static [Self] {
        &[WrapMode::Wrap, WrapMode::Truncate]
    }

    fn display_name(&self) -> &'static str {
        match self {
            WrapMode::Wrap => "Wrap",
            WrapMode::Truncate => "Truncate",
        }
    }
}

/// Hex byte grouping for hex encoding display
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum HexGrouping {
    /// No grouping (continuous hex)
    None,
    /// Group by 1 byte (space every byte)
    #[default]
    Byte,
    /// Group by 2 bytes (space every 2 bytes)
    Word,
    /// Group by 4 bytes (space every 4 bytes)
    DWord,
}

impl ConfigOption for HexGrouping {
    fn all_variants() -> &'static [Self] {
        &[
            HexGrouping::None,
            HexGrouping::Byte,
            HexGrouping::Word,
            HexGrouping::DWord,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            HexGrouping::None => "None",
            HexGrouping::Byte => "1 byte",
            HexGrouping::Word => "2 bytes",
            HexGrouping::DWord => "4 bytes",
        }
    }
}

impl HexGrouping {
    /// Get the number of bytes per group (0 means no grouping)
    pub fn bytes_per_group(&self) -> usize {
        match self {
            HexGrouping::None => 0,
            HexGrouping::Byte => 1,
            HexGrouping::Word => 2,
            HexGrouping::DWord => 4,
        }
    }
}

/// State for traffic view
#[derive(Debug)]
pub struct TrafficState {
    /// Scroll offset for traffic view
    pub scroll_offset: usize,
    /// Current display encoding
    pub encoding: Encoding,
    /// Target chunk to scroll to (resolved to physical row during render)
    pub scroll_to_chunk: Option<usize>,
    /// Whether to show line numbers in the gutter
    pub show_line_numbers: bool,
    /// Whether to show timestamps in the gutter
    pub show_timestamps: bool,
    /// Timestamp display format
    pub timestamp_format: TimestampFormat,
    /// Session start time (for relative timestamps)
    pub session_start: Option<std::time::SystemTime>,
    /// Whether the config panel is visible
    pub config_panel_visible: bool,
    /// Which panel is focused (traffic or config)
    pub focus: TrafficFocus,
    /// Which config field is selected (when config panel is focused)
    pub config_field: TrafficConfigField,
    /// Dropdown selection index (when dropdown is open)
    pub dropdown_index: usize,
    /// Auto-scroll: follow new data when at bottom
    pub auto_scroll: bool,
    /// Lock to bottom: always scroll to bottom
    pub lock_to_bottom: bool,
    /// Whether user was at bottom before last scroll (for auto-scroll logic)
    pub was_at_bottom: bool,
    /// How to handle long lines
    pub wrap_mode: WrapMode,
    /// Whether to show transmitted data
    pub show_tx: bool,
    /// Whether to show received data
    pub show_rx: bool,
    /// Hex byte grouping
    pub hex_grouping: HexGrouping,
    /// Total physical rows (cached during render for scroll calculations)
    pub total_rows: usize,
    /// Visible height (cached during render)
    pub visible_height: usize,
}

impl Default for TrafficState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            encoding: Encoding::default(),
            scroll_to_chunk: None,
            show_line_numbers: true,
            show_timestamps: false,
            timestamp_format: TimestampFormat::default(),
            session_start: None,
            config_panel_visible: false,
            focus: TrafficFocus::default(),
            config_field: TrafficConfigField::default(),
            dropdown_index: 0,
            auto_scroll: true,
            lock_to_bottom: false,
            was_at_bottom: true,
            wrap_mode: WrapMode::default(),
            show_tx: true,
            show_rx: true,
            hex_grouping: HexGrouping::default(),
            total_rows: 0,
            visible_height: 0,
        }
    }
}

impl TrafficState {
    /// Get display value for a traffic config field
    pub fn get_config_display(&self, field: TrafficConfigField) -> String {
        match field {
            TrafficConfigField::LineNumbers => {
                if self.show_line_numbers { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::Timestamps => {
                if self.show_timestamps { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::TimestampFormat => self.timestamp_format.display_name().to_string(),
            TrafficConfigField::AutoScroll => {
                if self.auto_scroll { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::LockToBottom => {
                if self.lock_to_bottom { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::Encoding => self.encoding.display_name().to_string(),
            TrafficConfigField::WrapMode => self.wrap_mode.display_name().to_string(),
            TrafficConfigField::ShowTx => {
                if self.show_tx { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::ShowRx => {
                if self.show_rx { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::HexGrouping => self.hex_grouping.display_name().to_string(),
        }
    }

    /// Get string options for dropdown (for non-toggle fields)
    pub fn get_config_option_strings(&self) -> Vec<String> {
        match self.config_field {
            TrafficConfigField::TimestampFormat => TimestampFormat::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            TrafficConfigField::Encoding => Encoding::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            TrafficConfigField::WrapMode => WrapMode::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            TrafficConfigField::HexGrouping => HexGrouping::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            // Toggle fields don't have dropdowns
            _ => vec![],
        }
    }

    /// Get the current index for dropdown selection
    pub fn get_current_config_index(&self) -> usize {
        match self.config_field {
            TrafficConfigField::TimestampFormat => self.timestamp_format.index(),
            TrafficConfigField::Encoding => self.encoding.index(),
            TrafficConfigField::WrapMode => self.wrap_mode.index(),
            TrafficConfigField::HexGrouping => self.hex_grouping.index(),
            _ => 0,
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config_field {
            TrafficConfigField::TimestampFormat => TimestampFormat::all_variants().len(),
            TrafficConfigField::Encoding => Encoding::all_variants().len(),
            TrafficConfigField::WrapMode => WrapMode::all_variants().len(),
            TrafficConfigField::HexGrouping => HexGrouping::all_variants().len(),
            _ => 0,
        }
    }

    /// Open the dropdown for the current config field
    pub fn open_dropdown(&mut self) {
        self.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value
    pub fn apply_dropdown_selection(&mut self) {
        match self.config_field {
            TrafficConfigField::TimestampFormat => {
                self.timestamp_format = TimestampFormat::from_index(self.dropdown_index);
            }
            TrafficConfigField::Encoding => {
                self.encoding = Encoding::from_index(self.dropdown_index);
            }
            TrafficConfigField::WrapMode => {
                self.wrap_mode = WrapMode::from_index(self.dropdown_index);
            }
            TrafficConfigField::HexGrouping => {
                self.hex_grouping = HexGrouping::from_index(self.dropdown_index);
            }
            _ => {}
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        match self.config_field {
            TrafficConfigField::LineNumbers => self.show_line_numbers = !self.show_line_numbers,
            TrafficConfigField::Timestamps => self.show_timestamps = !self.show_timestamps,
            TrafficConfigField::AutoScroll => self.auto_scroll = !self.auto_scroll,
            TrafficConfigField::LockToBottom => self.lock_to_bottom = !self.lock_to_bottom,
            TrafficConfigField::ShowTx => self.show_tx = !self.show_tx,
            TrafficConfigField::ShowRx => self.show_rx = !self.show_rx,
            _ => {}
        }
    }

    /// Check if we're currently at the bottom of the scroll
    pub fn is_at_bottom(&self) -> bool {
        if self.total_rows == 0 {
            return true;
        }
        let max_scroll = self.total_rows.saturating_sub(self.visible_height);
        self.scroll_offset >= max_scroll
    }
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
            InputMode::TrafficConfigDropdown => self.handle_key_traffic_config_dropdown(key),
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
        let config_visible = self.traffic.config_panel_visible;
        let config_focused = self.traffic.focus == TrafficFocus::Config;
        let Some(cmd) = map_traffic_key(key, config_visible, config_focused) else {
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
                // User is scrolling up, disable auto-scroll tracking
                self.traffic.was_at_bottom = false;
                self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_sub(1);
            }
            TrafficCommand::ScrollDown => {
                self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_add(1);
            }
            TrafficCommand::ScrollToTop => {
                self.traffic.was_at_bottom = false;
                self.traffic.scroll_offset = 0;
            }
            TrafficCommand::ScrollToBottom => {
                self.traffic.was_at_bottom = true;
                self.traffic.scroll_offset = usize::MAX;
            }
            TrafficCommand::PageUp => {
                self.traffic.was_at_bottom = false;
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
            TrafficCommand::ToggleConfigPanel => {
                self.traffic.config_panel_visible = !self.traffic.config_panel_visible;
                if !self.traffic.config_panel_visible {
                    self.traffic.focus = TrafficFocus::Traffic;
                }
                self.needs_full_clear = true;
            }
            TrafficCommand::FocusTraffic => {
                self.traffic.focus = TrafficFocus::Traffic;
            }
            TrafficCommand::FocusConfig => {
                if self.traffic.config_panel_visible {
                    self.traffic.focus = TrafficFocus::Config;
                }
            }
            TrafficCommand::MoveUp => {
                // Only when config panel is focused
                self.traffic.config_field = self.traffic.config_field.prev();
            }
            TrafficCommand::MoveDown => {
                self.traffic.config_field = self.traffic.config_field.next();
            }
            TrafficCommand::Confirm => {
                // Toggle or open dropdown for config field
                if self.traffic.config_field.is_toggle() {
                    self.traffic.toggle_setting();
                    self.status = format!(
                        "{}: {}",
                        self.traffic.config_field.label(),
                        self.traffic.get_config_display(self.traffic.config_field)
                    );
                    self.needs_full_clear = true;
                } else {
                    self.traffic.open_dropdown();
                    self.input.mode = InputMode::TrafficConfigDropdown;
                }
            }
            TrafficCommand::ToggleLineNumbers => {
                self.traffic.show_line_numbers = !self.traffic.show_line_numbers;
                self.status = if self.traffic.show_line_numbers {
                    "Line numbers: ON".to_string()
                } else {
                    "Line numbers: OFF".to_string()
                };
            }
            TrafficCommand::ToggleTimestamps => {
                self.traffic.show_timestamps = !self.traffic.show_timestamps;
                self.status = if self.traffic.show_timestamps {
                    "Timestamps: ON".to_string()
                } else {
                    "Timestamps: OFF".to_string()
                };
            }
            TrafficCommand::EscapeOrClear => {
                if config_focused {
                    // When config panel is focused, Esc returns focus to traffic
                    self.traffic.focus = TrafficFocus::Traffic;
                } else if self.search.pattern.is_some() {
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

    fn handle_key_traffic_config_dropdown(&mut self, key: KeyEvent) {
        let Some(cmd) = map_dropdown_key(key) else {
            return;
        };

        let options_count = self.traffic.get_options_count();

        match cmd {
            DropdownCommand::MoveUp => {
                if self.traffic.dropdown_index > 0 {
                    self.traffic.dropdown_index -= 1;
                }
            }
            DropdownCommand::MoveDown => {
                if self.traffic.dropdown_index < options_count.saturating_sub(1) {
                    self.traffic.dropdown_index += 1;
                }
            }
            DropdownCommand::Confirm => {
                self.traffic.apply_dropdown_selection();
                self.input.mode = InputMode::Normal;
                self.needs_full_clear = true;
                self.status = format!(
                    "{}: {}",
                    self.traffic.config_field.label(),
                    self.traffic.get_config_display(self.traffic.config_field)
                );
            }
            DropdownCommand::Cancel => {
                self.input.mode = InputMode::Normal;
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
                self.traffic.session_start = Some(std::time::SystemTime::now());
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
