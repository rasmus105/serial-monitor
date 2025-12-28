//! File saving functionality for serial data
//!
//! Provides two file saving systems:
//!
//! 1. **Auto-save (crash recovery)**: Automatically saves all data to a cache directory
//!    (`~/.cache/serial-monitor/`) for crash recovery. Rotates files by session.
//!
//! 2. **User-initiated save**: Save to a user-specified path with configurable format,
//!    scope (existing buffer, new data, or both), and metadata options.
//!
//! # Architecture
//!
//! Both systems share the same core writing logic via [`WriteFormat`], which determines
//! how chunks are formatted when written to disk.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tokio::sync::mpsc;

use super::chunk::{Direction, RawChunk};
use super::encoding::{encode, Encoding};

// ============================================================================
// Save Format Configuration
// ============================================================================

/// Format specification for how data is written to files.
///
/// This is shared between auto-save and user-initiated saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveFormat {
    /// Raw bytes - no encoding, no metadata.
    /// Useful for replaying data to a device.
    Raw,

    /// Encoded text with configurable metadata.
    Encoded {
        /// How to encode the bytes (UTF-8, ASCII, Hex, Binary display)
        encoding: Encoding,
        /// Include timestamps for each chunk
        include_timestamps: bool,
        /// Include direction markers (TX/RX) for each chunk
        include_direction: bool,
    },
}

impl Default for SaveFormat {
    /// Default: ASCII encoding with timestamps, no direction markers
    fn default() -> Self {
        Self::Encoded {
            encoding: Encoding::Ascii,
            include_timestamps: true,
            include_direction: false,
        }
    }
}

impl SaveFormat {
    /// Create a raw bytes format (no encoding)
    pub fn raw() -> Self {
        Self::Raw
    }

    /// Create an encoded format with all metadata
    pub fn encoded(encoding: Encoding) -> Self {
        Self::Encoded {
            encoding,
            include_timestamps: true,
            include_direction: true,
        }
    }

    /// Create an encoded format with just timestamps (default for auto-save)
    pub fn encoded_with_timestamps(encoding: Encoding) -> Self {
        Self::Encoded {
            encoding,
            include_timestamps: true,
            include_direction: false,
        }
    }

    /// Get file extension appropriate for this format
    pub fn file_extension(&self) -> &'static str {
        match self {
            SaveFormat::Raw => "bin",
            SaveFormat::Encoded { encoding, .. } => match encoding {
                Encoding::Utf8 | Encoding::Ascii => "txt",
                Encoding::Hex(_) => "hex",
                Encoding::Binary(_) => "bin",
            },
        }
    }
}

// ============================================================================
// Save Scope (what data to include)
// ============================================================================

/// What data to include in a user-initiated save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveScope {
    /// Snapshot existing buffer only (one-off save)
    ExistingOnly,

    /// Stream new data going forward
    #[default]
    NewOnly,

    /// Snapshot existing buffer, then continue streaming new data
    ExistingAndContinue,
}

// ============================================================================
// Direction Filter (which directions to save)
// ============================================================================

/// Filter for which directions to include in saved data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectionFilter {
    /// Include RX (received) data
    pub rx: bool,
    /// Include TX (transmitted) data
    pub tx: bool,
}

impl Default for DirectionFilter {
    /// Default: RX only (matching your preference for auto-save)
    fn default() -> Self {
        Self {
            rx: true,
            tx: false,
        }
    }
}

impl DirectionFilter {
    /// Include all directions
    pub fn all() -> Self {
        Self { rx: true, tx: true }
    }

    /// Include only RX
    pub fn rx_only() -> Self {
        Self {
            rx: true,
            tx: false,
        }
    }

    /// Include only TX
    pub fn tx_only() -> Self {
        Self {
            rx: false,
            tx: true,
        }
    }

    /// Check if a direction passes this filter
    pub fn includes(&self, direction: Direction) -> bool {
        match direction {
            Direction::Rx => self.rx,
            Direction::Tx => self.tx,
        }
    }
}

// ============================================================================
// Auto-Save Configuration
// ============================================================================

