//! State structures for the application.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serial_core::{
    ChunkingStrategy, DataBits, Encoding, FlowControl, GraphEngine, GraphMode, LineDelimiter,
    Parity, ParserType, PatternMatcher, PortInfo, SaveFormat, SerialConfig,
    SessionConfig, StopBits, FileSendHandle, FileSendProgress, list_ports,
};

use crate::app::types::{
    ChunkingMode, ConfigField, ConfigOption, ConfigPanelState,
    DelimiterOption, FileSaveSettings, HexGrouping, InputMode, PaneContent,
    PaneFocus, PortSelectFocus, SizeUnit, TimestampFormat, TrafficConfigField,
    TrafficFocus, WrapMode, GraphConfigField, GraphFocus, SendConfigField,
    SendFocus, LineEndingOption, InputEncodingMode,
};

// =============================================================================
// Tab Layout State
// =============================================================================

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
    pub fn active_state_mut(&mut self) -> &mut TabState {
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

// =============================================================================
// Port Selection State
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
    /// Config panel state (field selection, dropdown, scroll)
    pub config: ConfigPanelState<ConfigField>,
    /// Serial port configuration
    pub serial_config: SerialConfig,
    // Chunking configuration (RX)
    /// RX chunking mode
    pub rx_chunking_mode: ChunkingMode,
    /// RX delimiter option
    pub rx_delimiter: DelimiterOption,
    /// RX custom delimiter (hex string like "00" or "0D0A")
    pub rx_custom_delimiter: String,
    /// RX max line length value
    pub rx_max_line_length: String,
    /// RX max line length unit
    pub rx_max_line_length_unit: SizeUnit,
    // Chunking configuration (TX)
    /// TX chunking mode
    pub tx_chunking_mode: ChunkingMode,
    /// TX delimiter option
    pub tx_delimiter: DelimiterOption,
    /// TX custom delimiter (hex string like "00" or "0D0A")
    pub tx_custom_delimiter: String,
    /// TX max line length value
    pub tx_max_line_length: String,
    /// TX max line length unit
    pub tx_max_line_length_unit: SizeUnit,
    // File saving configuration (pre-connection)
    /// File saving settings
    pub file_save: FileSaveSettings,
}

