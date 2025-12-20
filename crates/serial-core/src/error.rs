//! Error types for serial-core

use thiserror::Error;

/// Result type alias for serial-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in serial-core
#[derive(Debug, Error)]
pub enum Error {
    /// Serial port error
    #[error("Serial port error: {0}")]
    SerialPort(#[from] serialport::Error),

    /// I/O error from std::io
    #[error("I/O error: {0}")]
    StdIo(#[from] std::io::Error),

    /// I/O error with custom message
    #[error("I/O error: {0}")]
    Io(String),

    /// Port not connected
    #[error("Port not connected")]
    NotConnected,

    /// Port already connected
    #[error("Port already connected")]
    AlreadyConnected,

    /// Channel send error
    #[error("Channel send failed")]
    ChannelSend,

    /// Channel receive error
    #[error("Channel receive failed")]
    ChannelRecv,

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