/// Configuration for automatic crash-recovery saving.
///
/// Auto-save runs in the background and saves all session data to a cache directory.
/// Files are rotated by session (one file per connection).
#[derive(Debug, Clone)]
pub struct AutoSaveConfig {
    /// Directory for crash recovery files.
    /// Default: `~/.cache/serial-monitor/` or `/tmp/serial-monitor/` (if `~/.cache` not found)
    pub directory: PathBuf,

    /// Maximum number of session files to keep.
    /// Oldest files are deleted when this limit is exceeded.
    pub max_sessions: usize,

    /// Whether auto-save is enabled.
    pub enabled: bool,

    /// Format for auto-saved data.
    pub format: SaveFormat,

    /// Which directions to save.
    pub directions: DirectionFilter,
}

impl Default for AutoSaveConfig {
    fn default() -> Self {
        Self {
            directory: default_cache_directory(),
            max_sessions: 10,
            enabled: true,
            format: SaveFormat::default(), // ASCII with timestamps
            directions: DirectionFilter::default(), // RX only
        }
    }
}

impl AutoSaveConfig {
    /// Create config with auto-save disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Set the directory
    pub fn with_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = dir.into();
        self
    }

    /// Set max sessions to keep
    pub fn with_max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Set the save format
    pub fn with_format(mut self, format: SaveFormat) -> Self {
        self.format = format;
        self
    }

    /// Set direction filter
    pub fn with_directions(mut self, directions: DirectionFilter) -> Self {
        self.directions = directions;
        self
    }

    /// Enable or disable
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Get the default cache directory for auto-save.
/// Tries `~/.cache/serial-monitor/`, falls back to `/tmp/serial-monitor/`.
pub fn default_cache_directory() -> PathBuf {
    dirs::cache_dir()
        .map(|p| p.join("serial-monitor"))
        .unwrap_or_else(|| PathBuf::from("/tmp/serial-monitor"))
}

// ============================================================================
// User Save Configuration
// ============================================================================

/// Configuration for user-initiated file saving.
#[derive(Debug, Clone)]
pub struct UserSaveConfig {
    /// Full path to save to (including filename)
    pub path: PathBuf,

    /// What data to include
    pub scope: SaveScope,

    /// Format for saved data
    pub format: SaveFormat,

    /// Which directions to save
    pub directions: DirectionFilter,
}

impl UserSaveConfig {
    /// Create a new user save config
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            scope: SaveScope::default(),
            format: SaveFormat::default(),
            directions: DirectionFilter::all(),
        }
    }

    /// Set the scope
    pub fn with_scope(mut self, scope: SaveScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set the format
    pub fn with_format(mut self, format: SaveFormat) -> Self {
        self.format = format;
        self
    }

    /// Set direction filter
    pub fn with_directions(mut self, directions: DirectionFilter) -> Self {
        self.directions = directions;
        self
    }
}

// ============================================================================
// File Saver Handle (internal)
// ============================================================================

/// Commands sent to the file saver task
#[derive(Debug)]
pub(crate) enum FileSaverCommand {
    /// Write a data chunk to the file
    Write(RawChunk),
    /// Stop saving and close the file
    Stop,
}

/// Internal handle for interacting with an active file saver.
///
/// Used for streaming saves (NewOnly or ExistingAndContinue scopes).
pub(crate) struct FileSaverHandle {
    command_tx: mpsc::Sender<FileSaverCommand>,
    file_path: PathBuf,
    format: SaveFormat,
    directions: DirectionFilter,
}

