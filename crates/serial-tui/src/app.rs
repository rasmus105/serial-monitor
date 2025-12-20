//! Application state and logic

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;
use serial_core::{
    encode, list_ports, send_file, start_file_saver, DataBits, DataChunk, Encoding, FileSendConfig,
    FileSendHandle, FileSendProgress, FileSaveConfig, FileSaverHandle, FlowControl, Parity,
    PortInfo, SaveFormat, SerialConfig, Session, SessionEvent, SessionHandle, StopBits,
};
use strum::{EnumCount, EnumIter, IntoEnumIterator};

use crate::command::{map_global_nav_key, DropdownCommand, GlobalNavCommand, PortSelectCommand, TrafficCommand};
use crate::settings::{
    key_event_to_binding, map_settings_key, Settings, SettingsCommand, SettingsPanelState,
    SettingsTab,
};
use crate::ui::format_hex_grouped;

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

impl ConfigOption for SaveFormat {
    fn all_variants() -> &'static [Self] {
        SaveFormat::all()
    }

    fn display_name(&self) -> &'static str {
        match self {
            SaveFormat::Utf8 => "UTF-8",
            SaveFormat::Ascii => "ASCII",
            SaveFormat::Hex => "HEX",
            SaveFormat::Raw => "Raw",
        }
    }
}

// =============================================================================
// Enums
// =============================================================================

/// Current view/screen (pre-connection vs post-connection)
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Port selection screen (pre-connection)
    PortSelect,
    /// Connected view with tabs (post-connection)
    Connected,
}

// =============================================================================
// Tab & Split System
// =============================================================================

/// Content types that can be displayed in a pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneContent {
    /// Serial traffic monitor
    #[default]
    Traffic,
    /// Graph view for numeric data
    Graph,
    /// Advanced send options - file sending, macros, etc.
    AdvancedSend,
}

impl PaneContent {
    /// Get the tab number (1-indexed for display)
    pub fn tab_number(&self) -> u8 {
        match self {
            PaneContent::Traffic => 1,
            PaneContent::Graph => 2,
            PaneContent::AdvancedSend => 3,
        }
    }

    /// Get the display name
    pub fn display_name(&self) -> &'static str {
        match self {
            PaneContent::Traffic => "Traffic",
            PaneContent::Graph => "Graph",
            PaneContent::AdvancedSend => "Send",
        }
    }

    /// Create from tab number (1-indexed)
    pub fn from_tab_number(n: u8) -> Option<Self> {
        match n {
            1 => Some(PaneContent::Traffic),
            2 => Some(PaneContent::Graph),
            3 => Some(PaneContent::AdvancedSend),
            _ => None,
        }
    }

    /// Get the available split options for this content as primary
    /// Returns content types that can be shown in the secondary pane
    pub fn available_splits(&self) -> &'static [PaneContent] {
        match self {
            PaneContent::Traffic => &[PaneContent::Graph, PaneContent::AdvancedSend],
            PaneContent::Graph => &[PaneContent::Traffic, PaneContent::AdvancedSend],
            PaneContent::AdvancedSend => &[PaneContent::Traffic, PaneContent::Graph],
        }
    }
}

/// Which pane is focused in a split layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneFocus {
    /// Primary pane (left side)
    #[default]
    Primary,
    /// Secondary pane (right side)
    Secondary,
}

/// State for a single tab (workspace)
#[derive(Debug, Clone, Default)]
pub struct TabState {
    /// Secondary pane content (if split is active)
    pub secondary: Option<PaneContent>,
    /// Which pane has focus within this tab
    pub focus: PaneFocus,
    /// Split ratio (percentage for primary pane width)
    pub split_ratio: u16,
}

impl TabState {
    /// Create a new tab state (no split)
    pub fn new() -> Self {
        Self {
            secondary: None,
            focus: PaneFocus::Primary,
            split_ratio: 50,
        }
    }

    /// Check if there's a split
    pub fn is_split(&self) -> bool {
        self.secondary.is_some()
    }
}

/// Number of tabs in the connected view
pub const TAB_COUNT: usize = 3;

/// Layout state for the connected view with tabs
/// Each tab has its own split configuration
#[derive(Debug, Clone)]
pub struct TabLayout {
    /// Current active tab (0-indexed internally, displayed as 1-indexed)
    active_tab: usize,
    /// State for each tab (index 0=Traffic, 1=Graph, 2=Send)
    tabs: [TabState; TAB_COUNT],
}

impl Default for TabLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TabLayout {
    /// Create a new tab layout (start on Tab 1)
    pub fn new() -> Self {
        Self {
            active_tab: 0,
            tabs: [TabState::new(), TabState::new(), TabState::new()],
        }
    }

    /// Get the active tab number (1-indexed for display)
    pub fn active_tab_number(&self) -> u8 {
        (self.active_tab + 1) as u8
    }

    /// Get the primary content for the active tab
    pub fn primary_content(&self) -> PaneContent {
        PaneContent::from_tab_number(self.active_tab_number()).unwrap_or_default()
    }

    /// Get the active tab's state
    fn active_state(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    /// Get mutable reference to the active tab's state
    fn active_state_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Switch to a specific tab (1-indexed: 1, 2, or 3)
    pub fn switch_tab(&mut self, tab: u8) {
        let idx = (tab as usize).saturating_sub(1);
        if idx < TAB_COUNT {
            self.active_tab = idx;
        }
    }

    /// Check if the active tab has a split
    pub fn is_split(&self) -> bool {
        self.active_state().is_split()
    }

    /// Get the secondary content of the active tab (if any)
    pub fn secondary(&self) -> Option<PaneContent> {
        self.active_state().secondary
    }

    /// Get the focus of the active tab
    pub fn focus(&self) -> PaneFocus {
        self.active_state().focus
    }

    /// Add a vertical split with the given content to the active tab
    /// Returns error message if split cannot be created
    pub fn vsplit(&mut self, content: PaneContent) -> Result<(), &'static str> {
        let primary = self.primary_content();
        if content == primary {
            return Err("Cannot split with the same content as primary");
        }
        let state = self.active_state_mut();
        if state.secondary.is_some() {
            return Err("Already split - close the secondary pane first");
        }
        state.secondary = Some(content);
        state.focus = PaneFocus::Secondary;
        Ok(())
    }

