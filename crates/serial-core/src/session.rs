//! Session management for serial port connections
//!
//! A Session represents a single serial port connection with its data buffer.
//! It handles async I/O internally and communicates via channels.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::{ClearBuffer, SerialPort, SerialPortBuilderExt};

use crate::buffer::file_saver::{self, AutoSaveSender, FileSaverHandle};
use crate::buffer::{AutoSaveConfig, DataBuffer, Direction};
use crate::chunking::{Chunker, ChunkingStrategy};
use crate::error::{Error, Result};
use crate::port::SerialConfig;

/// Connection statistics tracked during a session
///
/// All counters are cumulative and never decrease, even when the buffer
/// truncates old data.
///
/// Atomic integers are used for simplicity, so that we can easily share
/// statistics between threads (core and UI).
#[derive(Debug)]
pub struct Statistics {
    /// Total bytes received from the device
    bytes_rx: AtomicU64,
    /// Total bytes sent to the device
    bytes_tx: AtomicU64,
    /// Number of received packets/chunks
    packets_rx: AtomicU64,
    /// Number of sent packets/chunks
    packets_tx: AtomicU64,
    /// When the connection was established
    connected_at: Instant,
}

impl Statistics {
    fn new() -> Self {
        Self {
            bytes_rx: AtomicU64::new(0),
            bytes_tx: AtomicU64::new(0),
            packets_rx: AtomicU64::new(0),
            packets_tx: AtomicU64::new(0),
            connected_at: Instant::now(),
        }
    }

    /// Record received data
    fn record_rx(&self, bytes: usize) {
        self.bytes_rx.fetch_add(bytes as u64, Ordering::Relaxed);
        self.packets_rx.fetch_add(1, Ordering::Relaxed);
    }

    /// Record sent data
    fn record_tx(&self, bytes: usize) {
        self.bytes_tx.fetch_add(bytes as u64, Ordering::Relaxed);
        self.packets_tx.fetch_add(1, Ordering::Relaxed);
    }

    /// Total bytes received
    pub fn bytes_rx(&self) -> u64 {
        self.bytes_rx.load(Ordering::Relaxed)
    }

    /// Total bytes sent
    pub fn bytes_tx(&self) -> u64 {
        self.bytes_tx.load(Ordering::Relaxed)
    }

    /// Number of received packets/chunks
    pub fn packets_rx(&self) -> u64 {
        self.packets_rx.load(Ordering::Relaxed)
    }

    /// Number of sent packets/chunks
    pub fn packets_tx(&self) -> u64 {
        self.packets_tx.load(Ordering::Relaxed)
    }

    /// When the connection was established
    pub fn connected_at(&self) -> Instant {
        self.connected_at
    }

    /// Duration since connection was established
    pub fn duration(&self) -> std::time::Duration {
        self.connected_at.elapsed()
    }

    /// Average bytes received per second (returns 0.0 if duration is 0)
    pub fn avg_bytes_rx_per_sec(&self) -> f64 {
        let secs = self.duration().as_secs_f64();
        if secs > 0.0 {
            self.bytes_rx() as f64 / secs
        } else {
            0.0
        }
    }

    /// Average bytes sent per second (returns 0.0 if duration is 0)
    pub fn avg_bytes_tx_per_sec(&self) -> f64 {
        let secs = self.duration().as_secs_f64();
        if secs > 0.0 {
            self.bytes_tx() as f64 / secs
        } else {
            0.0
        }
    }
}

/// Events emitted by a session
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// New data received from the device
    DataReceived { data: Vec<u8>, direction: Direction },
    /// Data was sent to the device (otherwise UI doesn't know
    /// if the data operation succeeded)
    DataSent { data: Vec<u8>, direction: Direction },
    /// Connection established
    Connected,
    /// Connection closed (gracefully or due to error)
    Disconnected { error: Option<String> },
    /// An error occurred
    Error(String),
}

/// Commands sent to the session's I/O task
#[derive(Debug)]
pub enum SessionCommand {
    /// Send data to the serial port
    Send(Vec<u8>),
    /// Disconnect and stop the I/O task
    Disconnect,
}

/// Configuration for a session (beyond serial port settings)
#[derive(Debug, Clone, bon::Builder)]
pub struct SessionConfig {
    /// Strategy for chunking received data
    #[builder(default)]
    pub rx_chunking: ChunkingStrategy,
    /// Strategy for chunking transmitted data (usually Raw is fine)
    #[builder(default)]
    pub tx_chunking: ChunkingStrategy,
    /// Maximum buffer size in bytes
    pub buffer_size: Option<usize>,
    /// Auto-save configuration for crash recovery
    #[builder(default)]
    pub auto_save: AutoSaveConfig,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

/// Handle for interacting with an active session
///
/// This is the main interface for the UI to interact with a serial connection.
/// It provides non-blocking methods that communicate with the I/O task via channels.
pub struct SessionHandle {
    /// Shared buffer containing all session data
    buffer: Arc<RwLock<DataBuffer>>,
    /// Channel to receive events from the I/O task
    event_rx: mpsc::Receiver<SessionEvent>,
    /// Channel to send commands to the I/O task
    command_tx: mpsc::Sender<SessionCommand>,
    /// Port name for this session
    port_name: String,
    /// Auto-save handle (if enabled)
    auto_save: Option<FileSaverHandle>,
    /// Connection statistics
    statistics: Arc<Statistics>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("port_name", &self.port_name)
            .finish_non_exhaustive()
    }
}

