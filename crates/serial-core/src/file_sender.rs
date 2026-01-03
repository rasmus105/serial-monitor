//! File sending functionality with chunking and progress reporting
//!
//! Supports sending files with configurable chunk sizes and delays between chunks.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use memchr::memmem;
use strum::{IntoStaticStr, VariantArray};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc;

use crate::error::Result;
use crate::session::SessionHandle;

/// Channel capacity for progress updates. Sized to allow brief bursts
/// without blocking the sender, while not consuming excessive memory.
const PROGRESS_CHANNEL_CAPACITY: usize = 32;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum Delimiter {
    /// Line feed (`\n`)
    #[default]
    #[strum(serialize = "LF (\\n)")]
    Lf,
    /// Carriage return + line feed (`\r\n`)
    #[strum(serialize = "CRLF (\\r\\n)")]
    CrLf,
    /// Carriage return (`\r`)
    #[strum(serialize = "CR (\\r)")]
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

    /// Create from index into VARIANTS
    pub fn from_index(index: usize) -> Self {
        Self::VARIANTS.get(index).copied().unwrap_or_default()
    }
}

/// Configuration for file sending
#[derive(Debug, Clone, bon::Builder)]
pub struct FileSendConfig {
    /// How to divide the file into chunks
    #[builder(default)]
    pub chunk_mode: ChunkMode,
    /// Whether to include the delimiter in sent chunks (only for delimiter mode)
    #[builder(default = true)]
    pub include_delimiter: bool,
    /// Number of delimiter-separated units to send per chunk (only for delimiter mode)
    /// e.g., if set to 2 and delimiter is '\n', sends 2 lines at a time
    #[builder(default = 1)]
    pub lines_per_chunk: usize,
    /// Optional suffix to append to each chunk (e.g., line ending)
    pub chunk_suffix: Option<Cow<'static, [u8]>>,
    /// Delay between chunks
    #[builder(default = Duration::from_millis(10))]
    pub chunk_delay: Duration,
    /// Whether to loop the file continuously
    #[builder(default)]
    pub repeat: bool,
}

impl Default for FileSendConfig {
    fn default() -> Self {
        Self::builder().build()
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

    // Get file size without opening the file
    let metadata = tokio::fs::metadata(&path).await?;
    let total_bytes = metadata.len();

    // For delimiter mode, we don't know exact chunk count upfront
    // For byte mode, we can calculate it
    let total_chunks = match &config.chunk_mode {
        ChunkMode::Bytes(size) => (total_bytes as usize).div_ceil(*size),
        ChunkMode::Delimiter(_) => 0, // Unknown until we stream the file
    };

    // Create channels
    let (progress_tx, progress_rx) = mpsc::channel(PROGRESS_CHANNEL_CAPACITY);
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

/// Default buffer size for streaming delimiter search (64 KB)
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Send file using delimiter-based streaming chunks
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
    let file = match File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            progress.error = Some(e.to_string());
            progress.complete = true;
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }
    };

    let mut reader = BufReader::with_capacity(STREAM_BUFFER_SIZE, file);
    let delimiter_bytes = delimiter.as_bytes();
    let finder = memmem::Finder::new(delimiter_bytes);
    let lines_per_chunk = config.lines_per_chunk.max(1);

    // Buffer to accumulate data until we find delimiters
    let mut pending = Vec::new();
    let mut read_buf = [0u8; STREAM_BUFFER_SIZE];

    // Buffer to accumulate multiple delimiter-separated units before sending
    let mut chunk_buffer = Vec::new();
    let mut units_in_buffer = 0;

    loop {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            progress.complete = true;
            progress.error = Some("Cancelled".to_string());
            let _ = progress_tx.send(progress.clone()).await;
            return false;
        }

        // Read more data
        let n = match reader.read(&mut read_buf).await {
            Ok(0) => {
                // EOF - send any remaining data as final chunk
                // First, add any pending data to chunk_buffer
                if !pending.is_empty() {
                    chunk_buffer.extend_from_slice(&pending);
                }
                if !chunk_buffer.is_empty() {
                    let bytes_in_chunk = chunk_buffer.len();
                    let chunk = build_chunk(&chunk_buffer, config);
                    if !send_chunk(session, chunk, progress, progress_tx, bytes_in_chunk).await {
                        return false;
                    }
                }
                break;
            }
            Ok(n) => n,
            Err(e) => {
                progress.complete = true;
                progress.error = Some(e.to_string());
                let _ = progress_tx.send(progress.clone()).await;
                return false;
            }
        };

        pending.extend_from_slice(&read_buf[..n]);

        // Process all complete delimiter-separated units in pending buffer
        loop {
            let Some(pos) = finder.find(&pending) else {
                // No delimiter found, need more data
                break;
            };

            // Found a delimiter at `pos`
            let unit_end = if config.include_delimiter {
                pos + delimiter_bytes.len()
            } else {
                pos
            };

            // Add this unit to chunk_buffer
            if unit_end > 0 {
                chunk_buffer.extend_from_slice(&pending[..unit_end]);
            }
            units_in_buffer += 1;

            // Remove processed data (including delimiter) from pending
            let drain_end = pos + delimiter_bytes.len();
            pending.drain(..drain_end);

            // If we've accumulated enough units, send the chunk
            if units_in_buffer >= lines_per_chunk {
                let bytes_in_chunk = chunk_buffer.len();
                let chunk = build_chunk(&chunk_buffer, config);

                if !send_chunk(session, chunk, progress, progress_tx, bytes_in_chunk).await {
                    return false;
                }

                // Delay between chunks
                if config.chunk_delay > Duration::ZERO {
                    tokio::time::sleep(config.chunk_delay).await;
                }

                // Reset for next chunk
                chunk_buffer.clear();
                units_in_buffer = 0;
            }
        }
    }

    true
}

/// Build a chunk with optional suffix
fn build_chunk<'a>(data: &'a [u8], config: &FileSendConfig) -> Cow<'a, [u8]> {
    match &config.chunk_suffix {
        Some(suffix) => {
            let mut chunk = data.to_vec();
            chunk.extend_from_slice(suffix);
            Cow::Owned(chunk)
        }
        None => Cow::Borrowed(data),
    }
}

/// Send a single chunk and update progress
/// Returns false if session closed
async fn send_chunk(
    session: &mpsc::Sender<crate::session::SessionCommand>,
    chunk: Cow<'_, [u8]>,
    progress: &mut FileSendProgress,
    progress_tx: &mpsc::Sender<FileSendProgress>,
    bytes_count: usize,
) -> bool {
    if session
        .send(crate::session::SessionCommand::Send(chunk.into_owned()))
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


