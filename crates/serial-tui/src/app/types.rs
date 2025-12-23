//! Type definitions, traits, and enums for the application.

use serial_core::{
    DataBits, Encoding, FlowControl, Parity, PatternMode, SaveFormat, StopBits,
};
use strum::{EnumCount, EnumIter, IntoStaticStr, VariantArray};

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

/// Marker trait for local enums that use strum derives.
/// This allows the blanket `ConfigOption` impl without conflicting with external types.
pub trait LocalStrumEnum: Sized + Copy + PartialEq + 'static + VariantArray + Into<&'static str> {}

// Implement marker trait for all local strum enums that need ConfigOption
impl LocalStrumEnum for ChunkingMode {}
impl LocalStrumEnum for DelimiterOption {}
impl LocalStrumEnum for SizeUnit {}
impl LocalStrumEnum for TimestampFormat {}
impl LocalStrumEnum for WrapMode {}
impl LocalStrumEnum for HexGrouping {}

/// Blanket implementation of `ConfigOption` for local enums that derive strum's
/// `VariantArray` and `IntoStaticStr`. This avoids repetitive manual impls.
impl<T: LocalStrumEnum> ConfigOption for T {
    fn all_variants() -> &'static [Self] {
        Self::VARIANTS
    }

    fn display_name(&self) -> &'static str {
        (*self).into()
    }
}

// =============================================================================
// EnumNavigation Trait - Navigation helpers for strum-derived enums
// =============================================================================

/// Trait providing navigation methods for enums that derive `VariantArray` and `IntoStaticStr`.
/// 
/// This trait has default implementations using strum's derives, so any enum that
/// derives `VariantArray` and `IntoStaticStr` can implement this trait with no body.
pub trait EnumNavigation: Sized + Copy + PartialEq + VariantArray + Into<&'static str> {
    /// Get the next variant in the list (wrapping)
    fn next(self) -> Self {
        let idx = Self::VARIANTS.iter().position(|&v| v == self).unwrap_or(0);
        Self::VARIANTS[(idx + 1) % Self::VARIANTS.len()]
    }

    /// Get the previous variant in the list (wrapping)
    fn prev(self) -> Self {
        let idx = Self::VARIANTS.iter().position(|&v| v == self).unwrap_or(0);
        Self::VARIANTS[(idx + Self::VARIANTS.len() - 1) % Self::VARIANTS.len()]
    }

    /// Get the index of this variant in the list
    fn index(self) -> usize {
        Self::VARIANTS.iter().position(|&v| v == self).unwrap_or(0)
    }

    /// Get the display label for this variant (from `IntoStaticStr`)
    fn label(&self) -> &'static str {
        (*self).into()
    }
}

// =============================================================================
// Config Field Trait and Panel State
// =============================================================================

/// Trait for config field enums (e.g., ConfigField, TrafficConfigField)
/// Provides common methods for navigating and querying field properties.
/// 
/// Requires `EnumNavigation` which provides `next()`, `prev()`, `index()`, `label()`.
pub trait ConfigFieldKind: EnumNavigation + Default {
    /// Whether this field is a toggle (ON/OFF)
    fn is_toggle(&self) -> bool;
    /// Whether this field is a text input
    fn is_text_input(&self) -> bool;
    /// Whether this field has a dropdown (not toggle, not text input)
    fn is_dropdown(&self) -> bool {
        !self.is_toggle() && !self.is_text_input()
    }
    /// Get the section this field belongs to (for UI grouping and scroll calculation)
    fn section(&self) -> ConfigSection;
}

/// Section identifiers for config panels.
/// Used to group fields visually and calculate scroll positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigSection {
    /// Default/main section (no header)
    #[default]
    Main,
    /// Serial port settings (baud rate, data bits, etc.)
    Serial,
    /// RX chunking settings
    RxChunking,
    /// TX chunking settings
    TxChunking,
    /// File saving settings  
    FileSave,
    /// Traffic display settings
    TrafficDisplay,
    /// Filtering settings
    Filtering,
    /// Graph settings
    GraphSettings,
    /// Send tab - File Send settings
    SendFile,
    /// Send tab - Quick Send / Input settings
    SendInput,
}