    /// Close the secondary pane of the active tab
    pub fn close_secondary(&mut self) -> Result<(), &'static str> {
        let state = self.active_state_mut();
        if state.secondary.is_none() {
            return Err("No secondary pane to close");
        }
        state.secondary = None;
        state.focus = PaneFocus::Primary;
        Ok(())
    }

    /// Get the currently focused pane's content
    pub fn focused_content(&self) -> PaneContent {
        let state = self.active_state();
        match state.focus {
            PaneFocus::Primary => self.primary_content(),
            PaneFocus::Secondary => state.secondary.unwrap_or(self.primary_content()),
        }
    }

    /// Toggle focus between panes in the active tab
    pub fn toggle_focus(&mut self) {
        let state = self.active_state_mut();
        if state.secondary.is_some() {
            state.focus = match state.focus {
                PaneFocus::Primary => PaneFocus::Secondary,
                PaneFocus::Secondary => PaneFocus::Primary,
            };
        }
    }

    /// Move focus left in the active tab
    pub fn focus_left(&mut self) {
        let state = self.active_state_mut();
        if state.focus == PaneFocus::Secondary {
            state.focus = PaneFocus::Primary;
        }
    }

    /// Move focus right in the active tab
    pub fn focus_right(&mut self) {
        let state = self.active_state_mut();
        if state.focus == PaneFocus::Primary && state.secondary.is_some() {
            state.focus = PaneFocus::Secondary;
        }
    }

    /// Get the split ratio of the active tab
    pub fn split_ratio(&self) -> u16 {
        self.active_state().split_ratio
    }
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
    // File saving fields
    SaveEnabled,
    SaveFormat,
    SaveFilename,
    SaveDirectory,
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
    // File saving fields
    SaveEnabled,
    SaveFormat,
    SaveFilename,
    SaveDirectory,
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
            TrafficConfigField::SaveEnabled => "Save to File",
            TrafficConfigField::SaveFormat => "Save Format",
            TrafficConfigField::SaveFilename => "Filename",
            TrafficConfigField::SaveDirectory => "Directory",
        }
    }

    /// Whether this field is a simple toggle (vs a dropdown or text input)
    pub fn is_toggle(&self) -> bool {
        matches!(
            self,
            TrafficConfigField::LineNumbers
                | TrafficConfigField::Timestamps
                | TrafficConfigField::AutoScroll
                | TrafficConfigField::LockToBottom
                | TrafficConfigField::ShowTx
                | TrafficConfigField::ShowRx
                | TrafficConfigField::SaveEnabled
        )
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            TrafficConfigField::SaveFilename | TrafficConfigField::SaveDirectory
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

    /// Check if this is a file saving field (for section grouping)
    pub fn is_file_saving_field(&self) -> bool {
        matches!(
            self,
            TrafficConfigField::SaveEnabled
                | TrafficConfigField::SaveFormat
                | TrafficConfigField::SaveFilename
                | TrafficConfigField::SaveDirectory
        )
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
            ConfigField::SaveEnabled => "Save to File",
            ConfigField::SaveFormat => "Save Format",
            ConfigField::SaveFilename => "Filename",
            ConfigField::SaveDirectory => "Directory",
        }
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            ConfigField::SaveFilename | ConfigField::SaveDirectory
        )
    }

    /// Whether this field is a simple toggle
    pub fn is_toggle(&self) -> bool {
        matches!(self, ConfigField::SaveEnabled)
    }

    /// Check if this is a file saving field (for section grouping)
    pub fn is_file_saving_field(&self) -> bool {
        matches!(
            self,
            ConfigField::SaveEnabled
                | ConfigField::SaveFormat
                | ConfigField::SaveFilename
                | ConfigField::SaveDirectory
        )
    }

    /// Check if this is a serial config field
    pub fn is_serial_field(&self) -> bool {
        !self.is_file_saving_field()
    }
}

/// Which file saving configuration field is selected in port selection config panel
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount)]
pub enum FileSaveConfigField {
    #[default]
    SaveFormat,
    SaveFilename,
    SaveDirectory,
}

impl FileSaveConfigField {
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
            FileSaveConfigField::SaveFormat => "Save Format",
            FileSaveConfigField::SaveFilename => "Filename",
            FileSaveConfigField::SaveDirectory => "Directory",
        }
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            FileSaveConfigField::SaveFilename | FileSaveConfigField::SaveDirectory
        )
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
    /// Settings dropdown is open (General tab)
    SettingsDropdown,
    /// Waiting for window command after Ctrl+W
    WindowCommand,
    /// Command line mode (after pressing :)
    CommandLine,
    /// Split selection mode (choosing which content to split with)
    SplitSelect,
    /// Editing a config text field (port selection)
    ConfigTextInput,
    /// Editing a traffic config text field
    TrafficConfigTextInput,
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
            InputMode::SettingsDropdown => "j/k: navigate, Enter: select, Esc: cancel",
            InputMode::WindowCommand => "Ctrl+W: v=vsplit, q=close, h/l=navigate",
            InputMode::CommandLine => "",
            InputMode::SplitSelect => "", // Dynamic based on available splits
            InputMode::ConfigTextInput => "Enter value (Enter: confirm, Esc: cancel)",
            InputMode::TrafficConfigTextInput => "Enter value (Enter: confirm, Esc: cancel)",
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
            InputMode::SettingsDropdown => None,       // Uses special rendering
            InputMode::WindowCommand => None,          // Uses status bar message
            InputMode::CommandLine => Some(InputModeStyle {
                prefix: ":",
                color: Color::Yellow,
            }),
            InputMode::SplitSelect => None,            // Uses status bar message
            InputMode::ConfigTextInput => Some(InputModeStyle {
                prefix: "",
                color: Color::Cyan,
            }),
            InputMode::TrafficConfigTextInput => Some(InputModeStyle {
                prefix: "",
                color: Color::Cyan,
            }),
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
    // File saving configuration (pre-connection)
    /// Whether file saving should start on connect
    pub save_enabled: bool,
    /// Save format
    pub save_format: SaveFormat,
    /// Custom filename (None = auto-generated)
    pub save_filename: String,
    /// Save directory
    pub save_directory: String,
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
            save_enabled: false,
            save_format: SaveFormat::default(),
            save_filename: String::new(), // Empty = auto-generated
            save_directory: default_save_directory(),
        }
    }
}

