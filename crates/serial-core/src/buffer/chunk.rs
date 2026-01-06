//! Chunk types for raw storage and display views
//!
//! This module contains:
//! - [`RawChunk`]: Internal storage with raw bytes (hidden from frontends)
//! - [`ChunkView`]: Borrowed view for frontend iteration
//! - [`Direction`]: TX/RX direction enum

use std::time::SystemTime;

use strum::{AsRefStr, Display, EnumString};

/// Direction of data transmission
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, AsRefStr, EnumString)]
pub enum Direction {
    /// Data transmitted (sent by user)
    #[default]
    #[strum(serialize = "TX")]
    Tx,
    /// Data received (from device)
    #[strum(serialize = "RX")]
    Rx,
}

/// Internal raw data storage
///
/// Contains the source-of-truth raw bytes along with metadata.
/// This type is `pub(crate)` - frontends never see it directly.
#[derive(Debug, Clone)]
pub(crate) struct RawChunk {
    /// Raw bytes as received/sent
    pub data: Vec<u8>,
    /// Direction (TX/RX)
    pub direction: Direction,
    /// System timestamp when chunk was created
    pub timestamp: SystemTime,
}

/// Borrowed view of a chunk for frontend iteration
///
/// This is what frontends see when iterating chunks. It provides
/// access to the encoded string and metadata without exposing raw bytes.
///
/// # Lifetime
///
/// The `'a` lifetime is tied to the [`DataBuffer`](super::DataBuffer) borrow,
/// ensuring the view remains valid while iterating.
#[derive(Debug, Clone, Copy)]
pub struct ChunkView<'a> {
    /// Encoded string representation
    pub encoded: &'a str,
    /// Direction (TX/RX)
    pub direction: Direction,
    /// System timestamp
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_display() {
        assert_eq!(Direction::Tx.to_string(), "TX");
        assert_eq!(Direction::Rx.to_string(), "RX");
    }
}
