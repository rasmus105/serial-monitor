//! File sending functionality with chunking and progress reporting
//!
//! Supports sending files with configurable chunk sizes and delays between chunks.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::error::Result;
use crate::session::SessionHandle;

/// How to divide the file into chunks
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkMode {
    /// Fixed number of bytes per chunk
    Bytes(usize),
    /// Split on a delimiter (e.g., newline)
    Delimiter(Delimiter),
}

impl Default for ChunkMode {
    fn default() -> Self {
        ChunkMode::Delimiter(Delimiter::default())
    }
}

/// Predefined delimiters for chunking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Delimiter {
    /// Line feed (`\n`)
    #[default]
    Lf,
    /// Carriage return + line feed (`\r\n`)
    CrLf,
    /// Carriage return (`\r`)
    Cr,
}

impl Delimiter {
    /// Get the byte sequence for this delimiter
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            Delimiter::Lf => b"\n",
            Delimiter::CrLf => b"\r\n",
            Delimiter::Cr => b"\r",
        }
    }

    /// Display name for this delimiter
    pub fn display_name(&self) -> &'static str {
        match self {
            Delimiter::Lf => "LF (\\n)",
            Delimiter::CrLf => "CRLF (\\r\\n)",
            Delimiter::Cr => "CR (\\r)",
        }
    }

    /// All delimiter variants
    pub const ALL: &'static [Delimiter] = &[Delimiter::Lf, Delimiter::CrLf, Delimiter::Cr];

    /// Option strings for UI dropdown
    pub const OPTIONS: &'static [&'static str] = &["LF (\\n)", "CRLF (\\r\\n)", "CR (\\r)"];

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Get index of this delimiter
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|d| d == self).unwrap_or(0)
    }
}

/// Time unit for delay configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeUnit {
    #[default]
    Milliseconds,
    Seconds,
    Minutes,
    Hours,
}

impl TimeUnit {
    /// Convert a value in this unit to Duration
    pub fn to_duration(&self, value: u64) -> Duration {
        match self {
            TimeUnit::Milliseconds => Duration::from_millis(value),
            TimeUnit::Seconds => Duration::from_secs(value),
            TimeUnit::Minutes => Duration::from_secs(value * 60),
            TimeUnit::Hours => Duration::from_secs(value * 3600),
        }
    }

    /// Display name
    pub fn display_name(&self) -> &'static str {
        match self {
            TimeUnit::Milliseconds => "ms",
            TimeUnit::Seconds => "s",
            TimeUnit::Minutes => "min",
            TimeUnit::Hours => "h",
        }
    }

    /// All variants
    pub const ALL: &'static [TimeUnit] = &[
        TimeUnit::Milliseconds,
        TimeUnit::Seconds,
        TimeUnit::Minutes,
        TimeUnit::Hours,
    ];

    /// Option strings for UI dropdown
    pub const OPTIONS: &'static [&'static str] = &["ms", "s", "min", "h"];

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Get index
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|u| u == self).unwrap_or(0)
    }
}

/// Size unit for byte-based chunking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeUnit {
    #[default]
    Bytes,
    Kilobytes,
    Megabytes,
}

impl SizeUnit {
    /// Convert a value in this unit to bytes
    pub fn to_bytes(&self, value: usize) -> usize {
        match self {
            SizeUnit::Bytes => value,
            SizeUnit::Kilobytes => value * 1024,
            SizeUnit::Megabytes => value * 1024 * 1024,
        }
    }

    /// Display name
    pub fn display_name(&self) -> &'static str {
        match self {
            SizeUnit::Bytes => "B",
            SizeUnit::Kilobytes => "KB",
            SizeUnit::Megabytes => "MB",
        }
    }

    /// All variants
    pub const ALL: &'static [SizeUnit] = &[SizeUnit::Bytes, SizeUnit::Kilobytes, SizeUnit::Megabytes];

    /// Option strings for UI dropdown
    pub const OPTIONS: &'static [&'static str] = &["B", "KB", "MB"];

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Get index
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|u| u == self).unwrap_or(0)
    }
}

/// Configuration for file sending
#[derive(Debug, Clone)]
pub struct FileSendConfig {
    /// How to divide the file into chunks
    pub chunk_mode: ChunkMode,
    /// Whether to include the delimiter in sent chunks (only for delimiter mode)
    pub include_delimiter: bool,
    /// Number of delimiter-separated units to send per chunk (only for delimiter mode)
    /// e.g., if set to 2 and delimiter is '\n', sends 2 lines at a time
    pub lines_per_chunk: usize,
    /// Optional suffix to append to each chunk (e.g., line ending)
    pub chunk_suffix: Option<Cow<'static, [u8]>>,
    /// Delay between chunks
    pub chunk_delay: Duration,
    /// Whether to loop the file continuously
    pub repeat: bool,
}

