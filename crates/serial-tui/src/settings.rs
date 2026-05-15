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
            file_save_encoding_index: 0, // UTF-8 (first in encoding array)
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
    /// Show delimiter escape sequences (e.g., \n, \r\n).
    pub show_delimiter: bool,
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
    /// Whether to append a suffix when sending data.
    pub send_suffix_enabled: bool,
    /// The suffix to append (with escape sequences like \r\n).
    pub send_suffix: String,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            encoding_index: 0, // UTF-8
            show_tx: true,
            show_rx: true,
            show_delimiter: true,
            show_timestamps: true,
            timestamp_format_index: 0, // Relative
            auto_scroll: true,
            lock_to_bottom: false,
            search_mode_index: 0, // Normal
            filter_mode_index: 0, // Normal
            wrap_text: true,
            file_save_enabled: false,
            file_save_format_index: 1,   // Encoded
            file_save_encoding_index: 0, // UTF-8
            file_save_directory: serial_core::buffer::default_cache_directory()
                .to_string_lossy()
                .into_owned(),
            send_suffix_enabled: true,
            send_suffix: r"\r\n".to_string(),
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
    /// Legacy time range preset index: 0=All, 1=1 Hour, 2=5 Min, 3=Custom.
    pub time_range_index: usize,
    /// Time range value.
    pub custom_time_value: usize,
    /// Time range unit index: 0=seconds, 1=minutes, 2=hours.
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

impl GraphSettings {
    /// Return the persisted graph time range as value plus unit.
    pub fn time_range_value_and_unit(&self) -> (usize, usize) {
        match self.time_range_index {
            1 => (1, 2), // 1 hour
            2 => (5, 1), // 5 minutes
            _ => (
                self.custom_time_value.max(1),
                self.custom_time_unit_index.min(2),
            ),
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

    // === Scrollback settings ===
    /// Whether retained traffic history has a memory limit.
    pub scrollback_limit_enabled: bool,
    /// Numeric scrollback limit value.
    pub scrollback_limit_value: usize,
    /// Scrollback limit unit index.
    pub scrollback_limit_unit_index: usize,
    /// Legacy preset index from settings files saved before scrollback became configurable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_size_index: Option<usize>,

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
            auto_save_encoding_index: 0, // UTF-8
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

            // Scrollback defaults
            scrollback_limit_enabled: true,
            scrollback_limit_value: 10,
            scrollback_limit_unit_index: 1, // MB
            buffer_size_index: None,

            // System defaults
            keep_awake: true,
        }
    }
}

/// Scrollback size units in bytes.
pub const SCROLLBACK_UNIT_BYTES: &[usize] = &[
    1024,               // KB
    1024 * 1024,        // MB
    1024 * 1024 * 1024, // GB
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
            _ => Encoding::Utf8,
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

    /// Get the scrollback limit in bytes (usize::MAX for unlimited).
    pub fn scrollback_limit_bytes(&self) -> usize {
        if !self.scrollback_limit_enabled {
            return usize::MAX;
        }

        let unit = SCROLLBACK_UNIT_BYTES
            .get(self.scrollback_limit_unit_index)
            .copied()
            .unwrap_or(1024 * 1024);

        self.scrollback_limit_value.max(1).saturating_mul(unit)
    }

    /// Migrate old fixed buffer-size presets to the configurable scrollback limit.
    fn migrate_legacy_buffer_size(&mut self) {
        let Some(index) = self.buffer_size_index.take() else {
            return;
        };

        match index {
            0 => {
                self.scrollback_limit_enabled = true;
                self.scrollback_limit_value = 1;
                self.scrollback_limit_unit_index = 1;
            }
            1 => {
                self.scrollback_limit_enabled = true;
                self.scrollback_limit_value = 5;
                self.scrollback_limit_unit_index = 1;
            }
            2 => {
                self.scrollback_limit_enabled = true;
                self.scrollback_limit_value = 10;
                self.scrollback_limit_unit_index = 1;
            }
            3 => {
                self.scrollback_limit_enabled = true;
                self.scrollback_limit_value = 50;
                self.scrollback_limit_unit_index = 1;
            }
            4 => {
                self.scrollback_limit_enabled = true;
                self.scrollback_limit_value = 100;
                self.scrollback_limit_unit_index = 1;
            }
            5 => {
                self.scrollback_limit_enabled = false;
            }
            _ => {}
        }
    }
}

impl TuiSettings {
    /// Load settings from the config directory.
    ///
    /// Returns default settings if the file doesn't exist or cannot be parsed.
    pub fn load() -> Self {
        let config_dir = settings::config_directory(APP_NAME);
        match settings::load_or_default::<Self>(&config_dir, SETTINGS_FILE) {
            Ok(mut settings) => {
                settings.global.migrate_legacy_buffer_size();
                settings
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scrollback_limit_is_ten_mb() {
        assert_eq!(
            GlobalSettings::default().scrollback_limit_bytes(),
            10 * 1024 * 1024
        );
    }

    #[test]
    fn disabled_scrollback_limit_is_unlimited() {
        let settings = GlobalSettings {
            scrollback_limit_enabled: false,
            ..Default::default()
        };

        assert_eq!(settings.scrollback_limit_bytes(), usize::MAX);
    }

    #[test]
    fn scrollback_limit_uses_selected_unit() {
        let settings = GlobalSettings {
            scrollback_limit_value: 2,
            scrollback_limit_unit_index: 2,
            ..Default::default()
        };

        assert_eq!(settings.scrollback_limit_bytes(), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn legacy_buffer_size_preset_migrates_to_configurable_limit() {
        let mut settings = GlobalSettings {
            buffer_size_index: Some(3),
            ..Default::default()
        };

        settings.migrate_legacy_buffer_size();

        assert!(settings.scrollback_limit_enabled);
        assert_eq!(settings.scrollback_limit_value, 50);
        assert_eq!(settings.scrollback_limit_unit_index, 1);
        assert_eq!(settings.buffer_size_index, None);
    }

    #[test]
    fn legacy_unlimited_preset_migrates_to_disabled_limit() {
        let mut settings = GlobalSettings {
            buffer_size_index: Some(5),
            ..Default::default()
        };

        settings.migrate_legacy_buffer_size();

        assert!(!settings.scrollback_limit_enabled);
        assert_eq!(settings.scrollback_limit_bytes(), usize::MAX);
        assert_eq!(settings.buffer_size_index, None);
    }
}
