//! Serial Monitor Core Library
//!
//! This crate provides the frontend-agnostic core functionality for serial monitoring:
//! - Serial port enumeration and connection
//! - Data buffering with timestamps and direction tracking
//! - Async I/O with channel-based communication
//! - Search and filter utilities for encoded data
//! - File saving with configurable formats and scopes
//! - Auto-save for crash recovery
//! - Keep-awake to prevent system sleep during active sessions
//!
//! # Design Principles
//!
//! - **No UI dependencies**: This crate must never depend on any UI framework
//! - **Raw bytes as source of truth**: All data stored as raw bytes, encoding is UI's job
//! - **Non-blocking**: All operations are async or return immediately

mod chunking;
mod error;
mod file_sender;
pub mod keep_awake;
mod port;
mod session;
pub mod settings;

// Utility modules for frontends to avoid duplication
pub mod buffer;
pub mod crash;
pub mod ui;

// Re-export commonly used types from buffer
pub use buffer::{
    AutoSaveConfig, BinaryFormat, ChunkView, DataBuffer, Direction, DirectionFilter, Encoding,
    HexFormat, PatternMatcher, PatternMode, SaveFormat, SaveScope, SearchMatch, UserSaveConfig,
    default_cache_directory, encode, encode_ascii, encode_binary, encode_hex, encode_utf8, graph,
};
pub use chunking::{Chunker, ChunkingStrategy, LineDelimiter};
pub use error::{Error, Result};
pub use file_sender::{
    ChunkMode, Delimiter, FileSendConfig, FileSendHandle, FileSendProgress, send_file,
};
pub use keep_awake::KeepAwake;
pub use port::{DataBits, FlowControl, Parity, PortInfo, SerialConfig, StopBits, list_ports};
pub use session::{
    Session, SessionCommand, SessionConfig, SessionEvent, SessionHandle, Statistics,
};
