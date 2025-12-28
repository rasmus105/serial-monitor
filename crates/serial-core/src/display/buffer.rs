//! Display buffer - main orchestrator for display pipeline
//!
//! The `DisplayBuffer` manages the complete pipeline from raw serial data
//! to searchable, filterable display content.

use std::collections::VecDeque;
use std::rc::Rc;

use crate::DataChunk;

use super::chunk::DisplayChunk;
use super::encoding::Encoding;
use super::filter::FilterState;
use super::pattern::PatternMode;
use super::search::{SearchMatch, SearchState};

/// Display buffer for encoded, filterable, searchable serial data
///
/// `DisplayBuffer` is the main entry point for the display module. It:
/// - Maintains encoded representations of raw data chunks
/// - Filters chunks by pattern and direction (TX/RX)
/// - Searches within the current view
/// - Handles incremental updates and buffer truncation
///
/// # Example
///
/// ```ignore
/// use serial_core::display::{DisplayBuffer, Encoding, PatternMode};
///
/// // Create buffer with UTF-8 encoding
/// let mut display = DisplayBuffer {
///     encoding: Encoding::Utf8,
///     ..Default::default()
/// };
///
/// // Sync with raw data buffer
/// display.sync(&raw_buffer, 0);
///
/// // Set up filtering
/// display.set_filter_pattern("error", PatternMode::Normal)?;
/// display.show_rx = true;
/// display.show_tx = false;
///
/// // Search within current view
/// display.set_search_pattern("timeout", PatternMode::Normal)?;
///
/// // Iterate chunks with highlighting info
/// for (idx, chunk) in display.chunks().iter().enumerate() {
///     let matches = display.matches_in_chunk(idx);
///     // render chunk with highlighted matches
/// }
/// ```
#[derive(Debug)]
pub struct DisplayBuffer {
    /// All encoded chunks (source of truth for display)
    all_chunks: VecDeque<Rc<DisplayChunk>>,

    /// Filtered view - Rc clones pointing to same data
    /// Only populated when filter is active
    filtered_chunks: VecDeque<Rc<DisplayChunk>>,

    /// Current encoding setting
    pub encoding: Encoding,

    /// Whether encoding changed and needs full re-encode
    encoding_changed: bool,

    /// Filter state (pattern + direction)
    filter: FilterState,

    /// Show TX (transmitted) chunks
    pub show_tx: bool,

    /// Show RX (received) chunks
    pub show_rx: bool,

    /// Search state
    search: SearchState,

    /// Number of raw chunks we've encoded
    synced_count: usize,
}

impl Default for DisplayBuffer {
    fn default() -> Self {
        Self {
            all_chunks: VecDeque::new(),
            filtered_chunks: VecDeque::new(),
            encoding: Encoding::default(),
            encoding_changed: false,
            filter: FilterState::default(),
            show_tx: true,
            show_rx: true,
            search: SearchState::default(),
            synced_count: 0,
        }
    }
}

impl DisplayBuffer {
    // =========================================================================
    // Encoding
    // =========================================================================

    /// Set encoding
    ///
    /// This marks the buffer for full re-encoding on the next `sync()` call.
    pub fn set_encoding(&mut self, encoding: Encoding) {
        if self.encoding != encoding {
            self.encoding = encoding;
            self.encoding_changed = true;
            // Filter and search will be invalidated during sync when re-encoding happens
        }
    }

    // =========================================================================
    // Data synchronization
    // =========================================================================

    /// Sync with raw data buffer
    ///
    /// Call this when the raw buffer changes (new data arrived, truncation occurred).
    ///
    /// # Arguments
    ///
    /// * `raw_chunks` - The raw data buffer
    /// * `dropped` - Number of chunks dropped from front since last sync
    pub fn sync(&mut self, raw_chunks: &VecDeque<DataChunk>, dropped: usize) {
        todo!("Implement sync")
    }

    // =========================================================================
    // Unified chunk access
    // =========================================================================

    /// Get the current view of chunks
    ///
    /// Returns filtered chunks if a filter is active, otherwise all chunks.
    /// Search and rendering should use this method - they don't need to know
    /// whether filtering is active.
    pub fn chunks(&self) -> &VecDeque<Rc<DisplayChunk>> {
        if self.is_filter_active() {
            &self.filtered_chunks
        } else {
            &self.all_chunks
        }
    }

