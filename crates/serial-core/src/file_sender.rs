//! File sending functionality with chunking and progress reporting
//!
//! Supports sending files with configurable chunk sizes and delays between chunks.

use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::error::Result;
use crate::session::SessionHandle;

/// Configuration for file sending
#[derive(Debug, Clone)]
pub struct FileSendConfig {
    /// Size of each chunk in bytes
    pub chunk_size: usize,
    /// Delay between chunks
    pub chunk_delay: Duration,
    /// Whether to loop the file continuously
    pub repeat: bool,
}

impl Default for FileSendConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64,
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
    let total_chunks = (total_bytes as usize).div_ceil(config.chunk_size);

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

        // Open file for each loop iteration
        let mut file = match File::open(&path).await {
            Ok(f) => f,
            Err(e) => {
                progress.error = Some(e.to_string());
                progress.complete = true;
                let _ = progress_tx.send(progress).await;
                return;
            }
        };
        let mut buffer = vec![0u8; config.chunk_size];

        loop {
            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                progress.complete = true;
                progress.error = Some("Cancelled".to_string());
                let _ = progress_tx.send(progress).await;
                return;
            }

            // Read a chunk
            let n = match file.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(e) => {
                    progress.complete = true;
                    progress.error = Some(e.to_string());
                    let _ = progress_tx.send(progress).await;
                    return;
                }
            };

            // Send the chunk
            let chunk = buffer[..n].to_vec();
            if session
                .send(crate::session::SessionCommand::Send(chunk))
                .await
                .is_err()
            {
                progress.complete = true;
                progress.error = Some("Session closed".to_string());
                let _ = progress_tx.send(progress).await;
                return;
            }

            progress.bytes_sent += n as u64;
            progress.chunks_sent += 1;

            // Send progress update
            let _ = progress_tx.send(progress.clone()).await;

            // Delay between chunks
            if config.chunk_delay > Duration::ZERO {
                tokio::time::sleep(config.chunk_delay).await;
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
