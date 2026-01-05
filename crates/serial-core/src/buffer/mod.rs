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

pub(crate) mod chunk;
mod encoding;
pub(crate) mod file_saver;
pub mod graph;
mod pattern;
mod search;

// Public exports
pub use chunk::{ChunkView, Direction};
pub use encoding::{encode, encode_ascii, encode_binary, encode_hex, encode_utf8};
pub use encoding::{BinaryFormat, Encoding, HexFormat};
pub use file_saver::{
    default_cache_directory, AutoSaveConfig, DirectionFilter, SaveFormat, SaveScope, UserSaveConfig,
};
pub use pattern::{PatternMatcher, PatternMode};
pub use search::SearchMatch;

// Internal imports for DataBuffer
use std::collections::VecDeque;
use std::path::Path;
use std::time::SystemTime;

use chunk::RawChunk;
use file_saver::FileSaverHandle;
use graph::GraphEngine;
use search::SearchState;

/// Default maximum buffer size (10 MB)
const DEFAULT_MAX_SIZE: usize = 10 * 1024 * 1024;

/// Central data buffer for serial monitor
///
/// Manages raw data storage, encoding, filtering, searching, and optional
/// graph processing. Frontends interact with this through [`chunks()`](Self::chunks)
/// which returns an iterator of [`ChunkView`] - they never see raw bytes.
///
/// # Example
///
/// ```ignore
/// use serial_core::buffer::{DataBuffer, Direction};
///
/// let mut buffer = DataBuffer::default();
///
/// // Data arrives (typically called by Session I/O task)
/// buffer.push(b"Hello".to_vec(), Direction::Rx);
///
/// // Frontend iterates chunks
/// for chunk in buffer.chunks() {
///     println!("{}: {}", chunk.direction, chunk.encoded);
/// }
/// ```
#[derive(Debug, bon::Builder)]
pub struct DataBuffer {
    /// Raw chunks - source of truth (hidden from frontends)
    #[builder(skip)]
    raw_chunks: VecDeque<RawChunk>,

    /// Encoded strings - 1:1 with raw_chunks
    #[builder(skip)]
    encoded: VecDeque<String>,

    /// Current encoding setting
    #[builder(default)]
    pub encoding: Encoding,

    /// Indices into raw_chunks/encoded that pass the filter
    #[builder(skip)]
    filtered_indices: Vec<usize>,

    /// Filter pattern matcher
    #[builder(skip)]
    filter: PatternMatcher,

    /// Show TX chunks
    #[builder(default = true)]
    pub show_tx: bool,

    /// Show RX chunks
    #[builder(default = true)]
    pub show_rx: bool,

    /// Search state
    #[builder(skip)]
    search: SearchState,

    /// Current total size in bytes (raw data)
    #[builder(skip)]
    current_size: usize,

    /// Maximum size in bytes
    #[builder(default = DEFAULT_MAX_SIZE)]
    pub max_size: usize,

    /// Graph engine (lazy initialized)
    #[builder(skip)]
    graph: Option<GraphEngine>,

    /// File saver handle (when saving is active)
    #[builder(skip)]
    file_saver: Option<FileSaverHandle>,
}

impl Default for DataBuffer {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl DataBuffer {
    // =========================================================================
    // Data input
    // =========================================================================

    /// Push new data into the buffer
    ///
    /// This is called by the Session I/O task when data arrives.
    /// Handles encoding, filtering, size management, and graph feeding.
    pub fn push(&mut self, data: Vec<u8>, direction: Direction) {
        let timestamp = SystemTime::now();
        let size = data.len();

        // Encode the data
        let encoded_str = encode(&data, self.encoding);

        // Create raw chunk
        let raw = RawChunk {
            data,
            direction,
            timestamp,
        };

        // Save to file if active (before adding to buffer to access raw data)
        if let Some(ref saver) = self.file_saver {
            // Ignore errors - file saving is best-effort
            let _ = saver.write(&raw);
        }

        // Check if passes filter before adding
        let passes_filter = self.chunk_passes_filter(&raw, &encoded_str);

        // Add to storage
        let chunk_index = self.raw_chunks.len();
        self.raw_chunks.push_back(raw);
        self.encoded.push_back(encoded_str);
        self.current_size += size;

        // Update filtered indices and search
        if passes_filter {
            // Calculate visible index for search (position in filtered view)
            let visible_index = if self.is_filter_active() {
                self.filtered_indices.len() // Will be the index after we push
            } else {
                chunk_index // Same as raw index when no filter
            };

            self.filtered_indices.push(chunk_index);

            // Incrementally add matches from this chunk instead of invalidating
            self.search
                .add_chunk(visible_index, self.encoded.back().unwrap());
        }

        // Feed to graph if enabled
        if let Some(ref mut graph) = self.graph {
            graph.process_raw_chunk(self.raw_chunks.back().unwrap());
        }

        // Truncate if over size limit
        self.truncate_if_needed();
    }

