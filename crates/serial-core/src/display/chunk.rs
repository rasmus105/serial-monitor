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



#[cfg(test)]
mod tests {
    // Tests will be added with DisplayBuffer integration tests
}