impl std::fmt::Debug for FileSaverHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSaverHandle")
            .field("file_path", &self.file_path)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl FileSaverHandle {
    /// Write a raw chunk to the file (if it passes the direction filter)
    pub fn write(&self, chunk: &RawChunk) -> Result<(), crate::Error> {
        if !self.directions.includes(chunk.direction) {
            return Ok(()); // Silently skip filtered directions
        }
        self.command_tx
            .try_send(FileSaverCommand::Write(chunk.clone()))
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Get the file path being written to
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Get the format being used
    pub fn format(&self) -> &SaveFormat {
        &self.format
    }

    /// Stop saving and close the file
    pub fn stop(&self) -> Result<(), crate::Error> {
        self.command_tx
            .try_send(FileSaverCommand::Stop)
            .map_err(|_| crate::Error::ChannelSend)
    }

    /// Clone the command sender for use by the I/O task
    pub(crate) fn clone_sender(&self) -> AutoSaveSender {
        AutoSaveSender {
            tx: self.command_tx.clone(),
            directions: self.directions,
        }
    }
}

/// Lightweight sender for auto-save, used by the I/O task.
///
/// This is separate from FileSaverHandle to avoid the I/O task needing
/// the full handle (which has Drop behavior that would stop saving).
#[derive(Clone)]
pub(crate) struct AutoSaveSender {
    tx: mpsc::Sender<FileSaverCommand>,
    directions: DirectionFilter,
}

impl AutoSaveSender {
    /// Write a chunk to auto-save (if it passes direction filter)
    pub fn write(&self, chunk: &RawChunk) {
        if self.directions.includes(chunk.direction) {
            let _ = self.tx.try_send(FileSaverCommand::Write(chunk.clone()));
        }
    }

    /// Write new data to auto-save, automatically timestamping it.
    ///
    /// This is a convenience method for writing freshly received/sent data
    /// without manually constructing a `RawChunk`.
    pub fn write_new(&self, data: Vec<u8>, direction: Direction) {
        self.write(&RawChunk {
            data,
            direction,
            timestamp: SystemTime::now(),
        });
    }
}

impl Drop for FileSaverHandle {
    fn drop(&mut self) {
        let _ = self.command_tx.try_send(FileSaverCommand::Stop);
    }
}

// ============================================================================
// Core Writing Logic
// ============================================================================

/// Write a single chunk to a writer using the specified format.
fn write_chunk<W: Write>(
    writer: &mut W,
    chunk: &RawChunk,
    format: &SaveFormat,
) -> std::io::Result<()> {
    match format {
        SaveFormat::Raw => writer.write_all(&chunk.data),
        SaveFormat::Encoded {
            encoding,
            include_timestamps,
            include_direction,
        } => {
            let encoded = encode(&chunk.data, *encoding);

            // Build the line with optional metadata
            let mut line = String::new();

            if *include_timestamps {
                line.push('[');
                line.push_str(&format_iso8601_timestamp(chunk.timestamp));
                line.push_str("] ");
            }

            if *include_direction {
                line.push('[');
                line.push_str(match chunk.direction {
                    Direction::Tx => "TX",
                    Direction::Rx => "RX",
                });
                line.push_str("] ");
            }

            line.push_str(&encoded);
            writeln!(writer, "{}", line)
        }
    }
}

/// Write multiple chunks to a writer.
fn write_chunks<W: Write>(
    writer: &mut W,
    chunks: &VecDeque<RawChunk>,
    format: &SaveFormat,
    directions: &DirectionFilter,
) -> std::io::Result<()> {
    for chunk in chunks {
        if directions.includes(chunk.direction) {
            write_chunk(writer, chunk, format)?;
        }
    }
    Ok(())
}

// ============================================================================
// One-Off Save (ExistingOnly scope)
// ============================================================================

/// Save existing buffer data to a file (one-off, synchronous).
///
/// This is used for `SaveScope::ExistingOnly`.
pub(super) fn save_existing_to_file(
    chunks: &VecDeque<RawChunk>,
    config: &UserSaveConfig,
) -> crate::Result<()> {
    // Create directory if needed
    if let Some(parent) = config.path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&config.path)?;

    let mut writer = BufWriter::new(file);
    write_chunks(&mut writer, chunks, &config.format, &config.directions)?;
    writer.flush()?;

    Ok(())
}

// ============================================================================
// Streaming Save (NewOnly or ExistingAndContinue scopes)
// ============================================================================

