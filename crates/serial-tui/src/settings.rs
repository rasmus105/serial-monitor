//! Persistent settings for the TUI application.
//!
//! Settings are saved to `~/.config/serial-monitor-tui/settings.toml` on Linux,
//! with platform-appropriate paths on other systems.

use serde::{Deserialize, Serialize};
use serial_core::{
    DataBits, settings,
    ui::serial_config::{COMMON_BAUD_RATES, DATA_BITS_VARIANTS},
};

const APP_NAME: &str = "serial-monitor-tui";
const SETTINGS_FILE: &str = "settings.toml";

/// All persistent settings for the TUI application.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuiSettings {
    /// Pre-connection view settings (serial config defaults).
    pub pre_connect: PreConnectSettings,
    /// Traffic view settings.
    pub traffic: TrafficSettings,
    /// Graph view settings.
    pub graph: GraphSettings,
    /// File sender settings.
    pub file_sender: FileSenderSettings,
    /// Global application settings.
    pub global: GlobalSettings,
}

/// Pre-connection view settings (serial port defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PreConnectSettings {
    /// Baud rate index into COMMON_BAUD_RATES.
    pub baud_rate_index: usize,
    /// Data bits index.
    pub data_bits_index: usize,
    /// Parity index.
    pub parity_index: usize,
    /// Stop bits index.
    pub stop_bits_index: usize,
    /// Flow control index.
    pub flow_control_index: usize,
    /// Line ending index for RX chunking.
    pub line_ending_index: usize,
    /// File saving enabled by default.
    pub file_save_enabled: bool,
    /// File save format index.
    pub file_save_format_index: usize,
    /// File save encoding index.
    pub file_save_encoding_index: usize,
    /// File save directory.
    pub file_save_directory: String,
}

impl Default for PreConnectSettings {
    fn default() -> Self {
        Self {
            // Use position lookup for array-dependent indices to avoid hardcoded magic numbers
            baud_rate_index: COMMON_BAUD_RATES
                .iter()
                .position(|&r| r == 115200)
                .unwrap_or(8),
            data_bits_index: DATA_BITS_VARIANTS
                .iter()
                .position(|&d| d == DataBits::Eight)
                .unwrap_or(3),
            parity_index: 0,       // None (first in array)
            stop_bits_index: 0,    // 1 (first in array)
            flow_control_index: 0, // None (first in array)
            line_ending_index: 1,  // LF (second in LINE_ENDINGS array)
            file_save_enabled: false,
            file_save_format_index: 1,   // Encoded (second in format array)
            file_save_encoding_index: 1, // ASCII (second in encoding array)
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Traffic view settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrafficSettings {
    /// Display encoding index.
    pub encoding_index: usize,
    /// Show TX data.
    pub show_tx: bool,
    /// Show RX data.
    pub show_rx: bool,
    /// Show timestamps.
    pub show_timestamps: bool,
    /// Timestamp format index.
    pub timestamp_format_index: usize,
    /// Auto-scroll to bottom.
    pub auto_scroll: bool,
    /// Lock to bottom (prevent scroll up).
    pub lock_to_bottom: bool,
    /// Search mode index (Normal/Regex).
    pub search_mode_index: usize,
    /// Filter mode index (Normal/Regex).
    pub filter_mode_index: usize,
    /// Wrap long lines.
    pub wrap_text: bool,
    /// File saving enabled.
    pub file_save_enabled: bool,
    /// File save format index.
    pub file_save_format_index: usize,
    /// File save encoding index.
    pub file_save_encoding_index: usize,
    /// File save directory.
    pub file_save_directory: String,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            encoding_index: 0, // UTF-8
            show_tx: true,
            show_rx: true,
            show_timestamps: true,
            timestamp_format_index: 0, // Relative
            auto_scroll: true,
            lock_to_bottom: false,
            search_mode_index: 0, // Normal
            filter_mode_index: 0, // Normal
            wrap_text: true,
            file_save_enabled: false,
            file_save_format_index: 1,   // Encoded
            file_save_encoding_index: 1, // ASCII
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Graph view settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphSettings {
    /// Graph mode: 0=Parsed Data, 1=RX/TX Rate.
    pub mode_index: usize,
    /// Parser type index.
    pub parser_type_index: usize,
    /// Regex pattern for regex parser.
    pub regex_pattern: String,
    /// CSV delimiter index.
    pub csv_delimiter_index: usize,
    /// CSV column names.
    pub csv_columns: String,
    /// Parse RX (received) data for graphing.
    pub parse_rx: bool,
    /// Parse TX (transmitted) data for graphing.
    pub parse_tx: bool,
    /// Show RX rate (for rate mode).
    pub show_rx: bool,
    /// Show TX rate (for rate mode).
    pub show_tx: bool,
    /// Time range preset index.
    pub time_range_index: usize,
    /// Custom time value.
    pub custom_time_value: usize,
    /// Custom time unit index.
    pub custom_time_unit_index: usize,
}

impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            mode_index: 0,        // Parsed Data
            parser_type_index: 0, // Smart
            regex_pattern: String::new(),
            csv_delimiter_index: 0, // Comma
            csv_columns: String::new(),
            parse_rx: true,
            parse_tx: false,
            show_rx: true,
            show_tx: true,
            time_range_index: 0, // All
            custom_time_value: 60,
            custom_time_unit_index: 1, // minutes
        }
    }
}

/// File sender settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSenderSettings {
    /// Chunking mode: 0 = Delimiter, 1 = Bytes
    pub chunk_mode_index: usize,
    /// Delimiter index (for delimiter mode)
    pub delimiter_index: usize,
    /// Whether to include delimiter in sent chunks
    pub include_delimiter: bool,
    /// Number of lines per chunk (for delimiter mode)
    pub lines_per_chunk: usize,
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
    /// Repeat sending the file.
    pub repeat: bool,
    /// Preview size limit value
    pub preview_limit_value: usize,
    /// Preview size limit unit index (0=KB, 1=MB)
    pub preview_limit_unit_index: usize,
    /// Auto-follow current chunk during sending
    pub auto_follow: bool,
}