impl ConfigSection {
    /// Get the display name for this section (used as separator header)
    pub fn header(&self) -> Option<&'static str> {
        match self {
            ConfigSection::Main => None,
            ConfigSection::Serial => None, // First section, no header needed
            ConfigSection::RxChunking => Some("RX Chunking"),
            ConfigSection::TxChunking => Some("TX Chunking"),
            ConfigSection::FileSave => Some("File Saving"),
            ConfigSection::TrafficDisplay => None, // First section, no header needed
            ConfigSection::Filtering => Some("Filtering"),
            ConfigSection::GraphSettings => None, // First section, no header needed
            ConfigSection::SendFile => None, // First section in Send tab
            ConfigSection::SendInput => Some("Input Settings"),
        }
    }
}

/// Shared state for config panel UI (dropdown index, scroll offset, visibility)
#[derive(Debug, Clone, Default)]
pub struct ConfigPanelState<F: ConfigFieldKind> {
    /// Whether the config panel is visible
    pub visible: bool,
    /// Which config field is selected
    pub field: F,
    /// Dropdown selection index (when dropdown is open)
    pub dropdown_index: usize,
    /// Scroll offset for the config panel list
    pub scroll_offset: usize,
    /// Visual line index of each field (populated during render for scroll calculation)
    /// Maps field index -> visual line index
    field_line_positions: Vec<usize>,
}

impl<F: ConfigFieldKind> ConfigPanelState<F> {
    /// Create a new config panel state
    pub fn new() -> Self {
        Self {
            visible: false,
            field: F::default(),
            dropdown_index: 0,
            scroll_offset: 0,
            field_line_positions: Vec::new(),
        }
    }
    
    /// Create with initial visibility
    pub fn with_visible(visible: bool) -> Self {
        Self {
            visible,
            ..Self::new()
        }
    }
    
    /// Move to next field
    pub fn next_field(&mut self) {
        self.field = self.field.next();
    }
    
    /// Move to previous field
    pub fn prev_field(&mut self) {
        self.field = self.field.prev();
    }
    
    /// Open dropdown with current index
    pub fn open_dropdown(&mut self, current_index: usize) {
        self.dropdown_index = current_index;
    }
    
    /// Update the visual line positions of all fields.
    /// Call this during render after building the line list.
    pub fn set_field_line_positions(&mut self, positions: Vec<usize>) {
        self.field_line_positions = positions;
    }
    
    /// Get the visual line position for the currently selected field.
    /// Returns None if positions haven't been set yet.
    pub fn current_field_line(&self) -> Option<usize> {
        self.field_line_positions.get(self.field.index()).copied()
    }
    
    /// Adjust scroll offset to ensure the selected field is visible.
    /// Uses the cached line positions from the last render.
    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if let Some(line_idx) = self.current_field_line() {
            // Ensure the selected line is visible
            if line_idx < self.scroll_offset {
                self.scroll_offset = line_idx;
            } else if line_idx >= self.scroll_offset + visible_height {
                self.scroll_offset = line_idx.saturating_sub(visible_height - 1);
            }
        }
    }
}

// =============================================================================
// ConfigOption Implementations for External Types
// =============================================================================

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

impl ConfigOption for PatternMode {
    fn all_variants() -> &'static [Self] { PatternMode::all() }
    fn display_name(&self) -> &'static str { self.name() }
}

// =============================================================================
// Chunking Configuration Types
// =============================================================================

/// Chunking mode for UI selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum ChunkingMode {
    /// Raw chunking - chunks based on OS read timing
    #[default]
    Raw,
    /// Line-delimited chunking - splits on delimiter
    #[strum(serialize = "Line Delimited")]
    LineDelimited,
}