/// Start a streaming file saver task.
///
/// If `existing_chunks` is provided (for ExistingAndContinue), they are written first.
pub(super) fn start_streaming_saver(
    config: &UserSaveConfig,
    existing_chunks: Option<&VecDeque<RawChunk>>,
    runtime: &tokio::runtime::Handle,
) -> crate::Result<FileSaverHandle> {
    let file_path = config.path.clone();
    let format = config.format.clone();
    let directions = config.directions;

    // Create directory if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open file
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)?;

    let mut writer = BufWriter::new(file);

    // Write existing chunks if provided
    if let Some(chunks) = existing_chunks {
        write_chunks(&mut writer, chunks, &format, &directions)?;
        writer.flush()?;
    }

    let (command_tx, command_rx) = mpsc::channel(256);

    // Spawn the streaming task
    let format_clone = format.clone();
    let path_clone = file_path.clone();
    runtime.spawn(async move {
        streaming_saver_task(writer, command_rx, format_clone, path_clone).await;
    });

    Ok(FileSaverHandle {
        command_tx,
        file_path,
        format,
        directions,
    })
}

/// Async task that handles streaming writes.
/// Flushes every 1 second to ensure the max file saving delay is 1 second (instead
/// of just using the internal flushing logic of `BufWriter`, which only flushes
/// after x bytes)
async fn streaming_saver_task(
    mut writer: BufWriter<File>,
    mut command_rx: mpsc::Receiver<FileSaverCommand>,
    format: SaveFormat,
    file_path: PathBuf,
) {
    use tokio::time::{interval, Duration};

    let mut flush_interval = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            biased;

            cmd = command_rx.recv() => {
                match cmd {
                    Some(FileSaverCommand::Write(chunk)) => {
                        if let Err(e) = write_chunk(&mut writer, &chunk, &format) {
                            eprintln!("Error writing to file {:?}: {}", file_path, e);
                            break;
                        }
                    }
                    Some(FileSaverCommand::Stop) | None => {
                        break;
                    }
                }
            }

            _ = flush_interval.tick() => {
                if let Err(e) = writer.flush() {
                    eprintln!("Error flushing file {:?}: {}", file_path, e);
                    break;
                }
            }
        }
    }

    let _ = writer.flush();
}

// ============================================================================
// Auto-Save System
// ============================================================================

/// Start auto-save for a session.
///
/// Creates a new session file and returns a handle for streaming data.
pub(crate) fn start_auto_save(
    config: &AutoSaveConfig,
    port_name: &str,
    runtime: &tokio::runtime::Handle,
) -> crate::Result<FileSaverHandle> {
    if !config.enabled {
        return Err(crate::Error::InvalidConfig("Auto-save is disabled".into()));
    }

    // Create directory if needed
    fs::create_dir_all(&config.directory)?;

    // Rotate old session files
    rotate_session_files(&config.directory, config.max_sessions)?;

    // Generate filename for this session
    let filename = generate_session_filename(port_name, &config.format);
    let file_path = config.directory.join(filename);

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)?;

    let writer = BufWriter::new(file);
    let (command_tx, command_rx) = mpsc::channel(256);

    let format = config.format.clone();
    let directions = config.directions;
    let path_clone = file_path.clone();
    let format_clone = format.clone();

    runtime.spawn(async move {
        streaming_saver_task(writer, command_rx, format_clone, path_clone).await;
    });

    Ok(FileSaverHandle {
        command_tx,
        file_path,
        format,
        directions,
    })
}

