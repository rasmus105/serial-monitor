//! File saving functionality for serial data
//!
//! Provides the ability to save serial traffic to files in various formats.
//! The file saver runs as a background task and receives data chunks to write.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tokio::sync::mpsc;

use strum::{AsRefStr, Display, VariantArray};

use crate::buffer::Direction;
use crate::encoding::{encode, Encoding};

/// Format for saving serial data to files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, AsRefStr, VariantArray)]
pub enum SaveFormat {
    /// UTF-8 text with timestamps and direction markers
    #[default]
    #[strum(serialize = "UTF-8")]
    Utf8,
    /// ASCII text with escape sequences for non-printable characters
    #[strum(serialize = "ASCII")]
    Ascii,
    /// Hexadecimal representation
    #[strum(serialize = "HEX")]
    Hex,
    /// Raw binary data (no encoding, no metadata)
    #[strum(serialize = "Raw")]
    Raw,
}

impl SaveFormat {
    /// Get all available formats
    pub fn all() -> &'static [SaveFormat] {
        Self::VARIANTS
    }

    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            SaveFormat::Utf8 => "txt",
            SaveFormat::Ascii => "txt",
            SaveFormat::Hex => "hex",
            SaveFormat::Raw => "bin",
        }
    }

    /// Convert to the corresponding Encoding (for non-raw formats)
    fn as_encoding(&self) -> Option<Encoding> {
        match self {
            SaveFormat::Utf8 => Some(Encoding::Utf8),
            SaveFormat::Ascii => Some(Encoding::Ascii),
            SaveFormat::Hex => Some(Encoding::Hex),
            SaveFormat::Raw => None,
        }
    }
}

/// Data chunk for file saving (contains raw bytes + metadata)
#[derive(Debug, Clone)]
pub struct SaveChunk {
    pub data: Vec<u8>,
    pub direction: Direction,
    pub timestamp: SystemTime,
}

/// Commands sent to the file saver task
#[derive(Debug)]
pub enum FileSaverCommand {
    /// Write a data chunk to the file
    Write(SaveChunk),
    /// Change the save format (starts appending with new format)
    ChangeFormat(SaveFormat),
    /// Stop saving and close the file
    Stop,
}

/// Handle for interacting with an active file saver
pub struct FileSaverHandle {
    command_tx: mpsc::Sender<FileSaverCommand>,
    file_path: PathBuf,
}

impl FileSaverHandle {
    /// Write a data chunk to the file
    pub fn write(&self, data: Vec<u8>, direction: Direction, timestamp: SystemTime) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::Write(SaveChunk { data, direction, timestamp }))
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Change the save format
    pub fn change_format(&self, format: SaveFormat) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::ChangeFormat(format))
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Stop saving and close the file
    pub fn stop(&self) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::Stop)
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Get the file path being written to
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

impl Drop for FileSaverHandle {
    fn drop(&mut self) {
        // Try to stop the saver when the handle is dropped
        let _ = self.command_tx.try_send(FileSaverCommand::Stop);
    }
}

/// Configuration for file saving
#[derive(Debug, Clone)]
pub struct FileSaveConfig {
    /// Directory to save files to
    pub directory: PathBuf,
    /// Custom filename (None = auto-generated)
    pub filename: Option<String>,
    /// Format for saving data
    pub format: SaveFormat,
    /// Port name (used for auto-generated filenames)
    pub port_name: String,
}

impl FileSaveConfig {
    /// Create a new config with the given directory
    pub fn new(directory: impl Into<PathBuf>, port_name: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            filename: None,
            format: SaveFormat::default(),
            port_name: port_name.into(),
        }
    }

    /// Set a custom filename
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Set the save format
    pub fn with_format(mut self, format: SaveFormat) -> Self {
        self.format = format;
        self
    }

    /// Generate the full file path
    pub fn generate_path(&self) -> PathBuf {
        let filename = self
            .filename
            .clone()
            .unwrap_or_else(|| generate_auto_filename(&self.port_name, self.format));
        self.directory.join(filename)
    }
}

/// Generate an auto filename with ISO 8601 timestamp
/// Format: {port_name}-{timestamp}.{extension}
/// Example: ttyUSB0-2025-12-20T14:30:52.123Z.txt
fn generate_auto_filename(port_name: &str, format: SaveFormat) -> String {
    // Clean up port name for use in filename
    let clean_port_name = port_name
        .replace(['/', '\\'], "_")
        .trim_start_matches("_dev_")
        .to_string();

    // Generate ISO 8601 timestamp
    let timestamp = format_iso8601_timestamp(SystemTime::now());

    format!("{}-{}.{}", clean_port_name, timestamp, format.extension())
}

