//! File saving functionality for serial data
//!
//! Provides the ability to save serial traffic to files in various formats.
//! The file saver runs as a background task and receives data chunks to write.
//!
//! This module is internal to the buffer - `DataBuffer` exposes the public API.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tokio::sync::mpsc;

use super::chunk::{Direction, RawChunk};
use super::encoding::{encode, Encoding};

/// Commands sent to the file saver task
#[derive(Debug)]
pub(super) enum FileSaverCommand {
    /// Write a data chunk to the file
    Write(RawChunk),
    /// Stop saving and close the file
    Stop,
}

/// Internal handle for interacting with an active file saver
pub(super) struct FileSaverHandle {
    command_tx: mpsc::Sender<FileSaverCommand>,
    file_path: PathBuf,
    encoding: Encoding,
}

impl std::fmt::Debug for FileSaverHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSaverHandle")
            .field("file_path", &self.file_path)
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl FileSaverHandle {
    /// Write a raw chunk to the file
    pub fn write(&self, chunk: &RawChunk) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::Write(chunk.clone()))
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Get the file path being written to
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Get the encoding being used
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Stop saving and close the file
    pub fn stop(&self) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::Stop)
            .map_err(|_| crate::Error::ChannelSend)
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
    /// Encoding for saving data (determines file format)
    pub encoding: Encoding,
    /// Port name (used for auto-generated filenames)
    pub port_name: String,
}

impl FileSaveConfig {
    /// Create a new config with the given directory
    pub fn new(directory: impl Into<PathBuf>, port_name: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            filename: None,
            encoding: Encoding::Utf8,
            port_name: port_name.into(),
        }
    }

    /// Set a custom filename
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Set the encoding
    pub fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Generate the full file path
    pub fn generate_path(&self) -> PathBuf {
        let filename = self
            .filename
            .clone()
            .unwrap_or_else(|| generate_auto_filename(&self.port_name, self.encoding));
        self.directory.join(filename)
    }
}

/// Get file extension for an encoding
fn encoding_extension(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::Utf8 | Encoding::Ascii => "txt",
        Encoding::Hex(_) => "hex",
        Encoding::Binary(_) => "bin",
    }
}

/// Generate an auto filename with ISO 8601 timestamp
/// Format: {port_name}-{timestamp}.{extension}
/// Example: ttyUSB0-2025-12-20T14:30:52.123Z.txt
fn generate_auto_filename(port_name: &str, encoding: Encoding) -> String {
    // Clean up port name for use in filename
    let clean_port_name = port_name
        .replace(['/', '\\'], "_")
        .trim_start_matches("_dev_")
        .to_string();

    // Generate ISO 8601 timestamp
    let timestamp = format_iso8601_timestamp(SystemTime::now());

    format!(
        "{}-{}.{}",
        clean_port_name,
        timestamp,
        encoding_extension(encoding)
    )
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

/// Start a file saver task
///
/// Returns a handle for sending data to be saved.
///
/// This function must be called with a Tokio runtime handle to spawn the background task.
pub(super) fn start_file_saver(
    config: FileSaveConfig,
    runtime: &tokio::runtime::Handle,
) -> crate::Result<FileSaverHandle> {
    let file_path = config.generate_path();
    let encoding = config.encoding;

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
        file_saver_task(file, command_rx, encoding, path_clone).await;
    });

    Ok(FileSaverHandle {
        command_tx,
        file_path,
        encoding,
    })
}

/// The async task that handles file writing
async fn file_saver_task(
    file: File,
    mut command_rx: mpsc::Receiver<FileSaverCommand>,
    encoding: Encoding,
    file_path: PathBuf,
) {
    let mut writer = BufWriter::new(file);

    while let Some(cmd) = command_rx.recv().await {
        match cmd {
            FileSaverCommand::Write(chunk) => {
                if let Err(e) = write_chunk(&mut writer, &chunk, encoding) {
                    eprintln!("Error writing to file {:?}: {}", file_path, e);
                    break;
                }

                // Flush periodically (every write for now, could be optimized)
                if let Err(e) = writer.flush() {
                    eprintln!("Error flushing file {:?}: {}", file_path, e);
                    break;
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
fn write_chunk(writer: &mut BufWriter<File>, chunk: &RawChunk, encoding: Encoding) -> std::io::Result<()> {
    // For binary encoding with default format, write raw bytes without metadata
    if let Encoding::Binary(format) = encoding
        && format == super::encoding::BinaryFormat::default()
    {
        return writer.write_all(&chunk.data);
    }

    // Text formats: include timestamp and direction
    let timestamp = format_iso8601_timestamp(chunk.timestamp);
    let direction = match chunk.direction {
        Direction::Tx => "TX",
        Direction::Rx => "RX",
    };

    // Encode the data
    let encoded = encode(&chunk.data, encoding);

    // Write formatted line: [timestamp] [direction] data
    writeln!(writer, "[{}] [{}] {}", timestamp, direction, encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::encoding::HexFormat;

    #[test]
    fn auto_filename_generation() {
        let filename = generate_auto_filename("/dev/ttyUSB0", Encoding::Utf8);
        assert!(filename.starts_with("ttyUSB0-"));
        assert!(filename.ends_with(".txt"));
        // Check ISO 8601 format is present
        assert!(filename.contains("T"));
        assert!(filename.contains("Z"));
    }

    #[test]
    fn auto_filename_hex() {
        let filename = generate_auto_filename("COM3", Encoding::Hex(HexFormat::default()));
        assert!(filename.starts_with("COM3-"));
        assert!(filename.ends_with(".hex"));
    }

    #[test]
    fn iso8601_timestamp() {
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
