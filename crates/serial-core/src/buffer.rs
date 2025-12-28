//! Data buffer implementation
//!
//! Stores raw bytes with timestamps and direction metadata.
//! The buffer has a configurable size limit and drops oldest data when full.

use std::collections::VecDeque;
use std::time::SystemTime;

/// Direction of data flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Data sent by user to the device
    Tx,
    /// Data received from the device
    Rx,
}

/// A chunk of serial data with metadata
#[derive(Debug, Clone)]
pub struct DataChunk {
    /// When this chunk was received/sent
    pub timestamp: SystemTime,
    /// Direction of the data (TX or RX)
    pub direction: Direction,
    /// Raw bytes
    pub data: Vec<u8>,
}

impl DataChunk {
    /// Create a new data chunk with the current timestamp
    pub fn new(direction: Direction, data: Vec<u8>) -> Self {
        Self {
            timestamp: SystemTime::now(),
            direction,
            data,
        }
    }

    /// Create a new TX chunk
    pub fn tx(data: Vec<u8>) -> Self {
        Self::new(Direction::Tx, data)
    }

    /// Create a new RX chunk
    pub fn rx(data: Vec<u8>) -> Self {
        Self::new(Direction::Rx, data)
    }

    /// Size of this chunk in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Buffer for storing serial data with size limits
///
/// When the buffer exceeds its size limit, oldest chunks are dropped.
#[derive(Debug)]
pub struct DataBuffer {
    chunks: VecDeque<DataChunk>,
    /// Current total size in bytes
    current_size: usize,
    /// Maximum size in bytes
    max_size: usize,
}

impl DataBuffer {
    /// Default buffer size: 100 MB
    pub const DEFAULT_MAX_SIZE: usize = 100 * 1024 * 1024;

    /// Create a new buffer with the default size limit
    pub fn new() -> Self {
        Self::with_max_size(Self::DEFAULT_MAX_SIZE)
    }

    /// Create a new buffer with a custom size limit
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            current_size: 0,
            max_size,
        }
    }

    /// Push a new chunk into the buffer
    ///
    /// If this causes the buffer to exceed its size limit,
    /// oldest chunks will be dropped until it fits.
    pub fn push(&mut self, chunk: DataChunk) {
        let chunk_size = chunk.size();

        // If single chunk is larger than max, just store it and drop everything else
        if chunk_size >= self.max_size {
            self.chunks.clear();
            self.current_size = 0;
        }

        // Drop oldest chunks until we have room
        while self.current_size + chunk_size > self.max_size {
            if let Some(old) = self.chunks.pop_front() {
                self.current_size -= old.size();
            } else {
                break;
            }
        }

        self.current_size += chunk_size;
        self.chunks.push_back(chunk);
    }

    /// Get all chunks in the buffer
    pub fn chunks(&self) -> impl Iterator<Item = &DataChunk> {
        self.chunks.iter()
    }

    /// Get the number of chunks in the buffer
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Get the total size of data in the buffer (in bytes)
    pub fn total_size(&self) -> usize {
        self.current_size
    }

    /// Get the maximum size limit (in bytes)
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Clear all data from the buffer
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.current_size = 0;
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Get chunks starting from a given index
    ///
    /// Useful for incremental UI updates - the UI can track the last index
    /// it rendered and only request new chunks.
    pub fn chunks_from(&self, start_index: usize) -> impl Iterator<Item = &DataChunk> {
        self.chunks.iter().skip(start_index)
    }

    /// Get the latest N chunks
    pub fn latest_chunks(&self, count: usize) -> impl Iterator<Item = &DataChunk> {
        let skip = self.chunks.len().saturating_sub(count);
        self.chunks.iter().skip(skip)
    }
}

impl Default for DataBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_retrieve() {
        let mut buffer = DataBuffer::new();
        buffer.push(DataChunk::tx(vec![1, 2, 3]));
        buffer.push(DataChunk::rx(vec![4, 5, 6]));

        assert_eq!(buffer.chunk_count(), 2);
        assert_eq!(buffer.total_size(), 6);
    }

    #[test]
    fn test_size_limit() {
        let mut buffer = DataBuffer::with_max_size(10);

        buffer.push(DataChunk::rx(vec![1, 2, 3, 4, 5])); // 5 bytes
        buffer.push(DataChunk::rx(vec![6, 7, 8, 9, 10])); // 5 bytes, total 10
        assert_eq!(buffer.chunk_count(), 2);
        assert_eq!(buffer.total_size(), 10);

        buffer.push(DataChunk::rx(vec![11, 12, 13])); // 3 bytes, should drop first chunk
        assert_eq!(buffer.chunk_count(), 2);
        assert_eq!(buffer.total_size(), 8); // 5 + 3
    }
}