/// Delimiter option for UI selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum DelimiterOption {
    /// Unix-style newline: \n
    #[default]
    #[strum(serialize = "\\n (LF)")]
    Newline,
    /// Windows-style: \r\n
    #[strum(serialize = "\\r\\n (CRLF)")]
    CrLf,
    /// Carriage return only: \r
    #[strum(serialize = "\\r (CR)")]
    Cr,
    /// Custom delimiter (entered as text)
    Custom,
}

/// Size unit for max line length
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum SizeUnit {
    #[strum(serialize = "B")]
    Bytes,
    #[default]
    KiB,
    MiB,
}

impl SizeUnit {
    /// Convert a value with this unit to bytes
    pub fn to_bytes(&self, value: usize) -> usize {
        match self {
            SizeUnit::Bytes => value,
            SizeUnit::KiB => value * 1024,
            SizeUnit::MiB => value * 1024 * 1024,
        }
    }
}

// =============================================================================
// File Save Settings
// =============================================================================

/// Settings for file saving (shared between port selection and traffic view)
#[derive(Debug, Clone, Default)]
pub struct FileSaveSettings {
    /// Whether file saving is enabled
    pub enabled: bool,
    /// Save format
    pub format: SaveFormat,
    /// Custom filename (empty = auto-generated)
    pub filename: String,
    /// Save directory
    pub directory: String,
}

impl FileSaveSettings {
    /// Create new file save settings with default directory
    pub fn new() -> Self {
        Self {
            enabled: false,
            format: SaveFormat::default(),
            filename: String::new(),
            directory: default_save_directory(),
        }
    }
}

/// Get the default save directory (user's home directory or current directory)
fn default_save_directory() -> String {
    std::env::var("HOME")
        .map(|h| format!("{}/serial-logs", h))
        .unwrap_or_else(|_| ".".to_string())
}

// =============================================================================
// View and Connection Enums
// =============================================================================

/// Current view/screen (pre-connection vs post-connection)
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Port selection screen (pre-connection)
    PortSelect,
    /// Connected view with tabs (post-connection)
    Connected,
}

/// Connection state
#[derive(Debug)]
pub enum ConnectionState {
    Disconnected,
    Connected(serial_core::SessionHandle),
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

// =============================================================================
// Focus Enums
// =============================================================================

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

/// Which panel is focused in graph view
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GraphFocus {
    /// Graph display panel (left)
    #[default]
    Graph,
    /// Configuration panel (right)
    Config,
}

// =============================================================================
// Config Field Enums
// =============================================================================

/// Which configuration field is selected in port selection
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount, VariantArray, IntoStaticStr)]
pub enum ConfigField {
    // Serial port settings (section: Serial)
    #[default]
    #[strum(serialize = "Baud Rate")]
    BaudRate,
    #[strum(serialize = "Data Bits")]
    DataBits,
    Parity,
    #[strum(serialize = "Stop Bits")]
    StopBits,
    #[strum(serialize = "Flow Ctrl")]
    FlowControl,
    // RX Chunking fields (section: RxChunking)
    #[strum(serialize = "RX Mode")]
    RxChunkingMode,
    #[strum(serialize = "RX Delimiter")]
    RxDelimiter,
    #[strum(serialize = "RX Custom")]
    RxCustomDelimiter,
    #[strum(serialize = "RX Max Length")]
    RxMaxLineLength,
    #[strum(serialize = "Unit")]
    RxMaxLineLengthUnit,
    // TX Chunking fields (section: TxChunking)
    #[strum(serialize = "TX Mode")]
    TxChunkingMode,
    #[strum(serialize = "TX Delimiter")]
    TxDelimiter,
    #[strum(serialize = "TX Custom")]
    TxCustomDelimiter,
    #[strum(serialize = "TX Max Length")]
    TxMaxLineLength,
    #[strum(serialize = "Unit")]
    TxMaxLineLengthUnit,
    // File saving fields (section: FileSave)
    #[strum(serialize = "Save to File")]
    SaveEnabled,
    #[strum(serialize = "Save Format")]
    SaveFormat,
    #[strum(serialize = "Filename")]
    SaveFilename,
    #[strum(serialize = "Directory")]
    SaveDirectory,
}

