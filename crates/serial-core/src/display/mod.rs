//! Display module for serial monitor
//!
//! This module provides a unified pipeline from raw serial data bytes to
//! searchable, filterable display content. It handles:
//!
//! - **Encoding**: Converting raw bytes to display strings (UTF-8, ASCII, Hex, Binary)
//! - **Filtering**: Pattern-based and direction-based (TX/RX) filtering
//! - **Searching**: Finding matches within the filtered view
//! - **Caching**: Maintaining encoded representations for efficient rendering
//!
//! # Architecture
//!
//! The main entry point is [`DisplayBuffer`], which orchestrates the entire pipeline:
//!
//! ```text
//! Raw DataChunks (VecDeque<DataChunk>)
//!          │
//!          ▼ sync()
//! ┌─────────────────────────────────────────────────────┐
//! │                   DisplayBuffer                      │
//! │  ┌───────────────────────────────────────────────┐  │
//! │  │  chunks: VecDeque<DisplayChunk>               │  │
//! │  │  (encoded 1:1 with raw data)                  │  │
//! │  └───────────────────────────────────────────────┘  │
//! │         │                                           │
//! │         ▼ filter                                    │
//! │  ┌───────────────────────────────────────────────┐  │
//! │  │  FilterState                                  │  │
//! │  │  - pattern matching (literal/regex)           │  │
//! │  │  - direction filtering (show_tx/show_rx)      │  │
//! │  │  - visible: Vec<usize>                        │  │
//! │  └───────────────────────────────────────────────┘  │
//! │         │                                           │
//! │         ▼ search (on visible only)                  │
//! │  ┌───────────────────────────────────────────────┐  │
//! │  │  SearchState                                  │  │
//! │  │  - pattern matching                           │  │
//! │  │  - matches: Vec<SearchMatch>                  │  │
//! │  │  - navigation (current, next, prev)           │  │
//! │  └───────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────┘
//!          │
//!          ▼
//!     Frontend renders visible_chunks() with highlighted matches
//! ```
//!
//! # Example
//!
//! ```ignore
//! use serial_core::display::{DisplayBuffer, Encoding, PatternMode};
//!
//! // Create buffer with UTF-8 encoding
//! let mut display = DisplayBuffer::new(Encoding::Utf8);
//!
//! // Sync with raw data buffer (call when raw buffer changes)
//! display.sync(&raw_buffer, 0);
//!
//! // Set up filtering
//! display.set_filter_pattern("error", PatternMode::Normal)?;
//! display.set_show_rx(true);
//! display.set_show_tx(false);
//!
//! // Search within filtered view
//! display.set_search_pattern("timeout", PatternMode::Normal)?;
//!
//! // Get visible chunks for rendering
//! for (idx, chunk) in display.visible_chunks() {
//!     let matches = display.matches_in_chunk(idx);
//!     // render chunk with highlighted matches
//! }
//! ```

mod buffer;
mod chunk;
mod encoding;
mod filter;
mod pattern;
mod search;

// Public exports
pub use buffer::DisplayBuffer;
pub use chunk::DisplayChunk;
pub use encoding::{encode, BinaryFormat, Encoding, HexFormat};
pub use pattern::{PatternMatcher, PatternMode};
pub use search::SearchMatch;
