//! File sending with chunking and progress reporting.
//!
//! Supports sending files with configurable chunk sizes and delays between chunks.
//!
//! Notes:
//! - Could optimize by not updating progress each time a chunk is sent (though would only really
//!   matter for high-frequency sending)
//! - An object oriented style could make the code slightly cleaner, however, the API already feels
//!   quite nice.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use memchr::memmem;
use strum::{IntoStaticStr, VariantArray};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, watch};

use crate::error::Result;
use crate::session::{SessionCommand, SessionHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkMode {
    Bytes(usize),
    Delimiter(Delimiter),
}

impl Default for ChunkMode {
    fn default() -> Self {
        ChunkMode::Delimiter(Delimiter::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr)]
pub enum Delimiter {
    #[default]
    #[strum(serialize = "LF (\\n)")]
    Lf,
    #[strum(serialize = "CRLF (\\r\\n)")]
    CrLf,
    #[strum(serialize = "CR (\\r)")]
    Cr,
}

impl Delimiter {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            Delimiter::Lf => b"\n",
            Delimiter::CrLf => b"\r\n",
            Delimiter::Cr => b"\r",
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::VARIANTS.get(index).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, bon::Builder)]
pub struct FileSendConfig {
    #[builder(default)]
    pub chunk_mode: ChunkMode,
    #[builder(default = true)]
    pub include_delimiter: bool,
    #[builder(default = 1)]
    pub units_per_chunk: usize,
    pub chunk_suffix: Option<Cow<'static, [u8]>>,
    #[builder(default = Duration::from_millis(10))]
    pub chunk_delay: Duration,
    #[builder(default)]
    pub repeat: bool,
}

impl Default for FileSendConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

#[derive(Default, Debug, Clone)]
pub struct FileSendProgress {
    pub total_bytes: u64,
    pub bytes_sent: u64,
    pub chunks_sent: usize,
    pub complete: bool,
    pub error: Option<String>,
    pub loops_completed: usize,
}

impl FileSendProgress {
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
    /// Channel to receive progress updates (latest value semantics)
    progress_rx: watch::Receiver<FileSendProgress>,
    /// Channel to send cancel signal
    cancel_tx: mpsc::Sender<()>,
}

impl FileSendHandle {
    /// Get the current progress (non-blocking, always returns latest)
    pub fn progress(&self) -> FileSendProgress {
        self.progress_rx.borrow().clone()
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

    let metadata = tokio::fs::metadata(&path).await?;
    let total_bytes = metadata.len();

    let (progress_tx, progress_rx) = watch::channel(FileSendProgress {
        total_bytes,
        ..Default::default()
    });
    let (cancel_tx, cancel_rx) = mpsc::channel(1);

    let session_clone = session.clone_command_sender();

    tokio::spawn(async move {
        send_file_task(
            session_clone,
            path,
            config,
            total_bytes,
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

async fn send_file_task(
    session: mpsc::Sender<crate::session::SessionCommand>,
    path: std::path::PathBuf,
    config: FileSendConfig,
    total_bytes: u64,
    progress_tx: watch::Sender<FileSendProgress>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    let mut progress = FileSendProgress {
        total_bytes,
        ..Default::default()
    };

    loop {
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

        if !config.repeat {
            progress.complete = true;
            let _ = progress_tx.send(progress);
            return;
        }

        let _ = progress_tx.send(progress.clone());
    }
}

async fn send_bytes_chunked(
    session: &mpsc::Sender<crate::session::SessionCommand>,
    path: &std::path::Path,
    chunk_size: usize,
    config: &FileSendConfig,
    progress: &mut FileSendProgress,
    progress_tx: &watch::Sender<FileSendProgress>,
    cancel_rx: &mut mpsc::Receiver<()>,
) -> bool {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            progress.error = Some(e.to_string());
            progress.complete = true;
            let _ = progress_tx.send(progress.clone());
            return false;
        }
    };

    let mut buffer = vec![0u8; chunk_size];
    let mut bytes_consumed: u64 = 0;

    loop {
        if cancel_rx.try_recv().is_ok() {
            progress.complete = true;
            progress.error = Some("Cancelled".to_string());
            let _ = progress_tx.send(progress.clone());
            return false;
        }

        // Read a chunk
        let n = match file.read(&mut buffer).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                progress.complete = true;
                progress.error = Some(e.to_string());
                let _ = progress_tx.send(progress.clone());
                return false;
            }
        };

        bytes_consumed += n as u64;

        // Add optional suffix
        let chunk = build_chunk(&buffer[..n], config);

        if !send_chunk(session, chunk, progress, progress_tx, bytes_consumed).await {
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
    progress_tx: &watch::Sender<FileSendProgress>,
    cancel_rx: &mut mpsc::Receiver<()>,
) -> bool {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            progress.error = Some(e.to_string());
            progress.complete = true;
            let _ = progress_tx.send(progress.clone());
            return false;
        }
    };

    let delimiter_bytes = delimiter.as_bytes();
    let finder = memmem::Finder::new(delimiter_bytes);
    let units_per_chunk = config.units_per_chunk.max(1);

    // Main buffer for reading data. After processing, any incomplete unit (data after
    // the last delimiter) is moved to `remainder`, then `pending` is cleared and we
    // start fresh with `remainder` prepended to the next read.
    let mut pending = Vec::with_capacity(STREAM_BUFFER_SIZE);
    let mut remainder: Vec<u8> = Vec::new();

    // Buffer to accumulate multiple delimiter-separated units before sending
    let mut chunk_buffer = Vec::new();
    let mut units_in_buffer = 0;

    // Track bytes consumed from file (for accurate progress)
    let mut bytes_consumed: u64 = 0;

    loop {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            progress.complete = true;
            progress.error = Some("Cancelled".to_string());
            let _ = progress_tx.send(progress.clone());
            return false;
        }

        // Prepare pending buffer: start with any remainder from previous iteration
        pending.clear();
        pending.append(&mut remainder);

        let prev_len = pending.len();
        pending.resize(prev_len + STREAM_BUFFER_SIZE, 0);

        let n = match file.read(&mut pending[prev_len..]).await {
            Ok(0) => {
                pending.truncate(prev_len);
                if !pending.is_empty() {
                    chunk_buffer.extend_from_slice(&pending);
                    bytes_consumed += pending.len() as u64;
                }
                if !chunk_buffer.is_empty() {
                    let chunk = build_chunk(&chunk_buffer, config);
                    if !send_chunk(session, chunk, progress, progress_tx, bytes_consumed).await {
                        return false;
                    }
                }
                break;
            }
            Ok(n) => n,
            Err(e) => {
                progress.complete = true;
                progress.error = Some(e.to_string());
                let _ = progress_tx.send(progress.clone());
                return false;
            }
        };

        pending.truncate(prev_len + n);

        let mut search_start = 0;
        loop {
            let search_slice = &pending[search_start..];
            let Some(pos) = finder.find(search_slice) else {
                break;
            };

            let abs_pos = search_start + pos;
            let unit_end = if config.include_delimiter {
                abs_pos + delimiter_bytes.len()
            } else {
                abs_pos
            };

            if unit_end > search_start {
                chunk_buffer.extend_from_slice(&pending[search_start..unit_end]);
            }
            units_in_buffer += 1;

            let consumed_end = abs_pos + delimiter_bytes.len();
            bytes_consumed += (consumed_end - search_start) as u64;

            search_start = consumed_end;

            if units_in_buffer >= units_per_chunk {
                let chunk = build_chunk(&chunk_buffer, config);

                if !send_chunk(session, chunk, progress, progress_tx, bytes_consumed).await {
                    return false;
                }

                if config.chunk_delay > Duration::ZERO {
                    tokio::time::sleep(config.chunk_delay).await;
                }

                chunk_buffer.clear();
                units_in_buffer = 0;
            }
        }

        if search_start < pending.len() {
            remainder.extend_from_slice(&pending[search_start..]);
        }
    }

    true
}

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

async fn send_chunk(
    session: &mpsc::Sender<SessionCommand>,
    chunk: Cow<'_, [u8]>,
    progress: &mut FileSendProgress,
    progress_tx: &watch::Sender<FileSendProgress>,
    bytes_consumed: u64,
) -> bool {
    if session
        .send(SessionCommand::Send(chunk.into_owned()))
        .await
        .is_err()
    {
        progress.complete = true;
        progress.error = Some("Session closed".to_string());
        let _ = progress_tx.send(progress.clone());
        return false;
    }

    progress.bytes_sent = bytes_consumed;
    progress.chunks_sent += 1;

    let _ = progress_tx.send(progress.clone());

    true
}