    /// Get total chunk count in current view
    pub fn len(&self) -> usize {
        self.chunks().len()
    }

    /// Check if current view is empty
    pub fn is_empty(&self) -> bool {
        self.chunks().is_empty()
    }

    // =========================================================================
    // Filtering
    // =========================================================================

    /// Check if any filter is active
    fn is_filter_active(&self) -> bool {
        self.filter.pattern.has_pattern() || !self.show_tx || !self.show_rx
    }

    /// Set filter pattern
    pub fn set_filter_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.filter.pattern.set_pattern(pattern, mode)?;
        self.rebuild_filtered_view();
        self.search.invalidate();
        Ok(())
    }

    /// Set filter pattern mode
    pub fn set_filter_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.filter.pattern.set_mode(mode)?;
        self.rebuild_filtered_view();
        self.search.invalidate();
        Ok(())
    }

    /// Clear filter pattern
    pub fn clear_filter_pattern(&mut self) {
        self.filter.pattern.clear();
        self.rebuild_filtered_view();
        self.search.invalidate();
    }

    /// Get filter pattern
    pub fn filter_pattern(&self) -> Option<&str> {
        self.filter.pattern.pattern()
    }

    /// Get filter mode
    pub fn filter_mode(&self) -> PatternMode {
        self.filter.pattern.mode()
    }

    /// Get filter error
    pub fn filter_error(&self) -> Option<&str> {
        self.filter.pattern.error()
    }

    /// Rebuild the filtered view from all_chunks
    ///
    /// Call this after changing filter settings (pattern, show_tx, show_rx).
    pub fn rebuild_filtered_view(&mut self) {
        self.filtered_chunks.clear();
        if self.is_filter_active() {
            // Sync filter state with our show_tx/show_rx
            self.filter.show_tx = self.show_tx;
            self.filter.show_rx = self.show_rx;

            for chunk in &self.all_chunks {
                if self.filter.matches(chunk) {
                    self.filtered_chunks.push_back(Rc::clone(chunk));
                }
            }
        }
        self.search.invalidate();
    }

    // =========================================================================
    // Searching
    // =========================================================================

    /// Set search pattern
    pub fn set_search_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.search.set_pattern(pattern, mode)
    }

    /// Set search pattern mode
    pub fn set_search_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.search.set_mode(mode)
    }

    /// Clear search
    pub fn clear_search(&mut self) {
        self.search.clear();
    }

    /// Get search pattern
    pub fn search_pattern(&self) -> Option<&str> {
        self.search.pattern()
    }

    /// Get search mode
    pub fn search_mode(&self) -> PatternMode {
        self.search.mode()
    }

    /// Get search error
    pub fn search_error(&self) -> Option<&str> {
        self.search.error()
    }

    /// Get all search matches
    ///
    /// Matches are within the current view (chunks()).
    pub fn matches(&mut self) -> &[SearchMatch] {
        self.search.update(self.chunks())
    }

    /// Get match count
    pub fn match_count(&self) -> usize {
        self.search.match_count()
    }

    /// Go to next match
    ///
    /// Returns the chunk index in `chunks()` of the new current match.
    pub fn goto_next_match(&mut self) -> Option<usize> {
        self.search.goto_next()
    }

    /// Go to previous match
    ///
    /// Returns the chunk index in `chunks()` of the new current match.
    pub fn goto_prev_match(&mut self) -> Option<usize> {
        self.search.goto_prev()
    }

    /// Get current match index
    pub fn current_match_index(&self) -> Option<usize> {
        self.search.current_match_index()
    }

    /// Get current match
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.search.current_match()
    }

    /// Get matches in a specific chunk
    pub fn matches_in_chunk(&self, chunk_index: usize) -> impl Iterator<Item = &SearchMatch> {
        self.search.matches_in_chunk(chunk_index)
    }

    /// Check if a match is the current one
    pub fn is_current_match(&self, m: &SearchMatch) -> bool {
        self.search.is_current_match(m)
    }

    /// Get search status message
    pub fn search_status(&self) -> String {
        self.search.status_message()
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests once sync() is implemented
}