impl Default for FileSendConfig {
    fn default() -> Self {
        Self {
            chunk_mode: ChunkMode::default(),
            include_delimiter: true,
            lines_per_chunk: 1,
            chunk_suffix: None,
            chunk_delay: Duration::from_millis(10),
            repeat: false,
        }
    }
}

/// Progress update during file sending
#[derive(Default, Debug, Clone)]
pub struct FileSendProgress {
    /// Total bytes in the file
    pub total_bytes: u64,
    /// Bytes sent so far
    pub bytes_sent: u64,
    /// Number of chunks sent
    pub chunks_sent: usize,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Whether sending is complete
    pub complete: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Number of loops completed (for repeat mode)
    pub loops_completed: usize,
}

impl FileSendProgress {
    /// Get progress as a percentage (0.0 to 1.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            1.0
        } else {
            self.bytes_sent as f64 / self.total_bytes as f64
        }
    }
}

/// Handle for controlling an ongoing file send operation
pub struct FileSendHandle {
    /// Channel to receive progress updates
    progress_rx: mpsc::Receiver<FileSendProgress>,
    /// Channel to send cancel signal
    cancel_tx: mpsc::Sender<()>,
}

impl FileSendHandle {
    /// Try to receive a progress update (non-blocking)
    pub fn try_recv_progress(&mut self) -> Option<FileSendProgress> {
        self.progress_rx.try_recv().ok()
    }

    /// Receive a progress update (async, blocking)
    pub async fn recv_progress(&mut self) -> Option<FileSendProgress> {
        self.progress_rx.recv().await
    }

    /// Cancel the file send operation
    pub async fn cancel(&self) {
        let _ = self.cancel_tx.send(()).await;
    }
}

/// Start sending a file asynchronously
///
/// Returns a handle for monitoring progress and cancelling the operation.
pub async fn send_file(
    session: &SessionHandle,
    path: impl AsRef<Path>,
    config: FileSendConfig,
) -> Result<FileSendHandle> {
    let path = path.as_ref().to_path_buf();

    // Open file and get size
    let file = File::open(&path).await?;
    let metadata = file.metadata().await?;
    let total_bytes = metadata.len();

    // For delimiter mode, we don't know exact chunk count upfront
    // For byte mode, we can calculate it
    let total_chunks = match &config.chunk_mode {
        ChunkMode::Bytes(size) => (total_bytes as usize).div_ceil(*size),
        ChunkMode::Delimiter(_) => 0, // Unknown until we scan the file
    };

    // Create channels
    let (progress_tx, progress_rx) = mpsc::channel(32);
    let (cancel_tx, cancel_rx) = mpsc::channel(1);

    // Clone what we need for the task
    let session_clone = session.clone_sender();

    // Spawn the sending task
    tokio::spawn(async move {
        send_file_task(
            session_clone,
            path,
            config,
            total_bytes,
            total_chunks,
            progress_tx,
            cancel_rx,
        )
        .await;
    });

    Ok(FileSendHandle {
        progress_rx,
        cancel_tx,
    })
}

/// Internal task that performs the actual file sending
async fn send_file_task(
    session: mpsc::Sender<crate::session::SessionCommand>,
    path: std::path::PathBuf,
    config: FileSendConfig,
    total_bytes: u64,
    total_chunks: usize,
    progress_tx: mpsc::Sender<FileSendProgress>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    let mut progress = FileSendProgress {
        total_bytes,
        total_chunks,
        ..Default::default()
    };

    loop {
        // ensure reset
        progress.bytes_sent = 0;
        progress.chunks_sent = 0;

        match &config.chunk_mode {
            ChunkMode::Bytes(chunk_size) => {
                if !send_bytes_chunked(
                    &session,
                    &path,
                    *chunk_size,
                    &config,
                    &mut progress,
                    &progress_tx,
                    &mut cancel_rx,
                )
                .await
                {
                    return;
                }
            }
            ChunkMode::Delimiter(delimiter) => {
                if !send_delimiter_chunked(
                    &session,
                    &path,
                    delimiter,
                    &config,
                    &mut progress,
                    &progress_tx,
                    &mut cancel_rx,
                )
                .await
                {
                    return;
                }
            }
        }

        progress.loops_completed += 1;

        // Send completion or loop progress
        if !config.repeat {
            progress.complete = true;
            let _ = progress_tx.send(progress).await;
            return;
        }

        // In repeat mode, send progress and continue
        let _ = progress_tx.send(progress.clone()).await;
    }
}