/// Rotate session files, keeping only the most recent `max_sessions` (otherwise
/// we could end up cluttering up the user's system with these files)
fn rotate_session_files(directory: &Path, max_sessions: usize) -> std::io::Result<()> {
    if max_sessions == 0 {
        return Ok(());
    }

    let mut files: Vec<_> = fs::read_dir(directory)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let metadata = e.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            Some((e.path(), modified))
        })
        .collect();

    // Sort by modification time (oldest first)
    files.sort_by_key(|(_, time)| *time);

    // Delete oldest files if we have too many
    // We want to keep max_sessions - 1 (to make room for the new one)
    let to_delete = files.len().saturating_sub(max_sessions.saturating_sub(1));
    for (path, _) in files.into_iter().take(to_delete) {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

/// Generate a filename for a session file.
fn generate_session_filename(port_name: &str, format: &SaveFormat) -> String {
    let clean_port_name = port_name
        .replace(['/', '\\'], "_")
        .trim_start_matches("_dev_")
        .to_string();

    let timestamp = format_iso8601_timestamp(SystemTime::now());
    let extension = format.file_extension();

    format!("{}-{}.{}", clean_port_name, timestamp, extension)
}

// ============================================================================
// Timestamp Formatting
// ============================================================================

/// Format a SystemTime as ISO 8601 timestamp.
/// Format: YYYY-MM-DDTHH:mm:ss.sssZ
pub fn format_iso8601_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Convert days since Unix epoch to year, month, day (Howard Hinnant's algorithm).
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cache_dir_exists_or_fallback() {
        let dir = default_cache_directory();
        // Should be either ~/.cache/serial-monitor or /tmp/serial-monitor
        assert!(dir.ends_with("serial-monitor"));
    }

    #[test]
    fn save_format_extensions() {
        assert_eq!(SaveFormat::Raw.file_extension(), "bin");
        assert_eq!(SaveFormat::encoded(Encoding::Utf8).file_extension(), "txt");
        assert_eq!(SaveFormat::encoded(Encoding::Ascii).file_extension(), "txt");
        assert_eq!(
            SaveFormat::encoded(Encoding::Hex(Default::default())).file_extension(),
            "hex"
        );
    }

    #[test]
    fn direction_filter() {
        let rx_only = DirectionFilter::rx_only();
        assert!(rx_only.includes(Direction::Rx));
        assert!(!rx_only.includes(Direction::Tx));

        let all = DirectionFilter::all();
        assert!(all.includes(Direction::Rx));
        assert!(all.includes(Direction::Tx));
    }

    #[test]
    fn session_filename_generation() {
        let filename = generate_session_filename("/dev/ttyUSB0", &SaveFormat::default());
        assert!(filename.starts_with("ttyUSB0-"));
        assert!(filename.ends_with(".txt"));
        assert!(filename.contains("T")); // ISO 8601 format
    }

    #[test]
    fn iso8601_timestamp_format() {
        use std::time::UNIX_EPOCH;
        let time = UNIX_EPOCH + std::time::Duration::from_millis(1703071852123);
        let formatted = format_iso8601_timestamp(time);
        assert!(formatted.ends_with("Z"));
        assert!(formatted.contains("T"));
        assert_eq!(formatted.len(), 24);
    }

    #[test]
    fn write_chunk_raw() {
        let chunk = RawChunk {
            data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f], // "Hello"
            direction: Direction::Rx,
            timestamp: SystemTime::now(),
        };

        let mut buffer = Vec::new();
        write_chunk(&mut buffer, &chunk, &SaveFormat::Raw).unwrap();
        assert_eq!(buffer, vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    }

    #[test]
    fn write_chunk_encoded_no_metadata() {
        let chunk = RawChunk {
            data: b"Hello".to_vec(),
            direction: Direction::Rx,
            timestamp: SystemTime::now(),
        };

        let format = SaveFormat::Encoded {
            encoding: Encoding::Utf8,
            include_timestamps: false,
            include_direction: false,
        };

        let mut buffer = Vec::new();
        write_chunk(&mut buffer, &chunk, &format).unwrap();
        assert_eq!(String::from_utf8(buffer).unwrap(), "Hello\n");
    }

    #[test]
    fn write_chunk_encoded_with_direction() {
        let chunk = RawChunk {
            data: b"test".to_vec(),
            direction: Direction::Tx,
            timestamp: SystemTime::now(),
        };

        let format = SaveFormat::Encoded {
            encoding: Encoding::Utf8,
            include_timestamps: false,
            include_direction: true,
        };

        let mut buffer = Vec::new();
        write_chunk(&mut buffer, &chunk, &format).unwrap();
        assert_eq!(String::from_utf8(buffer).unwrap(), "[TX] test\n");
    }
}
