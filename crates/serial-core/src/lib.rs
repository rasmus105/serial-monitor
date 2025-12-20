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
mod error;
mod port;
mod session;

pub use buffer::{DataBuffer, DataChunk, Direction};
pub use error::{Error, Result};
pub use port::{list_ports, PortInfo, SerialConfig};
pub use session::{Session, SessionEvent, SessionHandle};