/// Send file using fixed byte chunks
/// Returns false if should stop (error or cancel), true to continue
async fn send_bytes_chunked(
    session: &mpsc::Sender<crate::session::SessionCommand>,
    path: &std::path::Path,
    chunk_size: usize,
    config: &FileSendConfig,
    progress: &mut FileSendProgress,
    progress_tx: &mpsc::Sender<FileSendProgress>,
    cancel_rx: &mut mpsc::Receiver<()>,
) -> bool {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            progress.error = Some(e.to_string());
            progress.complete = true;
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }
    };

    let mut buffer = vec![0u8; chunk_size];

    loop {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            progress.complete = true;
            progress.error = Some("Cancelled".to_string());
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }

        // Read a chunk
        let n = match file.read(&mut buffer).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                progress.complete = true;
                progress.error = Some(e.to_string());
                let _ = progress_tx.send(progress.clone()).await;
                return false;
            }
        };

        // Build chunk with optional suffix
        let chunk = build_chunk(&buffer[..n], config);

        if !send_chunk(session, chunk, progress, progress_tx, n).await {
            return false;
        }

        // Delay between chunks
        if config.chunk_delay > Duration::ZERO {
            tokio::time::sleep(config.chunk_delay).await;
        }
    }

    true
}

/// Send file using delimiter-based chunks
/// Returns false if should stop (error or cancel), true to continue
async fn send_delimiter_chunked(
    session: &mpsc::Sender<crate::session::SessionCommand>,
    path: &std::path::Path,
    delimiter: &Delimiter,
    config: &FileSendConfig,
    progress: &mut FileSendProgress,
    progress_tx: &mpsc::Sender<FileSendProgress>,
    cancel_rx: &mut mpsc::Receiver<()>,
) -> bool {
    // Read entire file into memory for delimiter splitting
    // (For very large files, a streaming approach would be better, but this is simpler)
    let content = match tokio::fs::read(path).await {
        Ok(c) => c,
        Err(e) => {
            progress.error = Some(e.to_string());
            progress.complete = true;
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }
    };

    let delimiter_bytes = delimiter.as_bytes();
    let individual_chunks = split_by_delimiter(&content, delimiter_bytes, config.include_delimiter);

    // Group chunks according to lines_per_chunk
    let lines_per_chunk = config.lines_per_chunk.max(1);
    let grouped_chunks: Vec<Vec<u8>> = individual_chunks
        .chunks(lines_per_chunk)
        .map(|group| group.concat())
        .collect();

    // Now we know the total number of chunks
    progress.total_chunks = grouped_chunks.len();

    for chunk_data in grouped_chunks {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            progress.complete = true;
            progress.error = Some("Cancelled".to_string());
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }

        let bytes_in_chunk = chunk_data.len();

        // Build chunk with optional suffix
        let chunk = build_chunk(&chunk_data, config);

        if !send_chunk(session, chunk, progress, progress_tx, bytes_in_chunk).await {
            return false;
        }

        // Delay between chunks
        if config.chunk_delay > Duration::ZERO {
            tokio::time::sleep(config.chunk_delay).await;
        }
    }

    true
}

/// Build a chunk with optional suffix
fn build_chunk(data: &[u8], config: &FileSendConfig) -> Vec<u8> {
    match &config.chunk_suffix {
        Some(suffix) => {
            let mut chunk = data.to_vec();
            chunk.extend_from_slice(suffix);
            chunk
        }
        None => data.to_vec(),
    }
}

/// Send a single chunk and update progress
/// Returns false if session closed
async fn send_chunk(
    session: &mpsc::Sender<crate::session::SessionCommand>,
    chunk: Vec<u8>,
    progress: &mut FileSendProgress,
    progress_tx: &mpsc::Sender<FileSendProgress>,
    bytes_count: usize,
) -> bool {
    if session
        .send(crate::session::SessionCommand::Send(chunk))
        .await
        .is_err()
    {
        progress.complete = true;
        progress.error = Some("Session closed".to_string());
        let _ = progress_tx.send(progress.clone()).await;
        return false;
    }

    progress.bytes_sent += bytes_count as u64;
    progress.chunks_sent += 1;

    // Send progress update
    let _ = progress_tx.send(progress.clone()).await;

    true
}

/// Split data by delimiter
fn split_by_delimiter(data: &[u8], delimiter: &[u8], include_delimiter: bool) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < data.len() {
        // Find next delimiter
        let end = find_subsequence(&data[start..], delimiter)
            .map(|pos| start + pos)
            .unwrap_or(data.len());

        if end > start || (end == start && end < data.len()) {
            let chunk_end = if include_delimiter && end < data.len() {
                // Include the delimiter
                (end + delimiter.len()).min(data.len())
            } else {
                end
            };

            if chunk_end > start {
                chunks.push(data[start..chunk_end].to_vec());
            }
        }

        // Move past the delimiter (or to end if not found)
        if end < data.len() {
            start = end + delimiter.len();
        } else {
            break;
        }
    }

    // Handle trailing data after last delimiter (if not ending with delimiter)
    // This is already handled by the unwrap_or(data.len()) above

    chunks
}

/// Find subsequence in data
fn find_subsequence(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len()).position(|w| w == pattern)
}
