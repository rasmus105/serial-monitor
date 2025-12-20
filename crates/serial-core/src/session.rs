//! Session management for serial port connections
//!
//! A Session represents a single serial port connection with its data buffer.
//! It handles async I/O internally and communicates via channels.

use std::sync::{Arc, RwLock};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::SerialPortBuilderExt;

use crate::buffer::{DataBuffer, DataChunk, Direction};
use crate::error::{Error, Result};
use crate::port::SerialConfig;

/// Events emitted by a session
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// New data received from the device
    DataReceived(DataChunk),
    /// Data was sent to the device
    DataSent(DataChunk),
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

    /// Clone the command sender (for use by file sender)
    pub(crate) fn clone_sender(&self) -> mpsc::Sender<SessionCommand> {
        self.command_tx.clone()
    }
}

/// Session builder and connector
pub struct Session;

impl Session {
    /// Connect to a serial port and start a new session
    ///
    /// Returns a `SessionHandle` for interacting with the session.
    pub async fn connect(port_name: &str, config: SerialConfig) -> Result<SessionHandle> {
        Self::connect_with_buffer_size(port_name, config, DataBuffer::DEFAULT_MAX_SIZE).await
    }

    /// Connect with a custom buffer size
    pub async fn connect_with_buffer_size(
        port_name: &str,
        config: SerialConfig,
        buffer_size: usize,
    ) -> Result<SessionHandle> {
        // Open the serial port
        let port = tokio_serial::new(port_name, config.baud_rate)
            .data_bits(config.data_bits)
            .parity(config.parity)
            .stop_bits(config.stop_bits)
            .flow_control(config.flow_control)
            .open_native_async()?;

        // Create shared buffer
        let buffer = Arc::new(RwLock::new(DataBuffer::with_max_size(buffer_size)));

        // Create channels
        let (event_tx, event_rx) = mpsc::channel(256);
        let (command_tx, command_rx) = mpsc::channel(64);

        // Clone for the I/O task
        let buffer_clone = Arc::clone(&buffer);
        let port_name_owned = port_name.to_string();

        // Spawn the I/O task
        tokio::spawn(async move {
            io_task(port, buffer_clone, event_tx, command_rx).await;
        });

        Ok(SessionHandle {
            buffer,
            event_rx,
            command_tx,
            port_name: port_name_owned,
        })
    }
}

/// The async I/O task that handles serial communication
async fn io_task(
    port: tokio_serial::SerialStream,
    buffer: Arc<RwLock<DataBuffer>>,
    event_tx: mpsc::Sender<SessionEvent>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
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
                        let _ = event_tx.send(SessionEvent::Disconnected { error: None }).await;
                        break;
                    }
                    Ok(n) => {
                        let data = read_buf[..n].to_vec();
                        let chunk = DataChunk::new(Direction::Rx, data);

                        // Store in buffer
                        {
                            let mut buf = buffer.write().unwrap();
                            buf.push(chunk.clone());
                        }

                        // Notify UI
                        let _ = event_tx.send(SessionEvent::DataReceived(chunk)).await;
                    }
                    Err(e) => {
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
                                let chunk = DataChunk::new(Direction::Tx, data);

                                // Store in buffer
                                {
                                    let mut buf = buffer.write().unwrap();
                                    buf.push(chunk.clone());
                                }

                                // Notify UI
                                let _ = event_tx.send(SessionEvent::DataSent(chunk)).await;
                            }
                            Err(e) => {
                                let _ = event_tx.send(SessionEvent::Error(e.to_string())).await;
                            }
                        }
                    }
                    Some(SessionCommand::Disconnect) | None => {
                        let _ = event_tx.send(SessionEvent::Disconnected { error: None }).await;
                        break;
                    }
                }
            }
        }
    }
}
