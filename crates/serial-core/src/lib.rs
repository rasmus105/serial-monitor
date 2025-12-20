//! Serial Monitor Core Library
//!
//! This crate provides the frontend-agnostic core functionality for serial monitoring:
//! - Serial port enumeration and connection
//! - Data buffering with timestamps and direction tracking
//! - Async I/O with channel-based communication
//!
//! # Design Principles
//!
//! - **No UI dependencies**: This crate must never depend on any UI framework
//! - **Raw bytes as source of truth**: All data stored as raw bytes, encoding is UI's job
//! - **Non-blocking**: All operations are async or return immediately

mod buffer;
mod encoding;
mod error;
mod file_sender;
mod port;
mod session;

pub use buffer::{DataBuffer, DataChunk, Direction};
pub use encoding::{encode, encode_ascii, encode_binary, encode_hex, encode_utf8, Encoding};
pub use error::{Error, Result};
pub use file_sender::{send_file, FileSendConfig, FileSendHandle, FileSendProgress};
pub use port::{list_ports, DataBits, FlowControl, Parity, PortInfo, SerialConfig, StopBits};
pub use session::{Session, SessionCommand, SessionEvent, SessionHandle};
