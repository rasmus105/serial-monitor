//! Serial Monitor Core Library
//!
//! This crate provides the frontend-agnostic core functionality for serial monitoring:
//! - Serial port enumeration and connection
//! - Data buffering with timestamps and direction tracking
//! - Async I/O with channel-based communication
//! - Search and filter utilities for encoded data
//!
//! # Design Principles
//!
//! - **No UI dependencies**: This crate must never depend on any UI framework
//! - **Raw bytes as source of truth**: All data stored as raw bytes, encoding is UI's job
//! - **Non-blocking**: All operations are async or return immediately

mod buffer;
mod chunking;
mod encoding;
mod error;
mod file_saver;
mod file_sender;
mod port;
mod session;

// utility crates to be used by libraries to avoid duplication across front-ends.
pub mod display;
pub mod graph;

pub use buffer::{DataBuffer, DataChunk, Direction};
pub use chunking::{Chunker, ChunkingStrategy, LineDelimiter};
pub use display::{PatternMatcher, PatternMode, SearchMatch};
pub use encoding::{Encoding, encode, encode_ascii, encode_binary, encode_hex, encode_utf8};
pub use error::{Error, Result};
pub use file_saver::{
    FileSaveConfig, FileSaverCommand, FileSaverHandle, SaveFormat, start_file_saver,
};
pub use file_sender::{FileSendConfig, FileSendHandle, FileSendProgress, send_file};
pub use port::{DataBits, FlowControl, Parity, PortInfo, SerialConfig, StopBits, list_ports};
pub use session::{Session, SessionCommand, SessionConfig, SessionEvent, SessionHandle};