impl EnumNavigation for ConfigField {}

impl ConfigField {
    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            ConfigField::SaveFilename
                | ConfigField::SaveDirectory
                | ConfigField::RxCustomDelimiter
                | ConfigField::TxCustomDelimiter
                | ConfigField::RxMaxLineLength
                | ConfigField::TxMaxLineLength
        )
    }

    /// Whether this field is a numeric-only text input
    pub fn is_numeric_input(&self) -> bool {
        matches!(
            self,
            ConfigField::RxMaxLineLength | ConfigField::TxMaxLineLength
        )
    }

    /// Whether this field is a simple toggle
    pub fn is_toggle(&self) -> bool {
        matches!(self, ConfigField::SaveEnabled)
    }

    /// Get the section this field belongs to
    pub fn section(&self) -> ConfigSection {
        match self {
            // Serial port settings
            ConfigField::BaudRate
            | ConfigField::DataBits
            | ConfigField::Parity
            | ConfigField::StopBits
            | ConfigField::FlowControl => ConfigSection::Serial,
            // RX Chunking
            ConfigField::RxChunkingMode
            | ConfigField::RxDelimiter
            | ConfigField::RxCustomDelimiter
            | ConfigField::RxMaxLineLength
            | ConfigField::RxMaxLineLengthUnit => ConfigSection::RxChunking,
            // TX Chunking
            ConfigField::TxChunkingMode
            | ConfigField::TxDelimiter
            | ConfigField::TxCustomDelimiter
            | ConfigField::TxMaxLineLength
            | ConfigField::TxMaxLineLengthUnit => ConfigSection::TxChunking,
            // File saving
            ConfigField::SaveEnabled
            | ConfigField::SaveFormat
            | ConfigField::SaveFilename
            | ConfigField::SaveDirectory => ConfigSection::FileSave,
        }
    }

    /// Check if this is a file saving field (for section grouping)
    pub fn is_file_saving_field(&self) -> bool {
        self.section() == ConfigSection::FileSave
    }

    /// Check if this is an RX chunking field (for section grouping)
    pub fn is_rx_chunking_field(&self) -> bool {
        self.section() == ConfigSection::RxChunking
    }

    /// Check if this is a TX chunking field (for section grouping)
    pub fn is_tx_chunking_field(&self) -> bool {
        self.section() == ConfigSection::TxChunking
    }

    /// Check if this is a serial config field
    pub fn is_serial_field(&self) -> bool {
        self.section() == ConfigSection::Serial
    }
}

impl ConfigFieldKind for ConfigField {
    fn is_toggle(&self) -> bool { ConfigField::is_toggle(self) }
    fn is_text_input(&self) -> bool { ConfigField::is_text_input(self) }
    fn section(&self) -> ConfigSection { ConfigField::section(self) }
}

/// Which configuration field is selected in traffic view config panel
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount, VariantArray, IntoStaticStr)]
pub enum TrafficConfigField {
    // Display settings (section: TrafficDisplay)
    #[default]
    #[strum(serialize = "Line Numbers")]
    LineNumbers,
    Timestamps,
    #[strum(serialize = "Time Format")]
    TimestampFormat,
    #[strum(serialize = "Auto-scroll")]
    AutoScroll,
    #[strum(serialize = "Lock Bottom")]
    LockToBottom,
    Encoding,
    #[strum(serialize = "Wrap Mode")]
    WrapMode,
    #[strum(serialize = "Show TX")]
    ShowTx,
    #[strum(serialize = "Show RX")]
    ShowRx,
    #[strum(serialize = "Hex Grouping")]
    HexGrouping,
    // Filtering fields (section: Filtering)
    #[strum(serialize = "Filter")]
    FilterEnabled,
    #[strum(serialize = "Filter Pattern")]
    FilterPattern,
    // File saving fields (section: FileSave)
    #[strum(serialize = "Save to File")]
    SaveEnabled,
    #[strum(serialize = "Save Format")]
    SaveFormat,
    #[strum(serialize = "Filename")]
    SaveFilename,
    #[strum(serialize = "Directory")]
    SaveDirectory,
}

