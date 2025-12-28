//! Display chunk type
//!
//! Represents an encoded chunk ready for display, containing the
//! string representation and metadata from the original data chunk.

use crate::Direction;

/// An encoded chunk ready for display
///
/// This is the display-ready representation of a raw `DataChunk`.
/// The content is encoded according to the current encoding setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayChunk {
    /// The encoded string representation
    pub content: String,
    /// Direction (TX/RX) - copied from the raw chunk
    pub direction: Direction,
}

impl DisplayChunk {
    /// Check if this chunk was transmitted (TX)
    pub fn is_tx(&self) -> bool {
        self.direction == Direction::Tx
    }

    /// Check if this chunk was received (RX)
    pub fn is_rx(&self) -> bool {
        self.direction == Direction::Rx
    }

    /// Get the length of the content in bytes
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if the content is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added with DisplayBuffer integration tests
}
