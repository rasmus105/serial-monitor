//! Chunking strategies for serial data
//!
//! Determines how incoming bytes are grouped into DataChunks.
//! This affects how data is displayed, searched, and filtered in the UI.

use crate::buffer::{DataChunk, Direction};

/// Strategy for chunking incoming serial data
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ChunkingStrategy {
    /// Raw chunking - chunks are created based on OS read timing.
    /// Each `read()` call creates one chunk with whatever bytes were available.
    /// Fast and low overhead, but chunk boundaries are unpredictable.
    #[default]
    Raw,

    /// Line-delimited chunking - chunks are split on a delimiter (typically newline).
    /// Each chunk contains exactly one "line" (data up to and including the delimiter).
    /// Incomplete lines are buffered until the delimiter arrives.
    LineDelimited {
        /// The delimiter byte(s) to split on
        delimiter: LineDelimiter,
        /// Maximum bytes to buffer before forcing a chunk (prevents memory issues
        /// if delimiter never arrives). Default: 64KB
        max_line_length: usize,
    },
}

impl ChunkingStrategy {
    /// Create a line-delimited strategy with newline delimiter
    pub fn line_delimited() -> Self {
        Self::LineDelimited {
            delimiter: LineDelimiter::Newline,
            max_line_length: 64 * 1024,
        }
    }

    /// Create a line-delimited strategy with a custom delimiter
    pub fn with_delimiter(delimiter: LineDelimiter) -> Self {
        Self::LineDelimited {
            delimiter,
            max_line_length: 64 * 1024,
        }
    }

    /// Create a line-delimited strategy with custom max line length
    pub fn with_max_line_length(mut self, max: usize) -> Self {
        if let Self::LineDelimited {
            max_line_length, ..
        } = &mut self
        {
            *max_line_length = max;
        }
        self
    }
}

/// Line delimiter options
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LineDelimiter {
    /// Unix-style newline: `\n` (0x0A)
    #[default]
    Newline,
    /// Windows-style: `\r\n` (0x0D 0x0A)
    CrLf,
    /// Carriage return only: `\r` (0x0D)
    Cr,
    /// Custom single-byte delimiter
    Byte(u8),
    /// Custom multi-byte delimiter
    Bytes(Vec<u8>),
}

impl LineDelimiter {
    /// Get the delimiter as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            LineDelimiter::Newline => b"\n",
            LineDelimiter::CrLf => b"\r\n",
            LineDelimiter::Cr => b"\r",
            LineDelimiter::Byte(b) => std::slice::from_ref(b),
            LineDelimiter::Bytes(v) => v.as_slice(),
        }
    }

    /// Find the delimiter in a byte slice, returning the position after it
    /// Returns None if delimiter not found
    pub fn find_end(&self, data: &[u8]) -> Option<usize> {
        let delim = self.as_bytes();
        if delim.is_empty() {
            return None;
        }

        // Use a simple search - could optimize with memchr for single bytes
        for i in 0..=data.len().saturating_sub(delim.len()) {
            if data[i..].starts_with(delim) {
                return Some(i + delim.len());
            }
        }
        None
    }
}

/// Chunker state machine that processes incoming bytes according to a strategy
#[derive(Debug)]
pub struct Chunker {
    strategy: ChunkingStrategy,
    /// Buffer for incomplete lines (only used in LineDelimited mode)
    pending: Vec<u8>,
    /// Direction for chunks created by this chunker
    direction: Direction,
}

impl Chunker {
    /// Create a new chunker with the given strategy
    pub fn new(strategy: ChunkingStrategy, direction: Direction) -> Self {
        Self {
            strategy,
            pending: Vec::new(),
            direction,
        }
    }

    /// Create a chunker for received data (RX)
    pub fn rx(strategy: ChunkingStrategy) -> Self {
        Self::new(strategy, Direction::Rx)
    }

    /// Create a chunker for transmitted data (TX)
    pub fn tx(strategy: ChunkingStrategy) -> Self {
        Self::new(strategy, Direction::Tx)
    }