impl Default for FileSenderSettings {
    fn default() -> Self {
        Self {
            chunk_mode_index: 0, // Delimiter
            delimiter_index: 0,  // LF
            include_delimiter: true,
            lines_per_chunk: 1,
            byte_chunk_value: 64,
            byte_unit_index: 0, // Bytes
            append_suffix: false,
            suffix_delimiter_index: 0, // LF
            delay_value: 10,
            delay_unit_index: 0, // Milliseconds
            repeat: false,
            preview_limit_value: 1,      // 1 MB default
            preview_limit_unit_index: 1, // MB
            auto_follow: false,          // Follow current chunk by default
        }
    }
}

/// Global application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    // === Auto-save (crash recovery) settings ===
    /// Whether auto-save is enabled.
    pub auto_save_enabled: bool,
    /// Maximum number of session files to keep.
    pub auto_save_max_sessions: usize,
    /// Format index: 0=Raw, 1=Encoded.
    pub auto_save_format_index: usize,
    /// Encoding index (when format=Encoded).
    pub auto_save_encoding_index: usize,
    /// Include timestamps in auto-save.
    pub auto_save_timestamps: bool,
    /// Include direction markers in auto-save.
    pub auto_save_direction: bool,
    /// Save RX data.
    pub auto_save_rx: bool,
    /// Save TX data.
    pub auto_save_tx: bool,

    // === File saving (user-initiated) settings ===
    /// Save scope index.
    pub file_save_scope_index: usize,
    /// Save RX data.
    pub file_save_rx: bool,
    /// Save TX data.
    pub file_save_tx: bool,
    /// Include timestamps.
    pub file_save_timestamps: bool,
    /// Include direction markers.
    pub file_save_direction: bool,

    // === Pattern matching defaults ===
    /// Default search mode.
    pub search_mode_index: usize,
    /// Default filter mode.
    pub filter_mode_index: usize,

    // === Buffer settings ===
    /// Buffer size index.
    pub buffer_size_index: usize,

    // === System settings ===
    /// Keep system awake while connected.
    pub keep_awake: bool,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            // Auto-save defaults
            auto_save_enabled: true,
            auto_save_max_sessions: 10,
            auto_save_format_index: 1,   // Encoded
            auto_save_encoding_index: 1, // ASCII
            auto_save_timestamps: true,
            auto_save_direction: false,
            auto_save_rx: true,
            auto_save_tx: false,

            // File saving defaults
            file_save_scope_index: 2, // ExistingAndContinue
            file_save_rx: true,
            file_save_tx: true,
            file_save_timestamps: true,
            file_save_direction: true,

            // Pattern matching defaults
            search_mode_index: 0, // Normal
            filter_mode_index: 0, // Normal

            // Buffer defaults
            buffer_size_index: 2, // 10 MB

            // System defaults
            keep_awake: true,
        }
    }
}

/// Buffer sizes in bytes corresponding to buffer_size_index options.
pub const BUFFER_SIZES: &[usize] = &[
    1024 * 1024,       // 1 MB
    5 * 1024 * 1024,   // 5 MB
    10 * 1024 * 1024,  // 10 MB
    50 * 1024 * 1024,  // 50 MB
    100 * 1024 * 1024, // 100 MB
    usize::MAX,        // Unlimited
];

impl GlobalSettings {
    /// Convert auto-save settings to AutoSaveConfig for the core.
    pub fn to_auto_save_config(&self) -> serial_core::buffer::AutoSaveConfig {
        use serial_core::buffer::{AutoSaveConfig, DirectionFilter, Encoding, SaveFormat};

        // Map encoding index to Encoding enum
        let encoding = match self.auto_save_encoding_index {
            0 => Encoding::Utf8,
            1 => Encoding::Ascii,
            2 => Encoding::Hex(Default::default()),
            3 => Encoding::Binary(Default::default()),
            _ => Encoding::Ascii,
        };

        // Build save format based on format index
        let format = match self.auto_save_format_index {
            0 => SaveFormat::Raw,
            _ => SaveFormat::Encoded {
                encoding,
                include_timestamps: self.auto_save_timestamps,
                include_direction: self.auto_save_direction,
            },
        };

        AutoSaveConfig {
            enabled: self.auto_save_enabled,
            max_sessions: self.auto_save_max_sessions,
            directions: DirectionFilter {
                tx: self.auto_save_tx,
                rx: self.auto_save_rx,
            },
            format,
            ..Default::default()
        }
    }

    /// Get the buffer size in bytes (usize::MAX for unlimited).
    pub fn buffer_size(&self) -> usize {
        BUFFER_SIZES
            .get(self.buffer_size_index)
            .copied()
            .unwrap_or(usize::MAX)
    }
}

impl TuiSettings {
    /// Load settings from the config directory.
    ///
    /// Returns default settings if the file doesn't exist or cannot be parsed.
    pub fn load() -> Self {
        let config_dir = settings::config_directory(APP_NAME);
        match settings::load_or_default(&config_dir, SETTINGS_FILE) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("Warning: Failed to load settings: {}", e);
                Self::default()
            }
        }
    }

    /// Save settings to the config directory.
    pub fn save(&self) -> Result<(), settings::SettingsError> {
        let config_dir = settings::config_directory(APP_NAME);
        settings::save(&config_dir, SETTINGS_FILE, self)
    }
}