    /// Truncate oldest chunks if over size limit
    fn truncate_if_needed(&mut self) {
        while self.current_size > self.max_size && !self.raw_chunks.is_empty() {
            self.drop_oldest();
        }
    }

    /// Drop the oldest chunk
    fn drop_oldest(&mut self) {
        if let Some(raw) = self.raw_chunks.pop_front() {
            self.current_size -= raw.data.len();
            self.encoded.pop_front();

            // Check if the dropped chunk was in the filtered view
            let was_in_filtered_view = if let Some(first) = self.filtered_indices.first() {
                *first == 0
            } else {
                false
            };

            // Adjust filtered indices
            // Remove index 0 if present, then subtract 1 from all remaining
            if was_in_filtered_view {
                self.filtered_indices.remove(0);
            }
            for idx in &mut self.filtered_indices {
                *idx -= 1;
            }

            // Update search: if the dropped chunk was visible, adjust match indices
            if was_in_filtered_view || !self.is_filter_active() {
                self.search.drop_oldest_chunk();
            }

            // Trim graph data to keep it in sync with buffer's time window
            if let Some(ref mut graph) = self.graph {
                if let Some(oldest) = self.raw_chunks.front() {
                    graph.trim_before(oldest.timestamp);
                } else {
                    // Buffer is now empty, clear the graph
                    graph.clear();
                }
            }
        }
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.raw_chunks.clear();
        self.encoded.clear();
        self.filtered_indices.clear();
        self.current_size = 0;
        self.search.invalidate();
        if let Some(ref mut graph) = self.graph {
            graph.clear();
        }
    }

    // =========================================================================
    // Chunk access
    // =========================================================================

    /// Iterate visible chunks
    ///
    /// Returns filtered chunks if filter is active, otherwise all chunks.
    /// This is the main method frontends use to get displayable data.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkView<'_>> {
        let indices: Box<dyn Iterator<Item = usize>> = if self.is_filter_active() {
            Box::new(self.filtered_indices.iter().copied())
        } else {
            Box::new(0..self.raw_chunks.len())
        };

        indices.map(move |i| ChunkView {
            encoded: &self.encoded[i],
            direction: self.raw_chunks[i].direction,
            timestamp: self.raw_chunks[i].timestamp,
        })
    }

    /// Get chunk count (respects filtering)
    pub fn len(&self) -> usize {
        if self.is_filter_active() {
            self.filtered_indices.len()
        } else {
            self.raw_chunks.len()
        }
    }

    /// Check if empty (respects filtering)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get chunk by visible index (respects filtering)
    pub fn get(&self, visible_index: usize) -> Option<ChunkView<'_>> {
        let chunk_index = if self.is_filter_active() {
            *self.filtered_indices.get(visible_index)?
        } else {
            if visible_index >= self.raw_chunks.len() {
                return None;
            }
            visible_index
        };