    /// Process incoming bytes and return any complete chunks
    ///
    /// In Raw mode, always returns exactly one chunk containing all input bytes.
    /// In LineDelimited mode, may return 0, 1, or many chunks depending on delimiters.
    pub fn process(&mut self, data: &[u8]) -> Vec<DataChunk> {
        match &self.strategy {
            ChunkingStrategy::Raw => {
                // Raw mode: one chunk per process() call
                vec![DataChunk::new(self.direction, data.to_vec())]
            }
            ChunkingStrategy::LineDelimited {
                delimiter,
                max_line_length,
            } => {
                let mut chunks = Vec::new();

                // Add new data to pending buffer
                self.pending.extend_from_slice(data);

                // Extract complete lines
                loop {
                    // Check for max line length first (safety valve)
                    if self.pending.len() >= *max_line_length {
                        // Force emit what we have
                        let data = std::mem::take(&mut self.pending);
                        chunks.push(DataChunk::new(self.direction, data));
                        break;
                    }

                    // Look for delimiter
                    if let Some(end) = delimiter.find_end(&self.pending) {
                        // Found delimiter - emit chunk including delimiter
                        let line: Vec<u8> = self.pending.drain(..end).collect();
                        chunks.push(DataChunk::new(self.direction, line));
                    } else {
                        // No delimiter found, wait for more data
                        break;
                    }
                }

                chunks
            }
        }
    }

    /// Flush any pending data as a final chunk
    ///
    /// Call this when the connection closes to emit any incomplete line.
    pub fn flush(&mut self) -> Option<DataChunk> {
        if self.pending.is_empty() {
            None
        } else {
            let data = std::mem::take(&mut self.pending);
            Some(DataChunk::new(self.direction, data))
        }
    }

    /// Check if there's pending data waiting for a delimiter
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get the amount of pending data in bytes
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_chunking() {
        let mut chunker = Chunker::rx(ChunkingStrategy::Raw);

        let chunks = chunker.process(b"Hello");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b"Hello");

        let chunks = chunker.process(b" World");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b" World");
    }

    #[test]
    fn test_line_delimited_complete_lines() {
        let mut chunker = Chunker::rx(ChunkingStrategy::line_delimited());

        // Two complete lines in one read
        let chunks = chunker.process(b"line1\nline2\n");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, b"line1\n");
        assert_eq!(chunks[1].data, b"line2\n");
        assert!(!chunker.has_pending());
    }

    #[test]
    fn test_line_delimited_partial_line() {
        let mut chunker = Chunker::rx(ChunkingStrategy::line_delimited());

        // Partial line - should buffer
        let chunks = chunker.process(b"Hello");
        assert_eq!(chunks.len(), 0);
        assert!(chunker.has_pending());
        assert_eq!(chunker.pending_len(), 5);

        // Complete the line
        let chunks = chunker.process(b" World\n");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b"Hello World\n");
        assert!(!chunker.has_pending());
    }

    #[test]
    fn test_line_delimited_split_across_reads() {
        let mut chunker = Chunker::rx(ChunkingStrategy::line_delimited());

        // First part
        let chunks = chunker.process(b"Hello ");
        assert_eq!(chunks.len(), 0);

        // Middle part
        let chunks = chunker.process(b"World ");
        assert_eq!(chunks.len(), 0);

        // Final part with delimiter
        let chunks = chunker.process(b"1\nHello World 2\n");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, b"Hello World 1\n");
        assert_eq!(chunks[1].data, b"Hello World 2\n");
    }

    #[test]
    fn test_line_delimited_flush() {
        let mut chunker = Chunker::rx(ChunkingStrategy::line_delimited());

        // Incomplete line
        let chunks = chunker.process(b"no newline");
        assert_eq!(chunks.len(), 0);

        // Flush should emit it
        let chunk = chunker.flush();
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().data, b"no newline");
    }

    #[test]
    fn test_line_delimited_crlf() {
        let mut chunker = Chunker::rx(ChunkingStrategy::with_delimiter(LineDelimiter::CrLf));

        let chunks = chunker.process(b"line1\r\nline2\r\n");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, b"line1\r\n");
        assert_eq!(chunks[1].data, b"line2\r\n");
    }

    #[test]
    fn test_line_delimited_max_length() {
        let strategy = ChunkingStrategy::LineDelimited {
            delimiter: LineDelimiter::Newline,
            max_line_length: 10,
        };
        let mut chunker = Chunker::rx(strategy);

        // Send more than max_line_length without delimiter
        let chunks = chunker.process(b"12345678901234567890");
        // Should force a chunk at max_line_length
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_delimiter_find() {
        assert_eq!(LineDelimiter::Newline.find_end(b"hello\nworld"), Some(6));
        assert_eq!(LineDelimiter::Newline.find_end(b"hello"), None);
        assert_eq!(LineDelimiter::CrLf.find_end(b"hello\r\nworld"), Some(7));
        assert_eq!(LineDelimiter::CrLf.find_end(b"hello\nworld"), None);
    }
}
