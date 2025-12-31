//! Persistent settings for the TUI application.
//!
//! Settings are saved to `~/.config/serial-monitor-tui/settings.toml` on Linux,
//! with platform-appropriate paths on other systems.

use serde::{Deserialize, Serialize};
use serial_core::settings;

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
            baud_rate_index: 8,  // 115200
            data_bits_index: 3,  // 8 bits
            parity_index: 0,     // None
            stop_bits_index: 0,  // 1
            flow_control_index: 0, // None
            line_ending_index: 1, // LF
            file_save_enabled: false,
            file_save_format_index: 1, // Encoded
            file_save_encoding_index: 1, // ASCII
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
    /// Pattern mode index (Normal/Regex).
    pub pattern_mode_index: usize,
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
            pattern_mode_index: 0, // Normal
            wrap_text: true,
            file_save_enabled: false,
            file_save_format_index: 1, // Encoded
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
            mode_index: 0, // Parsed Data
            parser_type_index: 0, // KeyValue
            regex_pattern: String::new(),
            csv_delimiter_index: 0, // Comma
            csv_columns: String::new(),
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
    /// Chunk size in bytes.
    pub chunk_size: usize,
    /// Delay between chunks in milliseconds.
    pub delay_ms: usize,
    /// Repeat sending the file.
    pub repeat: bool,
}

impl Default for FileSenderSettings {
    fn default() -> Self {
        Self {
            chunk_size: 64,
            delay_ms: 10,
            repeat: false,
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
            auto_save_format_index: 1, // Encoded
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

// =============================================================================
// Conversions between GlobalSettings and widget::help_overlay::AppSettings
// =============================================================================

use crate::widget::help_overlay::AppSettings;

impl From<GlobalSettings> for AppSettings {
    fn from(g: GlobalSettings) -> Self {
        AppSettings {
            auto_save_enabled: g.auto_save_enabled,
            auto_save_max_sessions: g.auto_save_max_sessions,
            auto_save_format_index: g.auto_save_format_index,
            auto_save_encoding_index: g.auto_save_encoding_index,
            auto_save_timestamps: g.auto_save_timestamps,
            auto_save_direction: g.auto_save_direction,
            auto_save_rx: g.auto_save_rx,
            auto_save_tx: g.auto_save_tx,
            file_save_scope_index: g.file_save_scope_index,
            file_save_rx: g.file_save_rx,
            file_save_tx: g.file_save_tx,
            file_save_timestamps: g.file_save_timestamps,
            file_save_direction: g.file_save_direction,
            search_mode_index: g.search_mode_index,
            filter_mode_index: g.filter_mode_index,
            buffer_size_index: g.buffer_size_index,
            keep_awake: g.keep_awake,
        }
    }
}

impl From<&AppSettings> for GlobalSettings {
    fn from(a: &AppSettings) -> Self {
        GlobalSettings {
            auto_save_enabled: a.auto_save_enabled,
            auto_save_max_sessions: a.auto_save_max_sessions,
            auto_save_format_index: a.auto_save_format_index,
            auto_save_encoding_index: a.auto_save_encoding_index,
            auto_save_timestamps: a.auto_save_timestamps,
            auto_save_direction: a.auto_save_direction,
            auto_save_rx: a.auto_save_rx,
            auto_save_tx: a.auto_save_tx,
            file_save_scope_index: a.file_save_scope_index,
            file_save_rx: a.file_save_rx,
            file_save_tx: a.file_save_tx,
            file_save_timestamps: a.file_save_timestamps,
            file_save_direction: a.file_save_direction,
            search_mode_index: a.search_mode_index,
            filter_mode_index: a.filter_mode_index,
            buffer_size_index: a.buffer_size_index,
            keep_awake: a.keep_awake,
        }
    }
}