impl EnumNavigation for TrafficConfigField {}

impl TrafficConfigField {
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
                | TrafficConfigField::FilterEnabled
                | TrafficConfigField::SaveEnabled
        )
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            TrafficConfigField::SaveFilename 
                | TrafficConfigField::SaveDirectory
                | TrafficConfigField::FilterPattern
        )
    }

    /// Get the associated TrafficCommand for this field (if any)
    /// This is used to look up the keyboard shortcut from the command system
    pub fn associated_command(&self) -> Option<crate::command::TrafficCommand> {
        use crate::command::TrafficCommand;
        match self {
            TrafficConfigField::LineNumbers => Some(TrafficCommand::ToggleLineNumbers),
            TrafficConfigField::Timestamps => Some(TrafficCommand::ToggleTimestamps),
            TrafficConfigField::Encoding => Some(TrafficCommand::CycleEncoding),
            _ => None,
        }
    }

    /// Get the section this field belongs to
    pub fn section(&self) -> ConfigSection {
        match self {
            // Display settings
            TrafficConfigField::LineNumbers
            | TrafficConfigField::Timestamps
            | TrafficConfigField::TimestampFormat
            | TrafficConfigField::AutoScroll
            | TrafficConfigField::LockToBottom
            | TrafficConfigField::Encoding
            | TrafficConfigField::WrapMode
            | TrafficConfigField::ShowTx
            | TrafficConfigField::ShowRx
            | TrafficConfigField::HexGrouping => ConfigSection::TrafficDisplay,
            // Filtering
            TrafficConfigField::FilterEnabled
            | TrafficConfigField::FilterPattern => ConfigSection::Filtering,
            // File saving
            TrafficConfigField::SaveEnabled
            | TrafficConfigField::SaveFormat
            | TrafficConfigField::SaveFilename
            | TrafficConfigField::SaveDirectory => ConfigSection::FileSave,
        }
    }

    /// Check if this is a file saving field (for section grouping)
    pub fn is_file_saving_field(&self) -> bool {
        self.section() == ConfigSection::FileSave
    }

    /// Check if this is a filtering field (for section grouping)
    pub fn is_filtering_field(&self) -> bool {
        self.section() == ConfigSection::Filtering
    }
}

impl ConfigFieldKind for TrafficConfigField {
    fn is_toggle(&self) -> bool { TrafficConfigField::is_toggle(self) }
    fn is_text_input(&self) -> bool { TrafficConfigField::is_text_input(self) }
    fn section(&self) -> ConfigSection { TrafficConfigField::section(self) }
}

/// Which configuration field is selected in graph view config panel
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount, VariantArray, IntoStaticStr)]
pub enum GraphConfigField {
    // Graph settings (section: GraphSettings)
    #[default]
    Mode,
    Parser,
    #[strum(serialize = "Regex Pattern")]
    RegexPattern,
    #[strum(serialize = "Time Window")]
    TimeWindow,
    #[strum(serialize = "Show RX")]
    ShowRx,
    #[strum(serialize = "Show TX")]
    ShowTx,
}

impl EnumNavigation for GraphConfigField {}

impl GraphConfigField {
    /// Whether this field is a simple toggle (vs a dropdown)
    pub fn is_toggle(&self) -> bool {
        matches!(self, GraphConfigField::ShowRx | GraphConfigField::ShowTx)
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(self, GraphConfigField::RegexPattern)
    }

