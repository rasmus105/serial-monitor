//! Buffer module for serial monitor
//!
//! Central data management handling storage, encoding, filtering, searching,
//! and graphing of serial data.
//!
//! # Design Principle
//!
//! **Raw bytes are hidden from frontends.** Frontends only see encoded strings
//! via [`ChunkView`]. This encapsulation allows internal optimizations without
//! affecting the public API.
//!
//! # Architecture
//!
//! ```text
//! Serial I/O
//!     │
//!     ▼ push()
//! ┌─────────────────────────────────────────────────────┐
//! │                    DataBuffer                        │
//! │                                                      │
//! │  raw_chunks ──► encoded ──► filtered_indices        │
//! │      │              │              │                 │
//! │      │              │              ▼                 │
//! │      │              │         SearchState           │
//! │      │              │                                │
//! │      ▼                                               │
//! │  GraphEngine (lazy, parses raw as UTF-8)            │
//! └─────────────────────────────────────────────────────┘
//!     │
//!     ▼ chunks() -> Iterator<ChunkView>
//! Frontend (only sees encoded + metadata)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use serial_core::buffer::{DataBuffer, Encoding, PatternMode};
//!
//! let mut buffer = DataBuffer::default();
//!
//! // Set encoding (default is UTF-8)
//! buffer.set_encoding(Encoding::Hex(Default::default()));
//!
//! // Set up filtering
//! buffer.set_filter_pattern("error", PatternMode::Regex)?;
//! buffer.show_tx = false;
//!
//! // Search within filtered view
//! buffer.set_search_pattern("timeout", PatternMode::Normal)?;
//!
//! // Iterate visible chunks
//! for chunk in buffer.chunks() {
//!     println!("{}: {}", chunk.direction, chunk.encoded);
//! }
//! ```

mod chunk;
mod data_buffer;
mod encoding;
mod filter;
pub mod graph;
mod pattern;
mod search;

// Public exports
pub use chunk::{ChunkView, Direction};
pub use data_buffer::DataBuffer;
pub use encoding::{BinaryFormat, Encoding, HexFormat};
pub use pattern::{PatternMatcher, PatternMode};
pub use search::SearchMatch;