impl SessionHandle {
    /// Get a read lock on the data buffer
    ///
    /// The UI should use this to access data for display.
    pub fn buffer(&self) -> std::sync::RwLockReadGuard<'_, DataBuffer> {
        self.buffer.read().unwrap()
    }

    /// Get a write lock on the data buffer
    ///
    /// Use this for operations that modify buffer state, such as:
    /// - Setting encoding
    /// - Enabling/disabling graph mode
    /// - Setting search/filter patterns
    pub fn buffer_mut(&self) -> std::sync::RwLockWriteGuard<'_, DataBuffer> {
        self.buffer.write().unwrap()
    }

    /// Send data to the serial port
    ///
    /// This is non-blocking - the data is queued for sending.
    pub async fn send(&self, data: Vec<u8>) -> Result<()> {
        self.command_tx
            .send(SessionCommand::Send(data))
            .await
            .map_err(|_| Error::ChannelSend)
    }

    /// Try to receive the next event (non-blocking)
    ///
    /// Returns `None` if no event is available.
    pub fn try_recv_event(&mut self) -> Option<SessionEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Receive the next event (async, blocking)
    pub async fn recv_event(&mut self) -> Option<SessionEvent> {
        self.event_rx.recv().await
    }

    /// Disconnect from the serial port
    pub async fn disconnect(self) -> Result<()> {
        let _ = self.command_tx.send(SessionCommand::Disconnect).await;
        Ok(())
    }

    /// Get the port name
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Check if auto-save is active
    pub fn is_auto_saving(&self) -> bool {
        self.auto_save.is_some()
    }

    /// Get the auto-save file path (if active)
    pub fn auto_save_path(&self) -> Option<&std::path::Path> {
        self.auto_save.as_ref().map(|h| h.file_path())
    }

    /// Clone the command sender (for use by file sender)
    pub(crate) fn clone_sender(&self) -> mpsc::Sender<SessionCommand> {
        self.command_tx.clone()
    }

    /// Get the connection statistics
    pub fn statistics(&self) -> &Statistics {
        &self.statistics
    }
}

/// Session builder and connector
pub struct Session;

impl Session {
    /// Connect to a serial port and start a new session
    ///
    /// Returns a `SessionHandle` for interacting with the session.
    pub async fn connect(port_name: &str, config: SerialConfig) -> Result<SessionHandle> {
        Self::connect_with_config(port_name, config, SessionConfig::default()).await
    }

    /// Connect with a custom buffer size (legacy API, use connect_with_config for new code)
    pub async fn connect_with_buffer_size(
        port_name: &str,
        config: SerialConfig,
        buffer_size: usize,
    ) -> Result<SessionHandle> {
        Self::connect_with_config(
            port_name,
            config,
            SessionConfig::builder().buffer_size(buffer_size).build(),
        )
        .await
    }