impl Default for PortSelectState {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            selected_port: 0,
            focus: PortSelectFocus::default(),
            config: ConfigPanelState::with_visible(true),
            serial_config: SerialConfig::default(),
            // RX chunking defaults
            rx_chunking_mode: ChunkingMode::default(),
            rx_delimiter: DelimiterOption::default(),
            rx_custom_delimiter: String::new(),
            rx_max_line_length: "64".to_string(),
            rx_max_line_length_unit: SizeUnit::KiB,
            // TX chunking defaults
            tx_chunking_mode: ChunkingMode::default(),
            tx_delimiter: DelimiterOption::default(),
            tx_custom_delimiter: String::new(),
            tx_max_line_length: "64".to_string(),
            tx_max_line_length_unit: SizeUnit::KiB,
            // File saving defaults
            file_save: FileSaveSettings::new(),
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
        match self.config.field {
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
            // Chunking mode options
            ConfigField::RxChunkingMode | ConfigField::TxChunkingMode => {
                ChunkingMode::all_display_names()
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            // Delimiter options
            ConfigField::RxDelimiter | ConfigField::TxDelimiter => {
                DelimiterOption::all_display_names()
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            // Size unit options
            ConfigField::RxMaxLineLengthUnit | ConfigField::TxMaxLineLengthUnit => {
                SizeUnit::all_display_names()
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            // Toggle and text input fields don't have dropdown options
            ConfigField::SaveEnabled
            | ConfigField::SaveFilename
            | ConfigField::SaveDirectory
            | ConfigField::RxCustomDelimiter
            | ConfigField::TxCustomDelimiter
            | ConfigField::RxMaxLineLength
            | ConfigField::TxMaxLineLength => vec![],
        }
    }

    /// Get the current index in the options list for the selected config field
    pub fn get_current_config_index(&self) -> usize {
        match self.config.field {
            ConfigField::BaudRate => Self::BAUD_RATES
                .iter()
                .position(|&b| b == self.serial_config.baud_rate)
                .unwrap_or(8), // Default to 115200
            ConfigField::DataBits => self.serial_config.data_bits.index(),
            ConfigField::Parity => self.serial_config.parity.index(),
            ConfigField::StopBits => self.serial_config.stop_bits.index(),
            ConfigField::FlowControl => self.serial_config.flow_control.index(),
            ConfigField::SaveFormat => self.file_save.format.index(),
            // Chunking fields
            ConfigField::RxChunkingMode => self.rx_chunking_mode.index(),
            ConfigField::TxChunkingMode => self.tx_chunking_mode.index(),
            ConfigField::RxDelimiter => self.rx_delimiter.index(),
            ConfigField::TxDelimiter => self.tx_delimiter.index(),
            ConfigField::RxMaxLineLengthUnit => self.rx_max_line_length_unit.index(),
            ConfigField::TxMaxLineLengthUnit => self.tx_max_line_length_unit.index(),
            // Text input and toggle fields
            ConfigField::SaveEnabled
            | ConfigField::SaveFilename
            | ConfigField::SaveDirectory
            | ConfigField::RxCustomDelimiter
            | ConfigField::TxCustomDelimiter
            | ConfigField::RxMaxLineLength
            | ConfigField::TxMaxLineLength => 0,
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
                if self.file_save.enabled { "ON" } else { "OFF" }.to_string()
            }
            ConfigField::SaveFormat => self.file_save.format.display_name().to_string(),
            ConfigField::SaveFilename => {
                if self.file_save.filename.is_empty() {
                    "(auto)".to_string()
                } else {
                    self.file_save.filename.clone()
                }
            }
            ConfigField::SaveDirectory => self.file_save.directory.clone(),
            // RX Chunking fields
            ConfigField::RxChunkingMode => self.rx_chunking_mode.display_name().to_string(),
            ConfigField::RxDelimiter => self.rx_delimiter.display_name().to_string(),
            ConfigField::RxCustomDelimiter => {
                if self.rx_custom_delimiter.is_empty() {
                    "(hex bytes)".to_string()
                } else {
                    self.rx_custom_delimiter.clone()
                }
            }
            ConfigField::RxMaxLineLength => self.rx_max_line_length.clone(),
            ConfigField::RxMaxLineLengthUnit => self.rx_max_line_length_unit.display_name().to_string(),
            // TX Chunking fields
            ConfigField::TxChunkingMode => self.tx_chunking_mode.display_name().to_string(),
            ConfigField::TxDelimiter => self.tx_delimiter.display_name().to_string(),
            ConfigField::TxCustomDelimiter => {
                if self.tx_custom_delimiter.is_empty() {
                    "(hex bytes)".to_string()
                } else {
                    self.tx_custom_delimiter.clone()
                }
            }
            ConfigField::TxMaxLineLength => self.tx_max_line_length.clone(),
            ConfigField::TxMaxLineLengthUnit => self.tx_max_line_length_unit.display_name().to_string(),
        }
    }

    /// Open the dropdown for the current config field
    pub fn open_dropdown(&mut self) {
        self.config.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value to the config
    pub fn apply_dropdown_selection(&mut self) {
        match self.config.field {
            ConfigField::BaudRate => {
                if let Some(&baud) = Self::BAUD_RATES.get(self.config.dropdown_index) {
                    self.serial_config.baud_rate = baud;
                }
            }
            ConfigField::DataBits => {
                self.serial_config.data_bits = DataBits::from_index(self.config.dropdown_index);
            }
            ConfigField::Parity => {
                self.serial_config.parity = Parity::from_index(self.config.dropdown_index);
            }
            ConfigField::StopBits => {
                self.serial_config.stop_bits = StopBits::from_index(self.config.dropdown_index);
            }
            ConfigField::FlowControl => {
                self.serial_config.flow_control = FlowControl::from_index(self.config.dropdown_index);
            }
            ConfigField::SaveFormat => {
                self.file_save.format = SaveFormat::from_index(self.config.dropdown_index);
            }
            // Chunking dropdown fields
            ConfigField::RxChunkingMode => {
                self.rx_chunking_mode = ChunkingMode::from_index(self.config.dropdown_index);
            }
            ConfigField::TxChunkingMode => {
                self.tx_chunking_mode = ChunkingMode::from_index(self.config.dropdown_index);
            }
            ConfigField::RxDelimiter => {
                self.rx_delimiter = DelimiterOption::from_index(self.config.dropdown_index);
            }
            ConfigField::TxDelimiter => {
                self.tx_delimiter = DelimiterOption::from_index(self.config.dropdown_index);
            }
            ConfigField::RxMaxLineLengthUnit => {
                self.rx_max_line_length_unit = SizeUnit::from_index(self.config.dropdown_index);
            }
            ConfigField::TxMaxLineLengthUnit => {
                self.tx_max_line_length_unit = SizeUnit::from_index(self.config.dropdown_index);
            }
            // Toggle and text input fields don't use dropdown
            ConfigField::SaveEnabled
            | ConfigField::SaveFilename
            | ConfigField::SaveDirectory
            | ConfigField::RxCustomDelimiter
            | ConfigField::TxCustomDelimiter
            | ConfigField::RxMaxLineLength
            | ConfigField::TxMaxLineLength => {}
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config.field {
            ConfigField::BaudRate => Self::BAUD_RATES.len(),
            ConfigField::DataBits => DataBits::all_variants().len(),
            ConfigField::Parity => Parity::all_variants().len(),
            ConfigField::StopBits => StopBits::all_variants().len(),
            ConfigField::FlowControl => FlowControl::all_variants().len(),
            ConfigField::SaveFormat => SaveFormat::all_variants().len(),
            // Chunking dropdown fields
            ConfigField::RxChunkingMode | ConfigField::TxChunkingMode => {
                ChunkingMode::all_variants().len()
            }
            ConfigField::RxDelimiter | ConfigField::TxDelimiter => {
                DelimiterOption::all_variants().len()
            }
            ConfigField::RxMaxLineLengthUnit | ConfigField::TxMaxLineLengthUnit => {
                SizeUnit::all_variants().len()
            }
            // Toggle and text input fields
            ConfigField::SaveEnabled
            | ConfigField::SaveFilename
            | ConfigField::SaveDirectory
            | ConfigField::RxCustomDelimiter
            | ConfigField::TxCustomDelimiter
            | ConfigField::RxMaxLineLength
            | ConfigField::TxMaxLineLength => 0,
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        if self.config.field == ConfigField::SaveEnabled {
            self.file_save.enabled = !self.file_save.enabled;
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config.field {
            ConfigField::SaveFilename => {
                self.file_save.filename = value;
            }
            ConfigField::SaveDirectory => {
                self.file_save.directory = value;
            }
            ConfigField::RxCustomDelimiter => {
                self.rx_custom_delimiter = value;
            }
            ConfigField::TxCustomDelimiter => {
                self.tx_custom_delimiter = value;
            }
            ConfigField::RxMaxLineLength => {
                // Only store if it's a valid number or empty
                if value.is_empty() || value.parse::<usize>().is_ok() {
                    self.rx_max_line_length = value;
                }
            }
            ConfigField::TxMaxLineLength => {
                // Only store if it's a valid number or empty
                if value.is_empty() || value.parse::<usize>().is_ok() {
                    self.tx_max_line_length = value;
                }
            }
            _ => {}
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config.field {
            ConfigField::SaveFilename => self.file_save.filename.clone(),
            ConfigField::SaveDirectory => self.file_save.directory.clone(),
            ConfigField::RxCustomDelimiter => self.rx_custom_delimiter.clone(),
            ConfigField::TxCustomDelimiter => self.tx_custom_delimiter.clone(),
            ConfigField::RxMaxLineLength => self.rx_max_line_length.clone(),
            ConfigField::TxMaxLineLength => self.tx_max_line_length.clone(),
            _ => String::new(),
        }
    }

    /// Build the SessionConfig from the current chunking settings
    pub fn build_session_config(&self) -> SessionConfig {
        let rx_chunking = self.build_chunking_strategy(
            self.rx_chunking_mode,
            self.rx_delimiter,
            &self.rx_custom_delimiter,
            &self.rx_max_line_length,
            self.rx_max_line_length_unit,
        );
        let tx_chunking = self.build_chunking_strategy(
            self.tx_chunking_mode,
            self.tx_delimiter,
            &self.tx_custom_delimiter,
            &self.tx_max_line_length,
            self.tx_max_line_length_unit,
        );

        SessionConfig::new()
            .with_rx_chunking(rx_chunking)
            .with_tx_chunking(tx_chunking)
    }

    /// Build a ChunkingStrategy from UI settings
    fn build_chunking_strategy(
        &self,
        mode: ChunkingMode,
        delimiter_opt: DelimiterOption,
        custom_delimiter: &str,
        max_length: &str,
        length_unit: SizeUnit,
    ) -> ChunkingStrategy {
        match mode {
            ChunkingMode::Raw => ChunkingStrategy::Raw,
            ChunkingMode::LineDelimited => {
                let delimiter = match delimiter_opt {
                    DelimiterOption::Newline => LineDelimiter::Newline,
                    DelimiterOption::CrLf => LineDelimiter::CrLf,
                    DelimiterOption::Cr => LineDelimiter::Cr,
                    DelimiterOption::Custom => {
                        // Parse hex string like "00" or "0D0A" into bytes
                        let bytes = Self::parse_hex_string(custom_delimiter);
                        if bytes.len() == 1 {
                            LineDelimiter::Byte(bytes[0])
                        } else if !bytes.is_empty() {
                            LineDelimiter::Bytes(bytes)
                        } else {
                            // Fallback to newline if parsing fails
                            LineDelimiter::Newline
                        }
                    }
                };

                let max_line_length = max_length
                    .parse::<usize>()
                    .map(|v| length_unit.to_bytes(v))
                    .unwrap_or(64 * 1024); // Default 64 KiB

                ChunkingStrategy::with_delimiter(delimiter)
                    .with_max_line_length(max_line_length)
            }
        }
    }

    /// Parse a hex string like "00", "0D0A", or "DE AD BE EF" into bytes
    fn parse_hex_string(s: &str) -> Vec<u8> {
        // Remove spaces and parse pairs of hex digits
        let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let mut bytes = Vec::new();
        let mut chars = clean.chars().peekable();
        
        while chars.peek().is_some() {
            let high = chars.next();
            let low = chars.next();
            
            if let (Some(h), Some(l)) = (high, low)
                && let Ok(byte) = u8::from_str_radix(&format!("{}{}", h, l), 16)
            {
                bytes.push(byte);
            }
        }
        
        bytes
    }
}

// =============================================================================
// Traffic State
// =============================================================================

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
    /// Config panel state (field selection, dropdown, scroll)
    pub config: ConfigPanelState<TrafficConfigField>,
    /// Which panel is focused (traffic or config)
    pub focus: TrafficFocus,
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
    // Filtering state
    /// Whether filtering is enabled
    pub filter_enabled: bool,
    /// Filter pattern matcher (with cached regex)
    pub filter: PatternMatcher,
    // File saving state
    /// File save settings
    pub file_save: FileSaveSettings,
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
            config: ConfigPanelState::new(),
            focus: TrafficFocus::default(),
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
            // Filtering defaults
            filter_enabled: false,
            filter: PatternMatcher::new(),
            // File saving defaults
            file_save: FileSaveSettings::new(),
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
            TrafficConfigField::FilterEnabled => {
                if self.filter_enabled { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::FilterPattern => {
                if let Some(pattern) = self.filter.pattern() {
                    pattern.to_string()
                } else {
                    "(none)".to_string()
                }
            }
            TrafficConfigField::SaveEnabled => {
                if self.file_save.enabled { "ON" } else { "OFF" }.to_string()
            }
            TrafficConfigField::SaveFormat => self.file_save.format.display_name().to_string(),
            TrafficConfigField::SaveFilename => {
                if self.file_save.filename.is_empty() {
                    "(auto)".to_string()
                } else {
                    self.file_save.filename.clone()
                }
            }
            TrafficConfigField::SaveDirectory => self.file_save.directory.clone(),
        }
    }

    /// Get string options for dropdown (for non-toggle fields)
    pub fn get_config_option_strings(&self) -> Vec<String> {
        match self.config.field {
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
        match self.config.field {
            TrafficConfigField::TimestampFormat => self.timestamp_format.index(),
            TrafficConfigField::Encoding => self.encoding.index(),
            TrafficConfigField::WrapMode => self.wrap_mode.index(),
            TrafficConfigField::HexGrouping => self.hex_grouping.index(),
            TrafficConfigField::SaveFormat => self.file_save.format.index(),
            _ => 0,
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config.field {
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
        self.config.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value
    pub fn apply_dropdown_selection(&mut self) {
        match self.config.field {
            TrafficConfigField::TimestampFormat => {
                self.timestamp_format = TimestampFormat::from_index(self.config.dropdown_index);
            }
            TrafficConfigField::Encoding => {
                self.encoding = Encoding::from_index(self.config.dropdown_index);
            }
            TrafficConfigField::WrapMode => {
                self.wrap_mode = WrapMode::from_index(self.config.dropdown_index);
            }
            TrafficConfigField::HexGrouping => {
                self.hex_grouping = HexGrouping::from_index(self.config.dropdown_index);
            }
            TrafficConfigField::SaveFormat => {
                self.file_save.format = SaveFormat::from_index(self.config.dropdown_index);
            }
            _ => {}
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        match self.config.field {
            TrafficConfigField::LineNumbers => self.show_line_numbers = !self.show_line_numbers,
            TrafficConfigField::Timestamps => self.show_timestamps = !self.show_timestamps,
            TrafficConfigField::AutoScroll => self.auto_scroll = !self.auto_scroll,
            TrafficConfigField::LockToBottom => self.lock_to_bottom = !self.lock_to_bottom,
            TrafficConfigField::ShowTx => self.show_tx = !self.show_tx,
            TrafficConfigField::ShowRx => self.show_rx = !self.show_rx,
            TrafficConfigField::FilterEnabled => self.filter_enabled = !self.filter_enabled,
            TrafficConfigField::SaveEnabled => self.file_save.enabled = !self.file_save.enabled,
            _ => {}
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config.field {
            TrafficConfigField::FilterPattern => {
                // Set the filter pattern using the PatternMatcher (preserves current mode)
                let mode = self.filter.mode();
                // Ignore errors - invalid regex won't hide data
                let _ = self.filter.set_pattern(&value, mode);
            }
            TrafficConfigField::SaveFilename => {
                self.file_save.filename = value;
            }
            TrafficConfigField::SaveDirectory => {
                self.file_save.directory = value;
            }
            _ => {}
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config.field {
            TrafficConfigField::FilterPattern => self.filter.pattern().unwrap_or("").to_string(),
            TrafficConfigField::SaveFilename => self.file_save.filename.clone(),
            TrafficConfigField::SaveDirectory => self.file_save.directory.clone(),
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

    /// Check if filtering should be applied (enabled, has pattern, and encoding is text-based)
    pub fn should_apply_filter(&self, encoding: Encoding) -> bool {
        self.filter_enabled
            && self.filter.has_pattern()
            && matches!(encoding, Encoding::Utf8 | Encoding::Ascii)
    }

    /// Check if a chunk's content matches the filter pattern.
    /// Returns true if the chunk should be shown.
    pub fn matches_filter(&self, encoded_content: &str) -> bool {
        // If filter is not active or pattern is empty, show everything
        if !self.filter_enabled || !self.filter.has_pattern() {
            return true;
        }

        // PatternMatcher returns true if no pattern is set (matches everything)
        // and handles both Normal and Regex modes with cached compilation
        self.filter.is_match(encoded_content)
    }
}

// =============================================================================
// Graph State
// =============================================================================

/// Time window options for graph display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphTimeWindow {
    /// Last 10 seconds
    Seconds10,
    /// Last 30 seconds
    #[default]
    Seconds30,
    /// Last 1 minute
    Minute1,
    /// Last 5 minutes
    Minutes5,
    /// All available data
    All,
}

impl GraphTimeWindow {
    /// Get all variants
    pub fn all_variants() -> &'static [GraphTimeWindow] {
        &[
            GraphTimeWindow::Seconds10,
            GraphTimeWindow::Seconds30,
            GraphTimeWindow::Minute1,
            GraphTimeWindow::Minutes5,
            GraphTimeWindow::All,
        ]
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            GraphTimeWindow::Seconds10 => "10 seconds",
            GraphTimeWindow::Seconds30 => "30 seconds",
            GraphTimeWindow::Minute1 => "1 minute",
            GraphTimeWindow::Minutes5 => "5 minutes",
            GraphTimeWindow::All => "All data",
        }
    }

    /// Get all display names
    pub fn all_display_names() -> Vec<&'static str> {
        Self::all_variants().iter().map(|v| v.display_name()).collect()
    }

    /// Get index
    pub fn index(&self) -> usize {
        Self::all_variants().iter().position(|v| v == self).unwrap_or(0)
    }

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        Self::all_variants().get(index).copied().unwrap_or_default()
    }

    /// Get duration in seconds (None for All)
    pub fn as_duration(&self) -> Option<std::time::Duration> {
        match self {
            GraphTimeWindow::Seconds10 => Some(std::time::Duration::from_secs(10)),
            GraphTimeWindow::Seconds30 => Some(std::time::Duration::from_secs(30)),
            GraphTimeWindow::Minute1 => Some(std::time::Duration::from_secs(60)),
            GraphTimeWindow::Minutes5 => Some(std::time::Duration::from_secs(300)),
            GraphTimeWindow::All => None,
        }
    }
}

/// State for graph view
#[derive(Debug)]
pub struct GraphState {
    /// Graph engine (lazy initialized when graph view is first opened)
    pub engine: Option<GraphEngine>,
    /// Whether the engine has been initialized with historical data
    pub initialized: bool,
    /// Which panel is focused (graph or config)
    pub focus: GraphFocus,
    /// Config panel state (field selection, dropdown, scroll)
    pub config: ConfigPanelState<GraphConfigField>,
    /// Time window for display
    pub time_window: GraphTimeWindow,
    /// Whether to show RX data in packet rate
    pub show_rx: bool,
    /// Whether to show TX data in packet rate
    pub show_tx: bool,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            engine: None,
            initialized: false,
            focus: GraphFocus::default(),
            config: ConfigPanelState::new(),
            time_window: GraphTimeWindow::default(),
            show_rx: true,
            show_tx: true,
        }
    }
}

impl GraphState {
    /// Get or create the graph engine (lazy initialization)
    pub fn engine_mut(&mut self) -> &mut GraphEngine {
        self.engine.get_or_insert_with(GraphEngine::new)
    }

    /// Check if the engine exists
    pub fn has_engine(&self) -> bool {
        self.engine.is_some()
    }

    /// Get display value for a graph config field
    pub fn get_config_display(&self, field: GraphConfigField) -> String {
        match field {
            GraphConfigField::Mode => self
                .engine
                .as_ref()
                .map(|e| e.mode().name())
                .unwrap_or(GraphMode::default().name())
                .to_string(),
            GraphConfigField::Parser => self
                .engine
                .as_ref()
                .map(|e| e.parser_config().parser_type().name())
                .unwrap_or(ParserType::default().name())
                .to_string(),
            GraphConfigField::RegexPattern => self.get_regex_pattern(),
            GraphConfigField::TimeWindow => self.time_window.display_name().to_string(),
            GraphConfigField::ShowRx => if self.show_rx { "ON" } else { "OFF" }.to_string(),
            GraphConfigField::ShowTx => if self.show_tx { "ON" } else { "OFF" }.to_string(),
        }
    }

    /// Get the current regex pattern (if parser is Regex type)
    pub fn get_regex_pattern(&self) -> String {
        if let Some(ref engine) = self.engine {
            if let serial_core::GraphParserConfig::Regex(cfg) = engine.parser_config() {
                return cfg.pattern.clone();
            }
        }
        // Default pattern
        r"(?P<key>\w+)[=:]\s*(?P<value>-?\d+\.?\d*)".to_string()
    }

    /// Set the regex pattern (only effective when parser is Regex type)
    pub fn set_regex_pattern(&mut self, pattern: String) {
        if self.engine.is_some() {
            let config = serial_core::GraphParserConfig::Regex(
                serial_core::RegexParserConfig { pattern }
            );
            self.engine_mut().set_parser_config(config);
        }
    }

    /// Check if the regex pattern field should be shown (only for Regex parser)
    pub fn should_show_regex_pattern(&self) -> bool {
        self.engine
            .as_ref()
            .map(|e| e.parser_config().parser_type() == ParserType::Regex)
            .unwrap_or(false)
    }

    /// Get list of series names from parsed data
    pub fn series_names(&self) -> Vec<String> {
        self.engine
            .as_ref()
            .map(|e| e.series_names().into_iter().map(String::from).collect())
            .unwrap_or_default()
    }

    /// Toggle visibility of a series by name
    pub fn toggle_series_visibility(&mut self, name: &str) {
        if let Some(ref mut engine) = self.engine {
            engine.toggle_series_visibility(name);
        }
    }

    /// Check if a series is visible
    pub fn is_series_visible(&self, name: &str) -> bool {
        self.engine
            .as_ref()
            .and_then(|e| e.parsed_data().series(name))
            .map(|s| s.visible)
            .unwrap_or(true)
    }

    /// Get string options for dropdown
    pub fn get_config_option_strings(&self) -> Vec<String> {
        match self.config.field {
            GraphConfigField::Mode => GraphMode::all()
                .iter()
                .map(|m| m.name().to_string())
                .collect(),
            GraphConfigField::Parser => ParserType::all()
                .iter()
                .map(|p| p.name().to_string())
                .collect(),
            GraphConfigField::TimeWindow => GraphTimeWindow::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            GraphConfigField::RegexPattern | GraphConfigField::ShowRx | GraphConfigField::ShowTx => vec![],
        }
    }

    /// Get the current index for dropdown selection
    pub fn get_current_config_index(&self) -> usize {
        match self.config.field {
            GraphConfigField::Mode => {
                let mode = self.engine.as_ref().map(|e| e.mode()).unwrap_or_default();
                GraphMode::all().iter().position(|m| *m == mode).unwrap_or(0)
            }
            GraphConfigField::Parser => {
                let parser_type = self
                    .engine
                    .as_ref()
                    .map(|e| e.parser_config().parser_type())
                    .unwrap_or_default();
                ParserType::all()
                    .iter()
                    .position(|p| *p == parser_type)
                    .unwrap_or(0)
            }
            GraphConfigField::TimeWindow => self.time_window.index(),
            GraphConfigField::RegexPattern | GraphConfigField::ShowRx | GraphConfigField::ShowTx => 0,
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config.field {
            GraphConfigField::Mode => GraphMode::all().len(),
            GraphConfigField::Parser => ParserType::all().len(),
            GraphConfigField::TimeWindow => GraphTimeWindow::all_variants().len(),
            GraphConfigField::RegexPattern | GraphConfigField::ShowRx | GraphConfigField::ShowTx => 0,
        }
    }

    /// Open the dropdown for the current config field
    pub fn open_dropdown(&mut self) {
        self.config.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value
    pub fn apply_dropdown_selection(&mut self) {
        match self.config.field {
            GraphConfigField::Mode => {
                if let Some(mode) = GraphMode::all().get(self.config.dropdown_index) {
                    self.engine_mut().set_mode(*mode);
                }
            }
            GraphConfigField::Parser => {
                if let Some(parser_type) = ParserType::all().get(self.config.dropdown_index) {
                    let config = match parser_type {
                        ParserType::KeyValue => {
                            serial_core::GraphParserConfig::KeyValue(Default::default())
                        }
                        ParserType::Regex => {
                            serial_core::GraphParserConfig::Regex(Default::default())
                        }
                        ParserType::Csv => {
                            serial_core::GraphParserConfig::Csv(Default::default())
                        }
                        ParserType::Json => serial_core::GraphParserConfig::Json,
                        ParserType::RawNumber => {
                            serial_core::GraphParserConfig::RawNumber(Default::default())
                        }
                    };
                    self.engine_mut().set_parser_config(config);
                }
            }
            GraphConfigField::TimeWindow => {
                self.time_window = GraphTimeWindow::from_index(self.config.dropdown_index);
            }
            GraphConfigField::RegexPattern | GraphConfigField::ShowRx | GraphConfigField::ShowTx => {}
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        match self.config.field {
            GraphConfigField::ShowRx => self.show_rx = !self.show_rx,
            GraphConfigField::ShowTx => self.show_tx = !self.show_tx,
            _ => {}
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config.field {
            GraphConfigField::RegexPattern => self.get_regex_pattern(),
            _ => String::new(),
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config.field {
            GraphConfigField::RegexPattern => {
                self.set_regex_pattern(value);
            }
            _ => {}
        }
    }
}

// =============================================================================
// Send State
// =============================================================================

/// State for advanced send view
#[derive(Debug)]
pub struct SendState {
    /// Which panel is focused (content or config)
    pub focus: SendFocus,
    /// Config panel state (field selection, dropdown, scroll)
    pub config: ConfigPanelState<SendConfigField>,
    /// File path for file sending
    pub file_path: String,
    /// Chunk size in bytes for file sending
    pub chunk_size: String,
    /// Delay between chunks in milliseconds
    pub chunk_delay: String,
    /// Whether to loop the file continuously
    pub continuous: bool,
    /// Line ending to append when sending
    pub line_ending: LineEndingOption,
    /// Input encoding mode (text vs hex)
    pub input_encoding: InputEncodingMode,
    /// Send history (ring buffer of previously sent data)
    pub history: Vec<String>,
    /// Current position in history (for arrow navigation)
    pub history_index: Option<usize>,
    /// Maximum history size
    pub history_max_size: usize,
}

impl Default for SendState {
    fn default() -> Self {
        Self {
            focus: SendFocus::default(),
            config: ConfigPanelState::with_visible(true),
            file_path: String::new(),
            chunk_size: "64".to_string(),
            chunk_delay: "10".to_string(),
            continuous: false,
            line_ending: LineEndingOption::default(),
            input_encoding: InputEncodingMode::default(),
            history: Vec::new(),
            history_index: None,
            history_max_size: 100,
        }
    }
}

impl SendState {
    /// Get display value for a send config field
    pub fn get_config_display(&self, field: SendConfigField) -> String {
        match field {
            SendConfigField::FilePath => {
                if self.file_path.is_empty() {
                    "(none)".to_string()
                } else {
                    self.file_path.clone()
                }
            }
            SendConfigField::ChunkSize => format!("{} bytes", self.chunk_size),
            SendConfigField::ChunkDelay => format!("{} ms", self.chunk_delay),
            SendConfigField::Continuous => {
                if self.continuous { "ON" } else { "OFF" }.to_string()
            }
            SendConfigField::LineEnding => self.line_ending.display_name().to_string(),
            SendConfigField::InputEncoding => self.input_encoding.display_name().to_string(),
        }
    }

    /// Get string options for dropdown
    pub fn get_config_option_strings(&self) -> Vec<String> {
        match self.config.field {
            SendConfigField::LineEnding => LineEndingOption::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            SendConfigField::InputEncoding => InputEncodingMode::all_display_names()
                .into_iter()
                .map(String::from)
                .collect(),
            // Text input fields don't have dropdowns
            _ => vec![],
        }
    }

    /// Get the current index for dropdown selection
    pub fn get_current_config_index(&self) -> usize {
        match self.config.field {
            SendConfigField::LineEnding => self.line_ending.index(),
            SendConfigField::InputEncoding => self.input_encoding.index(),
            _ => 0,
        }
    }

    /// Get the number of options for the current config field
    pub fn get_options_count(&self) -> usize {
        match self.config.field {
            SendConfigField::LineEnding => LineEndingOption::all_variants().len(),
            SendConfigField::InputEncoding => InputEncodingMode::all_variants().len(),
            _ => 0,
        }
    }

    /// Open the dropdown for the current config field
    pub fn open_dropdown(&mut self) {
        self.config.dropdown_index = self.get_current_config_index();
    }

    /// Apply the selected dropdown value
    pub fn apply_dropdown_selection(&mut self) {
        match self.config.field {
            SendConfigField::LineEnding => {
                self.line_ending = LineEndingOption::from_index(self.config.dropdown_index);
            }
            SendConfigField::InputEncoding => {
                self.input_encoding = InputEncodingMode::from_index(self.config.dropdown_index);
            }
            _ => {}
        }
    }

    /// Toggle a boolean setting
    pub fn toggle_setting(&mut self) {
        if self.config.field == SendConfigField::Continuous {
            self.continuous = !self.continuous;
        }
    }

    /// Get the current text value for text input fields
    pub fn get_text_value(&self) -> String {
        match self.config.field {
            SendConfigField::FilePath => self.file_path.clone(),
            SendConfigField::ChunkSize => self.chunk_size.clone(),
            SendConfigField::ChunkDelay => self.chunk_delay.clone(),
            _ => String::new(),
        }
    }

    /// Apply text input value to the appropriate field
    pub fn apply_text_input(&mut self, value: String) {
        match self.config.field {
            SendConfigField::FilePath => {
                self.file_path = value;
            }
            SendConfigField::ChunkSize => {
                // Only store if it's a valid number or empty
                if value.is_empty() || value.parse::<usize>().is_ok() {
                    self.chunk_size = value;
                }
            }
            SendConfigField::ChunkDelay => {
                // Only store if it's a valid number or empty
                if value.is_empty() || value.parse::<u64>().is_ok() {
                    self.chunk_delay = value;
                }
            }
            _ => {}
        }
    }

    /// Add a command to history
    pub fn add_to_history(&mut self, cmd: String) {
        // Don't add empty or duplicate of most recent
        if cmd.is_empty() {
            return;
        }
        if self.history.last().map(|h| h == &cmd).unwrap_or(false) {
            return;
        }
        
        self.history.push(cmd);
        
        // Trim to max size
        while self.history.len() > self.history_max_size {
            self.history.remove(0);
        }
        
        // Reset index when adding new item
        self.history_index = None;
    }

    /// Navigate up in history (older)
    pub fn history_up(&mut self) -> Option<&str> {
        if self.history.is_empty() {
            return None;
        }
        
        let new_idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        
        self.history_index = Some(new_idx);
        self.history.get(new_idx).map(|s| s.as_str())
    }

    /// Navigate down in history (newer)
    pub fn history_down(&mut self) -> Option<&str> {
        if self.history.is_empty() {
            return None;
        }
        
        match self.history_index {
            None => None,
            Some(idx) => {
                if idx + 1 >= self.history.len() {
                    self.history_index = None;
                    None
                } else {
                    self.history_index = Some(idx + 1);
                    self.history.get(idx + 1).map(|s| s.as_str())
                }
            }
        }
    }

    /// Reset history navigation (when user starts typing)
    pub fn reset_history_index(&mut self) {
        self.history_index = None;
    }

    /// Get chunk size as usize
    pub fn chunk_size_bytes(&self) -> usize {
        self.chunk_size.parse().unwrap_or(64)
    }

    /// Get chunk delay as Duration
    pub fn chunk_delay_duration(&self) -> std::time::Duration {
        let ms = self.chunk_delay.parse().unwrap_or(10u64);
        std::time::Duration::from_millis(ms)
    }
}

// =============================================================================
// Input State
// =============================================================================

/// State for file sending
#[derive(Default)]
pub struct FileSendState {
    /// Active file send operation
    pub handle: Option<FileSendHandle>,
    /// Latest file send progress
    pub progress: Option<FileSendProgress>,
}

// Manual Debug implementation since FileSendHandle doesn't implement Debug
impl std::fmt::Debug for FileSendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSendState")
            .field("handle", &self.handle.is_some())
            .field("progress", &self.progress)
            .finish()
    }
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
                let value = self.buffer.clone();
                self.buffer.clear();
                self.mode = InputMode::Normal;
                TextInputResult::Submit(value)
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