        Some(ChunkView {
            encoded: &self.encoded[chunk_index],
            direction: self.raw_chunks[chunk_index].direction,
            timestamp: self.raw_chunks[chunk_index].timestamp,
        })
    }

    /// Get total chunk count (ignoring filter)
    pub fn total_len(&self) -> usize {
        self.raw_chunks.len()
    }

    /// Get current buffer size in bytes
    pub fn size(&self) -> usize {
        self.current_size
    }

    // =========================================================================
    // Encoding
    // =========================================================================

    /// Set encoding
    ///
    /// Re-encodes all existing chunks and rebuilds filter.
    pub fn set_encoding(&mut self, encoding: Encoding) {
        if self.encoding != encoding {
            self.encoding = encoding;
            self.reencode_all();
        }
    }

    /// Re-encode all chunks with current encoding
    fn reencode_all(&mut self) {
        self.encoded.clear();
        for raw in &self.raw_chunks {
            self.encoded.push_back(encode(&raw.data, self.encoding));
        }
        self.rebuild_filter();
    }

    // =========================================================================
    // Filtering
    // =========================================================================

    /// Check if any filter is active
    fn is_filter_active(&self) -> bool {
        self.filter.has_pattern() || !self.show_tx || !self.show_rx
    }

    /// Check if a chunk passes the current filter
    fn chunk_passes_filter(&self, raw: &RawChunk, encoded: &str) -> bool {
        // Direction check
        match raw.direction {
            Direction::Tx if !self.show_tx => return false,
            Direction::Rx if !self.show_rx => return false,
            _ => {}
        }

        // Pattern check
        self.filter.is_match(encoded)
    }

    /// Set filter pattern
    pub fn set_filter_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.filter.set_pattern(pattern, mode)?;
        self.rebuild_filter();
        Ok(())
    }

    /// Set filter mode
    pub fn set_filter_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.filter.set_mode(mode)?;
        self.rebuild_filter();
        Ok(())
    }

    /// Clear filter pattern
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.rebuild_filter();
    }

    /// Get filter pattern
    pub fn filter_pattern(&self) -> Option<&str> {
        self.filter.pattern()
    }

    /// Get filter mode
    pub fn filter_mode(&self) -> PatternMode {
        self.filter.mode
    }

    /// Get filter error
    pub fn filter_error(&self) -> Option<&str> {
        self.filter.error()
    }

    /// Rebuild filtered indices from scratch
    fn rebuild_filter(&mut self) {
        self.filtered_indices.clear();

        if self.is_filter_active() {
            for (i, raw) in self.raw_chunks.iter().enumerate() {
                if self.chunk_passes_filter(raw, &self.encoded[i]) {
                    self.filtered_indices.push(i);
                }
            }
        }

        self.search.invalidate();
    }

    /// Set show_tx and rebuild filter if needed
    pub fn set_show_tx(&mut self, show: bool) {
        if self.show_tx != show {
            self.show_tx = show;
            self.rebuild_filter();
        }
    }

    /// Set show_rx and rebuild filter if needed
    pub fn set_show_rx(&mut self, show: bool) {
        if self.show_rx != show {
            self.show_rx = show;
            self.rebuild_filter();
        }
    }

    // =========================================================================
    // Searching
    // =========================================================================

    /// Set search pattern
    pub fn set_search_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.search.set_pattern(pattern, mode)
    }

    /// Set search mode
    pub fn set_search_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.search.set_mode(mode)
    }

    /// Clear search
    pub fn clear_search(&mut self) {
        self.search.clear();
    }

    /// Get search pattern
    pub fn search_pattern(&self) -> Option<&str> {
        self.search.pattern.pattern()
    }

    /// Get search mode
    pub fn search_mode(&self) -> PatternMode {
        self.search.pattern.mode
    }

    /// Get search error
    pub fn search_error(&self) -> Option<&str> {
        self.search.pattern.error()
    }

    /// Get all search matches (updates search if needed)
    pub fn matches(&mut self) -> &[SearchMatch] {
        self.ensure_search_updated();
        &self.search.matches
    }

    /// Go to next match
    pub fn goto_next_match(&mut self) -> Option<usize> {
        // Ensure matches are up-to-date before navigating
        self.ensure_search_updated();
        self.search.goto_next()
    }

    /// Go to previous match
    pub fn goto_prev_match(&mut self) -> Option<usize> {
        // Ensure matches are up-to-date before navigating
        self.ensure_search_updated();
        self.search.goto_prev()
    }

    /// Ensure search results are up-to-date (internal helper)
    fn ensure_search_updated(&mut self) {
        let indices: &[usize] = if self.is_filter_active() {
            &self.filtered_indices
        } else {
            &[]
        };
        let encoded = &self.encoded;
        self.search.update(indices, encoded);
    }

    /// Get current match index
    pub fn current_match_index(&self) -> Option<usize> {
        self.search.current_match
    }

    /// Get current match
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.search.current_match()
    }

    /// Get matches in a specific visible chunk
    ///
    /// Uses binary search for O(log n) lookup.
    pub fn matches_in_chunk(&self, visible_index: usize) -> &[SearchMatch] {
        self.search.matches_in_chunk(visible_index)
    }

    /// Check if a match is the current one
    pub fn is_current_match(&self, m: &SearchMatch) -> bool {
        self.search.is_current_match(m)
    }

    /// Get search status message
    pub fn search_status(&self) -> String {
        self.search.status_message()
    }

    // =========================================================================
    // Graph engine
    // =========================================================================

    /// Enable graph engine (lazy initialization)
    pub fn enable_graph(&mut self) {
        if self.graph.is_none() {
            let mut engine = GraphEngine::default();
            // Process all existing chunks (preserving direction and timestamp)
            for raw in &self.raw_chunks {
                engine.process_raw_chunk(raw);
            }
            self.graph = Some(engine);
        }
    }

    /// Enable graph engine with a specific parser type.
    pub fn enable_graph_with_parser(&mut self, parser: graph::GraphParserType) {
        if self.graph.is_none() {
            let mut engine = GraphEngine::from_parser(parser);
            for raw in &self.raw_chunks {
                engine.process_raw_chunk(raw);
            }
            self.graph = Some(engine);
        }
    }

    /// Disable graph engine
    pub fn disable_graph(&mut self) {
        self.graph = None;
    }

    /// Set a new parser for the graph engine and re-process all data.
    ///
    /// This clears existing parsed series and re-processes all raw chunks
    /// with the new parser. If the graph is not enabled, this does nothing.
    pub fn set_graph_parser(&mut self, parser: graph::GraphParserType) {
        if let Some(engine) = &mut self.graph {
            engine.set_parser(parser);
            // Re-process all raw chunks with the new parser
            for raw in &self.raw_chunks {
                engine.process_raw_chunk(raw);
            }
        }
    }

    /// Set which directions to parse for graphing.
    ///
    /// This clears existing parsed series and re-processes all raw chunks
    /// with the new direction settings. Packet rate data is preserved.
    pub fn set_graph_parse_directions(&mut self, parse_rx: bool, parse_tx: bool) {
        if let Some(engine) = &mut self.graph {
            // Only reparse if something changed
            if engine.parse_rx == parse_rx && engine.parse_tx == parse_tx {
                return;
            }

            engine.parse_rx = parse_rx;
            engine.parse_tx = parse_tx;

            // Clear parsed series data (but keep packet rate)
            engine.series.clear();
            engine.chunks_processed = 0;

            // Re-process all raw chunks with new direction settings
            for raw in &self.raw_chunks {
                engine.process_raw_chunk(raw);
            }
        }
    }

    /// Get graph engine reference
    pub fn graph(&self) -> Option<&GraphEngine> {
        self.graph.as_ref()
    }

    /// Get mutable graph engine reference
    pub fn graph_mut(&mut self) -> Option<&mut GraphEngine> {
        self.graph.as_mut()
    }

    /// Check if graph is enabled
    pub fn graph_enabled(&self) -> bool {
        self.graph.is_some()
    }

    // =========================================================================
    // File saving
    // =========================================================================

    /// Save data to a file with configurable scope and format.
    ///
    /// # Scope behaviors:
    ///
    /// - [`SaveScope::ExistingOnly`]: Writes current buffer contents and returns immediately.
    ///   No streaming - this is a one-off snapshot.
    ///
    /// - [`SaveScope::NewOnly`]: Starts streaming new data to the file. Call [`stop_saving()`]
    ///   to stop. Does NOT include existing buffer contents.
    ///
    /// - [`SaveScope::ExistingAndContinue`]: Writes current buffer contents, then continues
    ///   streaming new data. Call [`stop_saving()`] to stop.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use serial_core::buffer::{UserSaveConfig, SaveScope, SaveFormat};
    ///
    /// // Snapshot existing buffer
    /// buffer.save(
    ///     UserSaveConfig::builder()
    ///         .path("/tmp/capture.txt")
    ///         .scope(SaveScope::ExistingOnly)
    ///         .build(),
    ///     &runtime,
    /// )?;
    ///
    /// // Stream new data
    /// buffer.save(
    ///     UserSaveConfig::builder()
    ///         .path("/tmp/stream.txt")
    ///         .scope(SaveScope::NewOnly)
    ///         .build(),
    ///     &runtime,
    /// )?;
    /// // ... later ...
    /// buffer.stop_saving();
    /// ```
    pub fn save(
        &mut self,
        config: UserSaveConfig,
        runtime: &tokio::runtime::Handle,
    ) -> crate::Result<()> {
        // Stop any existing user save first
        self.stop_saving();

        match config.scope {
            SaveScope::ExistingOnly => {
                // One-off save, no streaming handle needed
                file_saver::save_existing_to_file(&self.raw_chunks, &config)?;
            }
            SaveScope::NewOnly => {
                // Start streaming without existing data
                let handle = file_saver::start_streaming_saver(&config, None, runtime)?;
                self.file_saver = Some(handle);
            }
            SaveScope::ExistingAndContinue => {
                // Write existing, then stream
                let handle =
                    file_saver::start_streaming_saver(&config, Some(&self.raw_chunks), runtime)?;
                self.file_saver = Some(handle);
            }
        }

        Ok(())
    }

    /// Stop an active streaming save (NewOnly or ExistingAndContinue).
    ///
    /// Does nothing if no streaming save is active.
    pub fn stop_saving(&mut self) {
        if let Some(saver) = self.file_saver.take() {
            let _ = saver.stop();
        }
    }

    /// Check if currently streaming to a user-specified file.
    pub fn is_saving(&self) -> bool {
        self.file_saver.is_some()
    }

    /// Get the file path being saved to (if streaming).
    pub fn save_path(&self) -> Option<&Path> {
        self.file_saver.as_ref().map(|s| s.file_path())
    }

    /// Get the save format being used (if streaming).
    pub fn save_format(&self) -> Option<&SaveFormat> {
        self.file_saver.as_ref().map(|s| s.format())
    }
}