    /// Connect with full session configuration
    pub async fn connect_with_config(
        port_name: &str,
        serial_config: SerialConfig,
        session_config: SessionConfig,
    ) -> Result<SessionHandle> {
        // Open the serial port
        let port = tokio_serial::new(port_name, serial_config.baud_rate)
            .data_bits(serial_config.data_bits)
            .parity(serial_config.parity)
            .stop_bits(serial_config.stop_bits)
            .flow_control(serial_config.flow_control)
            .open_native_async()?;

        // Flush buffers to discard any stale data from before we connected.
        port.clear(ClearBuffer::All)?;

        // Create shared buffer
        let mut buffer = DataBuffer::default();
        if let Some(size) = session_config.buffer_size {
            buffer.max_size = size;
        }
        let buffer = Arc::new(RwLock::new(buffer));

        // Create channels
        let (event_tx, event_rx) = mpsc::channel(256);
        let (command_tx, command_rx) = mpsc::channel(64);

        // Clone for the I/O task
        let buffer_clone = Arc::clone(&buffer);
        let port_name_owned = port_name.to_string();

        // Create chunkers
        let rx_chunker = Chunker::rx(session_config.rx_chunking);
        let tx_chunker = Chunker::tx(session_config.tx_chunking);

        // Start auto-save if enabled
        let runtime = tokio::runtime::Handle::current();
        let auto_save = if session_config.auto_save.enabled {
            match file_saver::start_auto_save(&session_config.auto_save, port_name, &runtime) {
                Ok(handle) => Some(handle),
                Err(e) => {
                    // Log but don't fail the connection if auto-save fails
                    eprintln!("Warning: Failed to start auto-save: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Clone auto-save sender for the I/O task
        let auto_save_tx = auto_save.as_ref().map(|h| h.clone_sender());

        // Create statistics
        let statistics = Arc::new(Statistics::new());
        let statistics_clone = Arc::clone(&statistics);

        // Spawn the I/O task
        tokio::spawn(async move {
            io_task(
                port,
                buffer_clone,
                event_tx,
                command_rx,
                rx_chunker,
                tx_chunker,
                auto_save_tx,
                statistics_clone,
            )
            .await;
        });

        Ok(SessionHandle {
            buffer,
            event_rx,
            command_tx,
            port_name: port_name_owned,
            auto_save,
            statistics,
        })
    }
}

/// The async I/O task that handles serial communication
async fn io_task(
    port: tokio_serial::SerialStream,
    buffer: Arc<RwLock<DataBuffer>>,
    event_tx: mpsc::Sender<SessionEvent>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    mut rx_chunker: Chunker,
    mut tx_chunker: Chunker,
    auto_save: Option<AutoSaveSender>,
    statistics: Arc<Statistics>,
) {
    let (mut reader, mut writer) = tokio::io::split(port);
    let mut read_buf = [0u8; 1024];

    // Send connected event
    let _ = event_tx.send(SessionEvent::Connected).await;

    loop {
        tokio::select! {
            // Handle incoming data from serial port
            result = reader.read(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        // EOF - port closed
                        // Flush any pending data
                        if let Some(data) = rx_chunker.flush() {
                            let direction = rx_chunker.direction();
                            {
                                let mut buf = buffer.write().unwrap();
                                buf.push(data.clone(), direction);
                            }
                            if let Some(ref saver) = auto_save {
                                saver.write_new(data.clone(), direction);
                            }
                            let _ = event_tx.send(SessionEvent::DataReceived { data, direction }).await;
                        }
                        let _ = event_tx.send(SessionEvent::Disconnected { error: None }).await;
                        break;
                    }
                    Ok(n) => {
                        // Record raw bytes received before chunking
                        statistics.record_rx(n);

                        // Process through chunker - may produce 0, 1, or many chunks
                        let chunks = rx_chunker.process(&read_buf[..n]);
                        let direction = rx_chunker.direction();

                        for data in chunks {
                            // Store in buffer
                            {
                                let mut buf = buffer.write().unwrap();
                                buf.push(data.clone(), direction);
                            }
                            if let Some(ref saver) = auto_save {
                                saver.write_new(data.clone(), direction);
                            }
                            // Notify UI
                            let _ = event_tx.send(SessionEvent::DataReceived { data, direction }).await;
                        }
                    }
                    Err(e) => {
                        // Flush any pending data before disconnecting
                        if let Some(data) = rx_chunker.flush() {
                            let direction = rx_chunker.direction();
                            {
                                let mut buf = buffer.write().unwrap();
                                buf.push(data.clone(), direction);
                            }
                            if let Some(ref saver) = auto_save {
                                saver.write_new(data.clone(), direction);
                            }
                            let _ = event_tx.send(SessionEvent::DataReceived { data, direction }).await;
                        }
                        let _ = event_tx.send(SessionEvent::Disconnected {
                            error: Some(e.to_string())
                        }).await;
                        break;
                    }
                }
            }

            // Handle commands from the UI
            cmd = command_rx.recv() => {
                match cmd {
                    Some(SessionCommand::Send(data)) => {
                        match writer.write_all(&data).await {
                            Ok(()) => {
                                // Record bytes sent
                                statistics.record_tx(data.len());

                                // Process through TX chunker
                                let chunks = tx_chunker.process(&data);
                                let direction = tx_chunker.direction();

                                for chunk_data in chunks {
                                    // Store in buffer
                                    {
                                        let mut buf = buffer.write().unwrap();
                                        buf.push(chunk_data.clone(), direction);
                                    }
                                    if let Some(ref saver) = auto_save {
                                        saver.write_new(chunk_data.clone(), direction);
                                    }
                                    // Notify UI
                                    let _ = event_tx.send(SessionEvent::DataSent { data: chunk_data, direction }).await;
                                }

                                // For TX, we might want to flush immediately if using line-delimited
                                // (since the user's send is a complete "message")
                                if let Some(chunk_data) = tx_chunker.flush() {
                                    let direction = tx_chunker.direction();
                                    {
                                        let mut buf = buffer.write().unwrap();
                                        buf.push(chunk_data.clone(), direction);
                                    }
                                    if let Some(ref saver) = auto_save {
                                        saver.write_new(chunk_data.clone(), direction);
                                    }
                                    let _ = event_tx.send(SessionEvent::DataSent { data: chunk_data, direction }).await;
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(SessionEvent::Error(e.to_string())).await;
                            }
                        }
                    }
                    Some(SessionCommand::Disconnect) | None => {
                        // Flush any pending data
                        if let Some(data) = rx_chunker.flush() {
                            let direction = rx_chunker.direction();
                            {
                                let mut buf = buffer.write().unwrap();
                                buf.push(data.clone(), direction);
                            }
                            if let Some(ref saver) = auto_save {
                                saver.write_new(data.clone(), direction);
                            }
                            let _ = event_tx.send(SessionEvent::DataReceived { data, direction }).await;
                        }
                        let _ = event_tx.send(SessionEvent::Disconnected { error: None }).await;
                        break;
                    }
                }
            }
        }
    }
}