/// Get the default save directory (user's home directory or current directory)
fn default_save_directory() -> String {
    std::env::var("HOME")
        .map(|h| format!("{}/serial-logs", h))
        .unwrap_or_else(|_| ".".to_string())
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
            ConfigField::SaveFormat => SaveFormat::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            // Toggle and text input fields don't have dropdown options
            ConfigField::SaveEnabled | ConfigField::SaveFilename | ConfigField::SaveDirectory => vec![],
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
            ConfigField::SaveFormat => self.save_format.index(),
            ConfigField::SaveEnabled | ConfigField::SaveFilename | ConfigField::SaveDirectory => 0,
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
            ConfigField::SaveEnabled => {
                if self.save_enabled { "ON" } else { "OFF" }.to_string()
            }
            ConfigField::SaveFormat => self.save_format.display_name().to_string(),
            ConfigField::SaveFilename => {
                if self.save_filename.is_empty() {
                    "(auto)".to_string()
                } else {
                    self.save_filename.clone()
                }
            }
            ConfigField::SaveDirectory => self.save_directory.clone(),
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
            ConfigField::SaveFormat => {
                self.save_format = SaveFormat::from_index(self.dropdown_index);
            }
            // Toggle and text input fields don't use dropdown
            ConfigField::SaveEnabled | ConfigField::SaveFilename | ConfigField::SaveDirectory => {}
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
            ConfigField::SaveFormat => SaveFormat::all_variants().len(),
            ConfigField::SaveEnabled | ConfigField::SaveFilename | ConfigField::SaveDirectory => 0,
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        match self.config_field {
            ConfigField::SaveEnabled => self.save_enabled = !self.save_enabled,
            _ => {}
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config_field {
            ConfigField::SaveFilename => {
                self.save_filename = value;
            }
            ConfigField::SaveDirectory => {
                self.save_directory = value;
            }
            _ => {}
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config_field {
            ConfigField::SaveFilename => self.save_filename.clone(),
            ConfigField::SaveDirectory => self.save_directory.clone(),
            _ => String::new(),
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
                // Always produce 7 characters: "+XXXX.Xs" pattern
                // Decrease decimal precision as integer part grows
                if secs < 10.0 {
                    format!("+{:05.3}s", secs)    // +1.234s (7 chars)
                } else if secs < 100.0 {
                    format!("+{:05.2}s", secs)    // +12.34s (7 chars)
                } else if secs < 1000.0 {
                    format!("+{:05.1}s", secs)    // +123.4s (7 chars)
                } else if secs < 10000.0 {
                    format!("+{:.1}s", secs)      // +1234.5s (8 chars)
                } else if secs < 100000.0 {
                    format!("+{:.0}s", secs)      // +12345s (7 chars)
                } else {
                    // For very long sessions (27+ hours), just show the number
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
            TimestampFormat::Relative => 9,        // "+1234.5s " - max reasonable width (up to ~3 hours)
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
    /// Whether quit confirmation dialog is showing
    pub quit_confirm: bool,
    // File saving state
    /// Whether file saving is enabled
    pub save_enabled: bool,
    /// Save format
    pub save_format: SaveFormat,
    /// Custom filename (empty = auto-generated)
    pub save_filename: String,
    /// Save directory
    pub save_directory: String,
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
            quit_confirm: false,
            // File saving defaults
            save_enabled: false,
            save_format: SaveFormat::default(),
            save_filename: String::new(),
            save_directory: default_save_directory(),
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
            TrafficConfigField::SaveEnabled => {
                if self.save_enabled { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::SaveFormat => self.save_format.display_name().to_string(),
            TrafficConfigField::SaveFilename => {
                if self.save_filename.is_empty() {
                    "(auto)".to_string()
                } else {
                    self.save_filename.clone()
                }
            }
            TrafficConfigField::SaveDirectory => self.save_directory.clone(),
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
            TrafficConfigField::SaveFormat => SaveFormat::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            // Toggle and text input fields don't have dropdowns
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
            TrafficConfigField::SaveFormat => self.save_format.index(),
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
            TrafficConfigField::SaveFormat => SaveFormat::all_variants().len(),
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
            TrafficConfigField::SaveFormat => {
                self.save_format = SaveFormat::from_index(self.dropdown_index);
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
            TrafficConfigField::SaveEnabled => self.save_enabled = !self.save_enabled,
            _ => {}
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config_field {
            TrafficConfigField::SaveFilename => {
                self.save_filename = value;
            }
            TrafficConfigField::SaveDirectory => {
                self.save_directory = value;
            }
            _ => {}
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config_field {
            TrafficConfigField::SaveFilename => self.save_filename.clone(),
            TrafficConfigField::SaveDirectory => self.save_directory.clone(),
            _ => String::new(),
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

/// A single search match occurrence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Index of the chunk containing this match
    pub chunk_index: usize,
    /// Byte offset where the match starts within the encoded content
    pub byte_start: usize,
    /// Byte offset where the match ends within the encoded content
    pub byte_end: usize,
}

/// Search mode - determines how the search pattern is interpreted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Regex search (default) - pattern is interpreted as a regular expression
    #[default]
    Regex,
    /// Normal search - pattern is interpreted as a literal string (case-insensitive)
    Normal,
}

impl SearchMode {
    pub fn name(&self) -> &'static str {
        match self {
            SearchMode::Regex => "Regex",
            SearchMode::Normal => "Normal",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            SearchMode::Regex => "Pattern is interpreted as a regular expression",
            SearchMode::Normal => "Pattern is interpreted as a literal string (case-insensitive)",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            SearchMode::Regex => SearchMode::Normal,
            SearchMode::Normal => SearchMode::Regex,
        }
    }
}

/// State for search functionality
#[derive(Debug, Default)]
pub struct SearchState {
    /// Current search pattern (if any)
    pub pattern: Option<String>,
    /// All match occurrences found
    pub matches: Vec<SearchMatch>,
    /// Index into `matches` for the current match (0-based)
    pub current_match: Option<usize>,
    /// Search mode (regex or normal)
    pub mode: SearchMode,
    /// Error message from regex compilation (if any)
    pub error: Option<String>,
}

impl SearchState {
    /// Clear search state
    pub fn clear(&mut self) {
        self.pattern = None;
        self.matches.clear();
        self.current_match = None;
        self.error = None;
    }

    /// Get the total number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get the current match (if any)
    pub fn current(&self) -> Option<&SearchMatch> {
        self.current_match.and_then(|idx| self.matches.get(idx))
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
    /// Tab layout for connected view (with persistent splits per tab)
    pub layout: TabLayout,
    /// Traffic view state
    pub traffic: TrafficState,
    /// Search state
    pub search: SearchState,
    /// File send state
    pub file_send: FileSendState,
    /// Application settings (including keybindings)
    pub settings: Settings,
    /// Settings panel state
    pub settings_panel: SettingsPanelState,
    /// File saver handle (if saving is active)
    pub file_saver: Option<FileSaverHandle>,

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
            layout: TabLayout::new(),
            traffic: TrafficState::default(),
            search: SearchState::default(),
            file_send: FileSendState::default(),
            settings: Settings::default(),
            settings_panel: SettingsPanelState::default(),
            file_saver: None,
            runtime,
        }
    }

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) {
        self.status = self.port_select.refresh_ports();
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Settings panel takes priority when open
        if self.settings_panel.open {
            self.handle_key_settings(key);
            return;
        }

        // Check for settings toggle key (? works everywhere)
        if key.code == KeyCode::Char('?') {
            self.settings_panel.open();
            self.needs_full_clear = true;
            return;
        }

        match self.input.mode {
            InputMode::Normal => match self.view {
                View::PortSelect => self.handle_key_port_select(key),
                View::Connected => self.handle_key_connected(key),
            },
            InputMode::PortInput => self.handle_key_port_input(key),
            InputMode::SendInput => self.handle_key_send_input(key),
            InputMode::SearchInput => self.handle_key_search_input(key),
            InputMode::FilePathInput => self.handle_key_file_path_input(key),
            InputMode::ConfigDropdown => self.handle_key_config_dropdown(key),
            InputMode::TrafficConfigDropdown => self.handle_key_traffic_config_dropdown(key),
            InputMode::SettingsDropdown => self.handle_key_settings_dropdown(key),
            InputMode::WindowCommand => self.handle_key_window_command(key),
            InputMode::CommandLine => self.handle_key_command_line(key),
            InputMode::SplitSelect => self.handle_key_split_select(key),
            InputMode::ConfigTextInput => self.handle_key_config_text_input(key),
            InputMode::TrafficConfigTextInput => self.handle_key_traffic_config_text_input(key),
        }
    }

    fn handle_key_settings(&mut self, key: KeyEvent) {
        // If recording a key binding, capture the key
        if self.settings_panel.recording_key {
            // Escape cancels recording
            if key.code == KeyCode::Esc {
                self.settings_panel.stop_recording();
                self.status = "Key binding cancelled.".to_string();
                return;
            }

            // Record the binding
            let binding = key_event_to_binding(&key);
            if let Some(cmd) = self.settings_panel.selected_any_command() {
                if let Some(edit_idx) = self.settings_panel.editing_binding_index {
                    // Replace existing binding
                    let mut bindings = self.settings.get_bindings(cmd);
                    if edit_idx < bindings.len() {
                        bindings[edit_idx] = binding;
                        self.settings.set_bindings(cmd, bindings);
                    }
                } else {
                    // Add new binding
                    self.settings.add_binding(cmd, binding);
                }
                self.status = format!("Added binding: {}", binding.display());
            }
            self.settings_panel.stop_recording();
            return;
        }

        // Use visible height for scroll calculations (approximate, will be set properly by render)
        let visible_height = self.settings_panel_visible_height();

        // Handle General tab separately - it has simpler controls
        if self.settings_panel.tab == SettingsTab::General {
            match key.code {
                KeyCode::Esc => {
                    self.settings_panel.close();
                    self.needs_full_clear = true;
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    // Open dropdown for search mode selection
                    self.settings_panel.dropdown_index = match self.search.mode {
                        SearchMode::Regex => 0,
                        SearchMode::Normal => 1,
                    };
                    self.input.mode = InputMode::SettingsDropdown;
                }
                KeyCode::Char('l') | KeyCode::Tab => {
                    self.settings_panel.tab = self.settings_panel.tab.next();
                }
                KeyCode::Char('h') | KeyCode::BackTab => {
                    self.settings_panel.tab = self.settings_panel.tab.prev();
                }
                _ => {}
            }
            return;
        }

        // First check for global navigation commands (j/k, Ctrl+u/d, etc.)
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    self.settings_panel.move_up(visible_height);
                    return;
                }
                GlobalNavCommand::Down => {
                    self.settings_panel.move_down(visible_height);
                    return;
                }
                GlobalNavCommand::PageUp => {
                    self.settings_panel.page_up(visible_height);
                    return;
                }
                GlobalNavCommand::PageDown => {
                    self.settings_panel.page_down(visible_height);
                    return;
                }
                GlobalNavCommand::Confirm => {
                    // Start recording to add a binding
                    self.settings_panel.start_recording();
                    self.status = "Press a key to add binding (Esc to cancel)...".to_string();
                    return;
                }
                GlobalNavCommand::Cancel => {
                    self.settings_panel.close();
                    self.needs_full_clear = true;
                    return;
                }
                GlobalNavCommand::Top => {
                    self.settings_panel.go_to_top();
                    return;
                }
                GlobalNavCommand::Bottom => {
                    self.settings_panel.go_to_bottom(visible_height);
                    return;
                }
            }
        }

        // Then check for settings-specific commands
        let Some(cmd) = map_settings_key(&key) else {
            return;
        };

        match cmd {
            SettingsCommand::Close => {
                self.settings_panel.close();
                self.needs_full_clear = true;
            }
            SettingsCommand::NextTab => {
                self.settings_panel.tab = self.settings_panel.tab.next();
            }
            SettingsCommand::PrevTab => {
                self.settings_panel.tab = self.settings_panel.tab.prev();
            }
            // Navigation is handled by global commands above, but keep as fallback
            SettingsCommand::MoveUp => {
                self.settings_panel.move_up(visible_height);
            }
            SettingsCommand::MoveDown => {
                self.settings_panel.move_down(visible_height);
            }
            SettingsCommand::AddBinding => {
                self.settings_panel.start_recording();
                self.status = "Press a key to add binding (Esc to cancel)...".to_string();
            }
            SettingsCommand::DeleteBinding => {
                // Delete the last binding for the selected command
                if let Some(cmd) = self.settings_panel.selected_any_command() {
                    let bindings = self.settings.get_bindings(cmd);
                    if !bindings.is_empty() {
                        let last = *bindings.last().unwrap();
                        self.settings.remove_binding(cmd, &last);
                        self.status = format!("Removed binding: {}", last.display());
                    } else {
                        self.status = "No bindings to remove.".to_string();
                    }
                }
            }
            SettingsCommand::ResetToDefault => {
                if let Some(cmd) = self.settings_panel.selected_any_command() {
                    self.settings.reset_command(cmd);
                    self.status = format!("Reset {} to defaults.", cmd.name());
                }
            }
            SettingsCommand::Confirm => {
                // Start recording to replace/add a binding
                self.settings_panel.start_recording();
                self.status = "Press a key to add binding (Esc to cancel)...".to_string();
            }
        }
    }

    /// Get approximate visible height for settings panel
    /// This is used for scroll calculations before rendering
    fn settings_panel_visible_height(&self) -> usize {
        // Approximate: 80% of terminal height minus borders/tabs/help
        // A more accurate value gets set during rendering
        20
    }

    fn handle_key_port_select(&mut self, key: KeyEvent) {
        // First check global navigation commands
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    match self.port_select.focus {
                        PortSelectFocus::PortList => {
                            if self.port_select.selected_port > 0 {
                                self.port_select.selected_port -= 1;
                            }
                        }
                        PortSelectFocus::Config => {
                            self.port_select.config_field = self.port_select.config_field.prev();
                        }
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    match self.port_select.focus {
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
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    match self.port_select.focus {
                        PortSelectFocus::PortList => {
                            if !self.port_select.ports.is_empty() {
                                self.connect_to_selected_port();
                            }
                        }
                        PortSelectFocus::Config => {
                            // Check if it's a toggle field
                            if self.port_select.config_field.is_toggle() {
                                self.port_select.toggle_setting();
                                self.status = format!(
                                    "{}: {}",
                                    self.port_select.config_field.label(),
                                    self.port_select.get_config_display(self.port_select.config_field)
                                );
                            } else if self.port_select.config_field.is_text_input() {
                                // Text input field
                                self.input.buffer = self.port_select.get_text_value();
                                self.input.mode = InputMode::ConfigTextInput;
                                self.status = InputMode::ConfigTextInput.entry_prompt().to_string();
                            } else {
                                // Dropdown field
                                self.port_select.open_dropdown();
                                self.input.mode = InputMode::ConfigDropdown;
                            }
                        }
                    }
                    return;
                }
                // PageUp/PageDown/Top/Bottom/Cancel not used in port select
                _ => {}
            }
        }

        // Then check context-specific commands
        let cmd = self.settings.keybindings.port_select.find_command(&key);

        // Handle context-sensitive commands
        let cmd = match cmd {
            Some(PortSelectCommand::FocusPortList) if !self.port_select.config_panel_visible => {
                None
            }
            Some(PortSelectCommand::FocusConfig) if !self.port_select.config_panel_visible => None,
            other => other,
        };

        let Some(cmd) = cmd else {
            // Check for command line entry with ':'
            if key.code == KeyCode::Char(':') && key.modifiers.is_empty() {
                self.input.mode = InputMode::CommandLine;
                self.input.buffer.clear();
                self.status = String::new();
                return;
            }
            return;
        };

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
            PortSelectCommand::Confirm => {
                // Handled by global nav above
            }
        }
    }

    fn handle_key_config_dropdown(&mut self, key: KeyEvent) {
        let options_count = self.port_select.get_options_count();

        // Use global navigation for dropdown
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    if self.port_select.dropdown_index > 0 {
                        self.port_select.dropdown_index -= 1;
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    if self.port_select.dropdown_index < options_count - 1 {
                        self.port_select.dropdown_index += 1;
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    self.port_select.apply_dropdown_selection();
                    self.input.mode = InputMode::Normal;
                    return;
                }
                GlobalNavCommand::Cancel => {
                    self.input.mode = InputMode::Normal;
                    return;
                }
                _ => {}
            }
        }

        // Fall back to dropdown-specific bindings
        if let Some(cmd) = self.settings.keybindings.dropdown.find_command(&key) {
            match cmd {
                DropdownCommand::Confirm => {
                    self.port_select.apply_dropdown_selection();
                    self.input.mode = InputMode::Normal;
                }
                DropdownCommand::Cancel => {
                    self.input.mode = InputMode::Normal;
                }
            }
        }
    }

    fn handle_key_settings_dropdown(&mut self, key: KeyEvent) {
        const OPTIONS_COUNT: usize = 2; // Regex, Normal

        // Use global navigation for dropdown
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    if self.settings_panel.dropdown_index > 0 {
                        self.settings_panel.dropdown_index -= 1;
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    if self.settings_panel.dropdown_index < OPTIONS_COUNT - 1 {
                        self.settings_panel.dropdown_index += 1;
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    // Apply selection
                    self.search.mode = match self.settings_panel.dropdown_index {
                        0 => SearchMode::Regex,
                        _ => SearchMode::Normal,
                    };
                    self.status = format!("Search mode: {}", self.search.mode.name());
                    // Re-run search if there's an active pattern
                    if self.search.pattern.is_some() {
                        self.update_search_matches();
                    }
                    self.input.mode = InputMode::Normal;
                    return;
                }
                GlobalNavCommand::Cancel => {
                    self.input.mode = InputMode::Normal;
                    return;
                }
                _ => {}
            }
        }

        // Fall back to dropdown-specific bindings
        if let Some(cmd) = self.settings.keybindings.dropdown.find_command(&key) {
            match cmd {
                DropdownCommand::Confirm => {
                    // Apply selection
                    self.search.mode = match self.settings_panel.dropdown_index {
                        0 => SearchMode::Regex,
                        _ => SearchMode::Normal,
                    };
                    self.status = format!("Search mode: {}", self.search.mode.name());
                    if self.search.pattern.is_some() {
                        self.update_search_matches();
                    }
                    self.input.mode = InputMode::Normal;
                }
                DropdownCommand::Cancel => {
                    self.input.mode = InputMode::Normal;
                }
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
        // Handle quit confirmation dialog first
        if self.traffic.quit_confirm {
            self.handle_key_quit_confirm(key);
            return;
        }

        let config_visible = self.traffic.config_panel_visible;
        let config_focused = self.traffic.focus == TrafficFocus::Config;

        // First check global navigation commands (j/k, Ctrl+u/d, g, G, etc.)
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    if config_focused {
                        // Move up in config panel
                        self.traffic.config_field = self.traffic.config_field.prev();
                    } else {
                        // Scroll up in traffic
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_sub(1);
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    if config_focused {
                        // Move down in config panel
                        self.traffic.config_field = self.traffic.config_field.next();
                    } else {
                        // Scroll down in traffic
                        self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_add(1);
                    }
                    return;
                }
                GlobalNavCommand::Top => {
                    if !config_focused {
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset = 0;
                    }
                    return;
                }
                GlobalNavCommand::Bottom => {
                    if !config_focused {
                        self.traffic.was_at_bottom = true;
                        self.traffic.scroll_offset = usize::MAX;
                    }
                    return;
                }
                GlobalNavCommand::PageUp => {
                    if !config_focused {
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset =
                            self.traffic.scroll_offset.saturating_sub(self.page_size());
                    }
                    return;
                }
                GlobalNavCommand::PageDown => {
                    if !config_focused {
                        self.traffic.scroll_offset =
                            self.traffic.scroll_offset.saturating_add(self.page_size());
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    if config_focused {
                        // Toggle or open dropdown/text input for config field
                        if self.traffic.config_field.is_toggle() {
                            self.handle_traffic_toggle();
                        } else if self.traffic.config_field.is_text_input() {
                            self.input.buffer = self.traffic.get_text_value();
                            self.input.mode = InputMode::TrafficConfigTextInput;
                            self.status = InputMode::TrafficConfigTextInput.entry_prompt().to_string();
                        } else {
                            self.traffic.open_dropdown();
                            self.input.mode = InputMode::TrafficConfigDropdown;
                        }
                    }
                    return;
                }
                GlobalNavCommand::Cancel => {
                    if config_focused {
                        // When config panel is focused, Esc returns focus to traffic
                        self.traffic.focus = TrafficFocus::Traffic;
                    } else if self.search.pattern.is_some() {
                        self.search.clear();
                        self.status = "Search cleared.".to_string();
                    }
                    // If nothing to cancel, Esc does nothing (use 'q' to disconnect)
                    return;
                }
            }
        }

        // Then check context-specific traffic commands
        let cmd = self.settings.keybindings.traffic.find_command(&key);

        // Handle context-sensitive commands
        let cmd = match cmd {
            Some(TrafficCommand::FocusConfig) if !config_visible => None,
            other => other,
        };

        let Some(cmd) = cmd else { return };

        match cmd {
            TrafficCommand::Disconnect => {
                self.traffic.quit_confirm = true;
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
                if self.traffic.config_panel_visible {
                    // Focus the config panel when opening it
                    self.traffic.focus = TrafficFocus::Config;
                } else {
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
        }
    }

    /// Handle key events in the connected view
    fn handle_key_connected(&mut self, key: KeyEvent) {
        // Handle quit confirmation dialog first
        if self.traffic.quit_confirm {
            self.handle_key_quit_confirm(key);
            return;
        }

        // Check for Ctrl+w prefix for window commands
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
            self.input.mode = InputMode::WindowCommand;
            self.status = "Ctrl+W: v=vsplit, q=close, h/l=navigate".to_string();
            return;
        }

        // Check for command line entry
        if key.modifiers.is_empty() && key.code == KeyCode::Char(':') {
            self.input.mode = InputMode::CommandLine;
            self.input.buffer.clear();
            self.status = String::new();
            return;
        }

        // Tab switching with number keys (1, 2, 3) - switches to that tab
        if key.modifiers.is_empty() {
            match key.code {
                KeyCode::Char('1') => {
                    self.layout.switch_tab(1);
                    self.needs_full_clear = true;
                    self.status = "Tab 1: Traffic".to_string();
                    return;
                }
                KeyCode::Char('2') => {
                    self.layout.switch_tab(2);
                    self.needs_full_clear = true;
                    self.status = "Tab 2: Graph".to_string();
                    return;
                }
                KeyCode::Char('3') => {
                    self.layout.switch_tab(3);
                    self.needs_full_clear = true;
                    self.status = "Tab 3: Send".to_string();
                    return;
                }
                // h/l for pane navigation (includes config panel)
                KeyCode::Char('h') => {
                    if self.navigate_focus_left() {
                        self.update_focus_status();
                        return;
                    }
                }
                KeyCode::Char('l') => {
                    if self.navigate_focus_right() {
                        self.update_focus_status();
                        return;
                    }
                }
                _ => {}
            }
        }

        // Tab key cycles focus within the current tab
        if key.code == KeyCode::Tab && key.modifiers.is_empty() {
            self.layout.toggle_focus();
            self.update_focus_status();
            return;
        }

        // If config panel is focused, handle config navigation first
        // This takes priority over pane-specific handlers
        if self.traffic.config_panel_visible
            && self.traffic.focus == TrafficFocus::Config
            && self.handle_key_config_panel(key)
        {
            return;
        }

        // Delegate to content-specific handler based on focused pane
        match self.layout.focused_content() {
            PaneContent::Traffic => self.handle_key_traffic(key),
            PaneContent::Graph => self.handle_key_graph(key),
            PaneContent::AdvancedSend => self.handle_key_advanced_send(key),
        }
    }

    /// Handle key events when config panel is focused (works from any pane)
    /// Returns true if the key was handled
    fn handle_key_config_panel(&mut self, key: KeyEvent) -> bool {
        // Global navigation for config panel
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    self.traffic.config_field = self.traffic.config_field.prev();
                    return true;
                }
                GlobalNavCommand::Down => {
                    self.traffic.config_field = self.traffic.config_field.next();
                    return true;
                }
                GlobalNavCommand::Confirm => {
                    if self.traffic.config_field.is_toggle() {
                        self.handle_traffic_toggle();
                    } else if self.traffic.config_field.is_text_input() {
                        self.input.buffer = self.traffic.get_text_value();
                        self.input.mode = InputMode::TrafficConfigTextInput;
                        self.status = InputMode::TrafficConfigTextInput.entry_prompt().to_string();
                    } else {
                        self.traffic.open_dropdown();
                        self.input.mode = InputMode::TrafficConfigDropdown;
                    }
                    return true;
                }
                GlobalNavCommand::Cancel => {
                    // Return focus to the pane
                    self.traffic.focus = TrafficFocus::Traffic;
                    self.update_focus_status();
                    return true;
                }
                // PageUp/PageDown/Top/Bottom not used in config panel
                _ => {}
            }
        }

        // 'c' closes config panel
        if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
            self.traffic.config_panel_visible = false;
            self.traffic.focus = TrafficFocus::Traffic;
            self.needs_full_clear = true;
            return true;
        }

        false
    }

    /// Handle window/split commands (after Ctrl+W prefix)
    fn handle_key_window_command(&mut self, key: KeyEvent) {
        self.input.mode = InputMode::Normal;

        match key.code {
            // Vertical split - show split selection prompt
            KeyCode::Char('v') => {
                if self.layout.is_split() {
                    self.status = "Already split - close with Ctrl+W q first".to_string();
                } else {
                    // Enter split selection mode
                    self.input.mode = InputMode::SplitSelect;
                    let primary = self.layout.primary_content();
                    let options = primary.available_splits();
                    let prompt = options
                        .iter()
                        .map(|c| format!("[{}] {}", c.tab_number(), c.display_name()))
                        .collect::<Vec<_>>()
                        .join("  ");
                    self.status = format!("Split with: {}  [Esc: cancel]", prompt);
                }
            }
            // Close secondary pane
            KeyCode::Char('q') => {
                match self.layout.close_secondary() {
                    Ok(()) => {
                        self.needs_full_clear = true;
                        self.status = "Closed secondary pane".to_string();
                    }
                    Err(msg) => {
                        self.status = msg.to_string();
                    }
                }
            }
            // Navigation between panes
            KeyCode::Char('h') => {
                self.layout.focus_left();
                self.update_focus_status();
            }
            KeyCode::Char('l') => {
                self.layout.focus_right();
                self.update_focus_status();
            }
            // Cycle focus with Tab or w
            KeyCode::Char('w') | KeyCode::Tab => {
                self.layout.toggle_focus();
                self.update_focus_status();
            }
            // Cancel
            KeyCode::Esc => {
                self.status = "Window command cancelled".to_string();
            }
            _ => {
                self.status = "Unknown window command (v=vsplit, q=close, h/l=nav)".to_string();
            }
        }
    }

    /// Handle split selection mode (choosing which content to split with)
    fn handle_key_split_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c @ '1'..='3') => {
                self.input.mode = InputMode::Normal;
                let tab_num = c.to_digit(10).unwrap() as u8;
                if let Some(content) = PaneContent::from_tab_number(tab_num) {
                    match self.layout.vsplit(content) {
                        Ok(()) => {
                            self.needs_full_clear = true;
                            self.status = format!("Split with {}", content.display_name());
                        }
                        Err(msg) => {
                            self.status = msg.to_string();
                        }
                    }
                }
            }
            KeyCode::Esc => {
                self.input.mode = InputMode::Normal;
                self.status = "Split cancelled".to_string();
            }
            _ => {
                // Invalid selection, show options again
                let primary = self.layout.primary_content();
                let options = primary.available_splits();
                let prompt = options
                    .iter()
                    .map(|c| format!("[{}] {}", c.tab_number(), c.display_name()))
                    .collect::<Vec<_>>()
                    .join("  ");
                self.status = format!("Split with: {}  [Esc: cancel]", prompt);
            }
        }
    }

    fn update_focus_status(&mut self) {
        if self.traffic.config_panel_visible && self.traffic.focus == TrafficFocus::Config {
            self.status = "Focus: Config".to_string();
        } else {
            let content = self.layout.focused_content();
            let pane_indicator = if self.layout.is_split() {
                match self.layout.focus() {
                    PaneFocus::Primary => " (Primary)",
                    PaneFocus::Secondary => " (Secondary)",
                }
            } else {
                ""
            };
            self.status = format!("Focus: {}{}", content.display_name(), pane_indicator);
        }
    }

    /// Navigate focus left: Config -> Secondary -> Primary
    /// Returns true if focus changed
    fn navigate_focus_left(&mut self) -> bool {
        // If config panel is focused, move to the rightmost pane
        if self.traffic.config_panel_visible && self.traffic.focus == TrafficFocus::Config {
            self.traffic.focus = TrafficFocus::Traffic;
            // If split, focus the secondary (rightmost) pane
            if self.layout.is_split() {
                self.layout.active_state_mut().focus = PaneFocus::Secondary;
            }
            return true;
        }

        // If in split view, try to move left between panes
        if self.layout.is_split() && self.layout.focus() == PaneFocus::Secondary {
            self.layout.focus_left();
            return true;
        }

        // Already at leftmost position
        false
    }

    /// Navigate focus right: Primary -> Secondary -> Config
    /// Returns true if focus changed
    fn navigate_focus_right(&mut self) -> bool {
        // If config panel is visible and we're on traffic side
        if self.traffic.focus == TrafficFocus::Traffic {
            // If split, check if we're on secondary pane
            if self.layout.is_split() {
                if self.layout.focus() == PaneFocus::Primary {
                    // Move to secondary pane
                    self.layout.focus_right();
                    return true;
                } else if self.traffic.config_panel_visible {
                    // Already on secondary, move to config
                    self.traffic.focus = TrafficFocus::Config;
                    return true;
                }
            } else if self.traffic.config_panel_visible {
                // No split, move directly to config
                self.traffic.focus = TrafficFocus::Config;
                return true;
            }
        }

        // Already at rightmost position
        false
    }

    /// Handle common key events for placeholder panes (Graph, AdvancedSend).
    /// These panes share basic functionality until fully implemented.
    /// Returns true if the key was handled, false otherwise.
    fn handle_key_placeholder_pane(&mut self, key: KeyEvent) -> bool {
        // Check for disconnect command
        let cmd = self.settings.keybindings.traffic.find_command(&key);
        if let Some(TrafficCommand::Disconnect) = cmd {
            self.traffic.quit_confirm = true;
            return true;
        }

        // Toggle config panel with 'c'
        if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
            self.traffic.config_panel_visible = !self.traffic.config_panel_visible;
            if self.traffic.config_panel_visible {
                self.traffic.focus = TrafficFocus::Config;
            } else {
                self.traffic.focus = TrafficFocus::Traffic;
            }
            self.needs_full_clear = true;
            return true;
        }

        false
    }

    /// Handle key events for graph pane (placeholder)
    fn handle_key_graph(&mut self, key: KeyEvent) {
        // Use shared placeholder handler for common functionality
        if self.handle_key_placeholder_pane(key) {
            // Handled by shared handler
        }
        // Graph-specific keybindings will go here
    }

    /// Handle key events for advanced send pane (placeholder)
    fn handle_key_advanced_send(&mut self, key: KeyEvent) {
        // Use shared placeholder handler for common functionality
        if self.handle_key_placeholder_pane(key) {
            // Handled by shared handler
        }
        // Send-specific keybindings will go here
    }

    /// Handle command line input (after pressing :)
    fn handle_key_command_line(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(cmd) => {
                self.execute_command(&cmd);
            }
            TextInputResult::Cancel => {
                self.status = "Command cancelled".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    /// Execute a command line command
    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "q" | "quit" => {
                if matches!(self.view, View::Connected) {
                    self.traffic.quit_confirm = true;
                } else {
                    self.should_quit = true;
                }
            }
            "connect" => {
                if parts.len() > 1 {
                    let port_path = parts[1..].join(" ");
                    self.connect_to_port(&port_path);
                } else {
                    self.status = "Usage: :connect <port_path>".to_string();
                }
            }
            "disconnect" => {
                if matches!(self.view, View::Connected) {
                    self.disconnect();
                    self.view = View::PortSelect;
                    self.needs_full_clear = true;
                    self.status = "Disconnected.".to_string();
                } else {
                    self.status = "Not connected".to_string();
                }
            }
            "vsplit" => {
                if !matches!(self.view, View::Connected) {
                    self.status = "Must be connected to use splits".to_string();
                    return;
                }
                if self.layout.is_split() {
                    self.status = "Already split - use :close first".to_string();
                    return;
                }
                if parts.len() > 1 {
                    if let Ok(tab_num) = parts[1].parse::<u8>() {
                        if let Some(content) = PaneContent::from_tab_number(tab_num) {
                            match self.layout.vsplit(content) {
                                Ok(()) => {
                                    self.needs_full_clear = true;
                                    self.status = format!("Split with {}", content.display_name());
                                }
                                Err(msg) => {
                                    self.status = msg.to_string();
                                }
                            }
                        } else {
                            self.status = "Invalid pane number (1=Traffic, 2=Graph, 3=Send)".to_string();
                        }
                    } else {
                        self.status = "Usage: :vsplit [1|2|3]".to_string();
                    }
                } else {
                    // No argument: enter split selection mode
                    self.input.mode = InputMode::SplitSelect;
                    let primary = self.layout.primary_content();
                    let options = primary.available_splits();
                    let prompt = options
                        .iter()
                        .map(|c| format!("[{}] {}", c.tab_number(), c.display_name()))
                        .collect::<Vec<_>>()
                        .join("  ");
                    self.status = format!("Split with: {}  [Esc: cancel]", prompt);
                }
            }
            "close" => {
                if !matches!(self.view, View::Connected) {
                    self.status = "Must be connected".to_string();
                    return;
                }
                match self.layout.close_secondary() {
                    Ok(()) => {
                        self.needs_full_clear = true;
                        self.status = "Closed secondary pane".to_string();
                    }
                    Err(msg) => {
                        self.status = msg.to_string();
                    }
                }
            }
            "set" => {
                // Handle :set commands (encoding, baud, etc.) - placeholder for now
                self.status = "Set commands not yet implemented".to_string();
            }
            _ => {
                self.status = format!("Unknown command: {}", parts[0]);
            }
        }
    }

    fn handle_key_quit_confirm(&mut self, key: KeyEvent) {
        match key.code {
            // Y/y confirms disconnect
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.traffic.quit_confirm = false;
                self.disconnect();
                self.view = View::PortSelect;
                self.needs_full_clear = true;
                self.status = "Disconnected.".to_string();
            }
            // n/N/q/Escape cancels
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                self.traffic.quit_confirm = false;
                self.status = "Disconnect cancelled.".to_string();
            }
            // Any other key is ignored
            _ => {}
        }
    }

    fn handle_key_traffic_config_dropdown(&mut self, key: KeyEvent) {
        let options_count = self.traffic.get_options_count();

        // Use global navigation for dropdown
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    if self.traffic.dropdown_index > 0 {
                        self.traffic.dropdown_index -= 1;
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    if self.traffic.dropdown_index < options_count.saturating_sub(1) {
                        self.traffic.dropdown_index += 1;
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    self.traffic.apply_dropdown_selection();
                    self.input.mode = InputMode::Normal;
                    self.needs_full_clear = true;
                    self.status = format!(
                        "{}: {}",
                        self.traffic.config_field.label(),
                        self.traffic.get_config_display(self.traffic.config_field)
                    );
                    return;
                }
                GlobalNavCommand::Cancel => {
                    self.input.mode = InputMode::Normal;
                    return;
                }
                _ => {}
            }
        }

        // Fall back to dropdown-specific bindings
        if let Some(cmd) = self.settings.keybindings.dropdown.find_command(&key) {
            match cmd {
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

    fn handle_key_config_text_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(value) => {
                self.port_select.apply_text_input(value.clone());
                self.status = format!(
                    "{}: {}",
                    self.port_select.config_field.label(),
                    self.port_select.get_config_display(self.port_select.config_field)
                );
            }
            TextInputResult::Cancel => {
                self.status = "Input cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    fn handle_key_traffic_config_text_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(value) => {
                self.traffic.apply_text_input(value.clone());
                self.status = format!(
                    "{}: {}",
                    self.traffic.config_field.label(),
                    self.traffic.get_config_display(self.traffic.config_field)
                );
            }
            TextInputResult::Cancel => {
                self.status = "Input cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    /// Find all match occurrences across all chunks
    /// Returns Ok(matches) on success, or Err(error_message) if regex is invalid
    fn find_all_matches(&self) -> Result<Vec<SearchMatch>, String> {
        let pattern = match &self.search.pattern {
            Some(p) if !p.is_empty() => p,
            _ => return Ok(vec![]),
        };

        let mut matches = Vec::new();

        match self.search.mode {
            SearchMode::Regex => {
                // Compile the regex pattern
                let regex = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;

                if let ConnectionState::Connected(ref handle) = self.connection {
                    let buffer = handle.buffer();
                    for (chunk_idx, chunk) in buffer.chunks().enumerate() {
                        let encoded = encode(&chunk.data, self.traffic.encoding);
                        // Apply hex grouping if in hex mode (same as rendering)
                        let encoded = if self.traffic.encoding == serial_core::Encoding::Hex {
                            format_hex_grouped(&encoded, self.traffic.hex_grouping)
                        } else {
                            encoded
                        };

                        // Find all regex matches within this chunk
                        for mat in regex.find_iter(&encoded) {
                            matches.push(SearchMatch {
                                chunk_index: chunk_idx,
                                byte_start: mat.start(),
                                byte_end: mat.end(),
                            });
                        }
                    }
                }
            }
            SearchMode::Normal => {
                // Case-insensitive literal search (original behavior)
                let pattern_lower = pattern.to_lowercase();

                if let ConnectionState::Connected(ref handle) = self.connection {
                    let buffer = handle.buffer();
                    for (chunk_idx, chunk) in buffer.chunks().enumerate() {
                        let encoded = encode(&chunk.data, self.traffic.encoding);
                        // Apply hex grouping if in hex mode (same as rendering)
                        let encoded = if self.traffic.encoding == serial_core::Encoding::Hex {
                            format_hex_grouped(&encoded, self.traffic.hex_grouping)
                        } else {
                            encoded
                        };
                        let encoded_lower = encoded.to_lowercase();

                        // Find all occurrences within this chunk
                        let mut search_start = 0;
                        while let Some(rel_pos) = encoded_lower[search_start..].find(&pattern_lower)
                        {
                            let byte_start = search_start + rel_pos;
                            let byte_end = byte_start + pattern.len();
                            matches.push(SearchMatch {
                                chunk_index: chunk_idx,
                                byte_start,
                                byte_end,
                            });
                            search_start = byte_end;
                        }
                    }
                }
            }
        }

        Ok(matches)
    }

    fn update_search_matches(&mut self) {
        // Clear any previous error
        self.search.error = None;

        match self.find_all_matches() {
            Ok(found_matches) => {
                self.search.matches = found_matches;
                self.search.current_match = None;

                if self.search.matches.is_empty() {
                    if let Some(ref pattern) = self.search.pattern {
                        self.status = format!("Pattern not found: {}", pattern);
                    }
                } else {
                    self.status = format!(
                        "Found {} match{}",
                        self.search.match_count(),
                        if self.search.match_count() == 1 {
                            ""
                        } else {
                            "es"
                        }
                    );
                }
            }
            Err(error) => {
                // Store the error and show in status line
                self.search.error = Some(error.clone());
                self.search.matches.clear();
                self.search.current_match = None;
                self.status = error;
            }
        }
    }

    fn goto_next_match(&mut self) {
        if self.search.matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        // Move to next match, wrapping around
        let next_idx = match self.search.current_match {
            Some(current) => (current + 1) % self.search.matches.len(),
            None => 0,
        };

        self.search.current_match = Some(next_idx);
        let m = &self.search.matches[next_idx];
        self.traffic.scroll_to_chunk = Some(m.chunk_index);
        self.status = format!(
            "Match {}/{}: {}",
            next_idx + 1,
            self.search.matches.len(),
            self.search.pattern.as_deref().unwrap_or("")
        );
    }

    fn goto_prev_match(&mut self) {
        if self.search.matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        // Move to previous match, wrapping around
        let prev_idx = match self.search.current_match {
            Some(current) => {
                if current == 0 {
                    self.search.matches.len() - 1
                } else {
                    current - 1
                }
            }
            None => self.search.matches.len() - 1,
        };

        self.search.current_match = Some(prev_idx);
        let m = &self.search.matches[prev_idx];
        self.traffic.scroll_to_chunk = Some(m.chunk_index);
        self.status = format!(
            "Match {}/{}: {}",
            prev_idx + 1,
            self.search.matches.len(),
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
                self.view = View::Connected;
                self.traffic.scroll_offset = 0;
                self.traffic.session_start = Some(std::time::SystemTime::now());
                
                // Copy pre-connection file save settings to traffic state
                self.traffic.save_enabled = self.port_select.save_enabled;
                self.traffic.save_format = self.port_select.save_format;
                self.traffic.save_filename = self.port_select.save_filename.clone();
                self.traffic.save_directory = self.port_select.save_directory.clone();
                
                // Start file saving if enabled in pre-connection settings
                if self.traffic.save_enabled {
                    self.start_file_saving();
                }
                
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
        // Stop file saving before disconnecting
        self.stop_file_saving();
        
        if let ConnectionState::Connected(handle) =
            std::mem::replace(&mut self.connection, ConnectionState::Disconnected)
        {
            let _ = self.runtime.block_on(handle.disconnect());
        }
    }

    /// Start file saving with current configuration
    fn start_file_saving(&mut self) {
        // Stop any existing file saver first
        self.stop_file_saving();

        // Get port name for auto-generated filename
        let port_name = if let ConnectionState::Connected(ref handle) = self.connection {
            handle.port_name().to_string()
        } else {
            "unknown".to_string()
        };

        // Build config
        let mut config = FileSaveConfig::new(
            self.traffic.save_directory.clone(),
            &port_name,
        ).with_format(self.traffic.save_format);

        // Set custom filename if provided
        if !self.traffic.save_filename.is_empty() {
            config = config.with_filename(&self.traffic.save_filename);
        }

        // Start the file saver (spawns async task on the provided runtime)
        match start_file_saver(config, &self.runtime) {
            Ok(handle) => {
                let path = handle.file_path().display().to_string();
                self.file_saver = Some(handle);
                self.status = format!("Saving to: {}", path);
            }
            Err(e) => {
                self.status = format!("Failed to start file saving: {}", e);
                self.traffic.save_enabled = false;
            }
        }
    }

    /// Stop file saving
    fn stop_file_saving(&mut self) {
        if let Some(handle) = self.file_saver.take() {
            let _ = handle.stop();
            self.status = "File saving stopped.".to_string();
        }
    }

    /// Send data chunk to file saver (if active)
    fn write_to_file_saver(&self, chunk: &DataChunk) {
        if let Some(ref handle) = self.file_saver {
            let _ = handle.write(chunk.clone());
        }
    }

    /// Handle toggling a traffic config setting
    /// This is separate from TrafficState::toggle_setting to handle side effects like file saving
    fn handle_traffic_toggle(&mut self) {
        let field = self.traffic.config_field;
        
        // Handle SaveEnabled specially - toggling during a session starts/stops file saving
        if field == TrafficConfigField::SaveEnabled {
            self.traffic.save_enabled = !self.traffic.save_enabled;
            if self.traffic.save_enabled {
                // Start file saving when enabled during a session
                self.start_file_saving();
            } else {
                // Stop file saving when disabled during a session
                self.stop_file_saving();
            }
        } else {
            // For other toggles, use the TrafficState method
            self.traffic.toggle_setting();
        }
        
        self.status = format!(
            "{}: {}",
            field.label(),
            self.traffic.get_config_display(field)
        );
        self.needs_full_clear = true;
    }

    /// Poll for session events (non-blocking)
    pub fn poll_session_events(&mut self) {
        // Collect events first to avoid borrow checker issues
        let events: Vec<SessionEvent> = if let ConnectionState::Connected(ref mut handle) = self.connection {
            let mut events = Vec::new();
            while let Some(event) = handle.try_recv_event() {
                events.push(event);
            }
            events
        } else {
            Vec::new()
        };

        // Now process the events
        for event in events {
            match event {
                SessionEvent::Disconnected { error } => {
                    // Stop file saving on disconnect
                    self.stop_file_saving();
                    
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
                SessionEvent::DataReceived(chunk) => {
                    // Write received data to file saver
                    self.write_to_file_saver(&chunk);
                }
                SessionEvent::DataSent(chunk) => {
                    // Write sent data to file saver
                    self.write_to_file_saver(&chunk);
                }
                SessionEvent::Connected => {}
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