/// Format a SystemTime as ISO 8601 timestamp (like Node.js toISOString)
/// Format: YYYY-MM-DDTHH:mm:ss.sssZ
fn format_iso8601_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Calculate date/time components
    // Note: This is a simplified calculation that doesn't account for leap seconds
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Convert days since Unix epoch to year, month, day
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Format a timestamp for inclusion in the saved file
fn format_file_timestamp(time: SystemTime) -> String {
    format_iso8601_timestamp(time)
}

/// Start a file saver task
///
/// Returns a handle for sending data to be saved.
///
/// This function must be called with a Tokio runtime handle to spawn the background task.
pub fn start_file_saver(
    config: FileSaveConfig,
    runtime: &tokio::runtime::Handle,
) -> crate::Result<FileSaverHandle> {
    let file_path = config.generate_path();

    // Create directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open the file for writing (create or truncate)
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)?;

    let (command_tx, command_rx) = mpsc::channel(256);

    // Spawn the file saver task on the provided runtime
    let path_clone = file_path.clone();
    runtime.spawn(async move {
        file_saver_task(file, command_rx, config.format, path_clone).await;
    });

    Ok(FileSaverHandle {
        command_tx,
        file_path,
    })
}

/// The async task that handles file writing
async fn file_saver_task(
    file: File,
    mut command_rx: mpsc::Receiver<FileSaverCommand>,
    mut format: SaveFormat,
    file_path: PathBuf,
) {
    let mut writer = BufWriter::new(file);

    while let Some(cmd) = command_rx.recv().await {
        match cmd {
            FileSaverCommand::Write(chunk) => {
                if let Err(e) = write_chunk(&mut writer, &chunk, format) {
                    eprintln!("Error writing to file {:?}: {}", file_path, e);
                    break;
                }

                // Flush periodically (every write for now, could be optimized)
                if let Err(e) = writer.flush() {
                    eprintln!("Error flushing file {:?}: {}", file_path, e);
                    break;
                }
            }
            FileSaverCommand::ChangeFormat(new_format) => {
                if new_format != format {
                    format = new_format;
                    // Write a format change marker (for non-raw formats)
                    if format != SaveFormat::Raw {
                        let marker = format!("\n--- Format changed to {} ---\n", format);
                        if let Err(e) = writer.write_all(marker.as_bytes()) {
                            eprintln!("Error writing format marker to file {:?}: {}", file_path, e);
                        }
                    }
                }
            }
            FileSaverCommand::Stop => {
                break;
            }
        }
    }

    // Final flush before closing
    let _ = writer.flush();
}

/// Write a single data chunk to the file
fn write_chunk(
    writer: &mut BufWriter<File>,
    chunk: &SaveChunk,
    format: SaveFormat,
) -> std::io::Result<()> {
    match format {
        SaveFormat::Raw => {
            // Raw format: just write the bytes directly, no metadata
            writer.write_all(&chunk.data)
        }
        _ => {
            // Text formats: include timestamp and direction
            let timestamp = format_file_timestamp(chunk.timestamp);
            let direction = match chunk.direction {
                Direction::Tx => "TX",
                Direction::Rx => "RX",
            };

            // Encode the data
            let encoding = format.as_encoding().unwrap_or(Encoding::Utf8);
            let encoded = encode(&chunk.data, encoding);

            // Write formatted line: [timestamp] [direction] data
            writeln!(writer, "[{}] [{}] {}", timestamp, direction, encoded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_filename_generation() {
        let filename = generate_auto_filename("/dev/ttyUSB0", SaveFormat::Utf8);
        assert!(filename.starts_with("ttyUSB0-"));
        assert!(filename.ends_with(".txt"));
        // Check ISO 8601 format is present
        assert!(filename.contains("T"));
        assert!(filename.contains("Z"));
    }

    #[test]
    fn test_iso8601_timestamp() {
        use std::time::UNIX_EPOCH;
        // Test with a known timestamp
        let time = UNIX_EPOCH + std::time::Duration::from_millis(1703071852123);
        let formatted = format_iso8601_timestamp(time);
        // Should be: 2023-12-20T12:30:52.123Z (approximately)
        assert!(formatted.ends_with("Z"));
        assert!(formatted.contains("T"));
        assert_eq!(formatted.len(), 24); // YYYY-MM-DDTHH:MM:SS.mmmZ
    }
}