    /// Get the section this field belongs to
    pub fn section(&self) -> ConfigSection {
        ConfigSection::GraphSettings
    }
}

impl ConfigFieldKind for GraphConfigField {
    fn is_toggle(&self) -> bool { GraphConfigField::is_toggle(self) }
    fn is_text_input(&self) -> bool { GraphConfigField::is_text_input(self) }
    fn section(&self) -> ConfigSection { GraphConfigField::section(self) }
}

/// Which configuration field is selected in send view config panel
#[derive(Debug, Clone, Copy, PartialEq, Default, EnumIter, EnumCount, VariantArray, IntoStaticStr)]
pub enum SendConfigField {
    // File send settings (section: SendFile)
    #[default]
    #[strum(serialize = "File Path")]
    FilePath,
    #[strum(serialize = "Chunk Size")]
    ChunkSize,
    #[strum(serialize = "Chunk Delay (ms)")]
    ChunkDelay,
    #[strum(serialize = "Continuous")]
    Continuous,
    // Input settings (section: SendInput)
    #[strum(serialize = "Line Ending")]
    LineEnding,
    #[strum(serialize = "Input Mode")]
    InputEncoding,
}

impl EnumNavigation for SendConfigField {}

impl SendConfigField {
    /// Whether this field is a simple toggle (vs a dropdown)
    pub fn is_toggle(&self) -> bool {
        matches!(self, SendConfigField::Continuous)
    }

    /// Whether this field is a text input field
    pub fn is_text_input(&self) -> bool {
        matches!(
            self, 
            SendConfigField::FilePath 
                | SendConfigField::ChunkSize 
                | SendConfigField::ChunkDelay
        )
    }
    
    /// Whether this is a numeric-only text input
    pub fn is_numeric_input(&self) -> bool {
        matches!(
            self,
            SendConfigField::ChunkSize | SendConfigField::ChunkDelay
        )
    }

    /// Get the section this field belongs to
    pub fn section(&self) -> ConfigSection {
        match self {
            SendConfigField::FilePath
            | SendConfigField::ChunkSize
            | SendConfigField::ChunkDelay
            | SendConfigField::Continuous => ConfigSection::SendFile,
            SendConfigField::LineEnding
            | SendConfigField::InputEncoding => ConfigSection::SendInput,
        }
    }
}

impl ConfigFieldKind for SendConfigField {
    fn is_toggle(&self) -> bool { SendConfigField::is_toggle(self) }
    fn is_text_input(&self) -> bool { SendConfigField::is_text_input(self) }
    fn section(&self) -> ConfigSection { SendConfigField::section(self) }
}

/// Line ending options for sending data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum LineEndingOption {
    /// No line ending appended
    None,
    /// Unix-style newline: \n
    #[default]
    #[strum(serialize = "LF (\\n)")]
    Lf,
    /// Windows-style: \r\n
    #[strum(serialize = "CRLF (\\r\\n)")]
    CrLf,
    /// Carriage return only: \r
    #[strum(serialize = "CR (\\r)")]
    Cr,
}

impl LocalStrumEnum for LineEndingOption {}

impl LineEndingOption {
    /// Get the bytes to append for this line ending
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEndingOption::None => &[],
            LineEndingOption::Lf => b"\n",
            LineEndingOption::CrLf => b"\r\n",
            LineEndingOption::Cr => b"\r",
        }
    }
}

/// Input encoding mode for sending data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum InputEncodingMode {
    /// Text mode - send as UTF-8
    #[default]
    Text,
    /// Hex mode - parse hex bytes like "DE AD BE EF"
    Hex,
}

impl LocalStrumEnum for InputEncodingMode {}

/// Which panel is focused in send view
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SendFocus {
    /// Main send content panel (left)
    #[default]
    Content,
    /// Configuration panel (right)
    Config,
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
        use strum::IntoEnumIterator;
        let variants: Vec<_> = Self::iter().collect();
        let idx = variants.iter().position(|&v| v == self).unwrap_or(0);
        variants[(idx + 1) % variants.len()]
    }

    pub fn prev(self) -> Self {
        use strum::IntoEnumIterator;
        let variants: Vec<_> = Self::iter().collect();
        let idx = variants.iter().position(|&v| v == self).unwrap_or(0);
        variants[(idx + variants.len() - 1) % variants.len()]
    }

    pub fn index(self) -> usize {
        use strum::IntoEnumIterator;
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

// =============================================================================
// Input Mode
// =============================================================================

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
    /// Graph config dropdown is open
    GraphConfigDropdown,
    /// Send config dropdown is open
    SendConfigDropdown,
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
    /// Editing a graph config text field (e.g., regex pattern)
    GraphConfigTextInput,
    /// Editing a send config text field (e.g., file path, chunk size)
    SendConfigTextInput,
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
            InputMode::GraphConfigDropdown => "j/k: navigate, Enter: select, Esc: cancel",
            InputMode::SendConfigDropdown => "j/k: navigate, Enter: select, Esc: cancel",
            InputMode::SettingsDropdown => "j/k: navigate, Enter: select, Esc: cancel",
            InputMode::WindowCommand => "Ctrl+W: v=vsplit, q=close, h/l=navigate",
            InputMode::CommandLine => "",
            InputMode::SplitSelect => "", // Dynamic based on available splits
            InputMode::ConfigTextInput => "Enter value (Enter: confirm, Esc: cancel)",
            InputMode::TrafficConfigTextInput => "Enter value (Enter: confirm, Esc: cancel)",
            InputMode::GraphConfigTextInput => "Enter regex pattern (Enter: confirm, Esc: cancel)",
            InputMode::SendConfigTextInput => "Enter value (Enter: confirm, Esc: cancel)",
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
            InputMode::GraphConfigDropdown => None,    // Uses special rendering
            InputMode::SendConfigDropdown => None,     // Uses special rendering
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
            InputMode::GraphConfigTextInput => Some(InputModeStyle {
                prefix: "",
                color: Color::Cyan,
            }),
            InputMode::SendConfigTextInput => Some(InputModeStyle {
                prefix: "",
                color: Color::Cyan,
            }),
        }
    }
}

// =============================================================================
// Display Format Types
// =============================================================================

/// Format for displaying timestamps in traffic view
#[derive(Debug, Clone, Copy, PartialEq, Default, VariantArray, IntoStaticStr)]
pub enum TimestampFormat {
    /// Relative time since session start (e.g., "+1.234s")
    #[default]
    Relative,
    /// Absolute time with milliseconds (e.g., "12:34:56.789")
    #[strum(serialize = "HH:MM:SS.mmm")]
    AbsoluteMillis,
    /// Absolute time without milliseconds (e.g., "12:34:56")
    #[strum(serialize = "HH:MM:SS")]
    Absolute,
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
#[derive(Debug, Clone, Copy, PartialEq, Default, VariantArray, IntoStaticStr)]
pub enum WrapMode {
    /// Wrap long lines to fit the terminal width
    #[default]
    Wrap,
    /// Truncate long lines (with ellipsis indicator)
    Truncate,
}

/// Hex byte grouping for hex encoding display
#[derive(Debug, Clone, Copy, PartialEq, Default, VariantArray, IntoStaticStr)]
pub enum HexGrouping {
    /// No grouping (continuous hex)
    None,
    /// Group by 1 byte (space every byte)
    #[default]
    #[strum(serialize = "1 byte")]
    Byte,
    /// Group by 2 bytes (space every 2 bytes)
    #[strum(serialize = "2 bytes")]
    Word,
    /// Group by 4 bytes (space every 4 bytes)
    #[strum(serialize = "4 bytes")]
    DWord,
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
