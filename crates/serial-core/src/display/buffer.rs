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
/// use serial_core::display::{DisplayBuffer, Encoding};
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
/// display.set_show_rx(true);
/// display.set_show_tx(false);
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
        if self.filter.is_active() {
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

    /// Set filter pattern
    pub fn set_filter_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.filter.set_pattern(pattern, mode)?;
        self.rebuild_filtered_view();
        self.search.invalidate();
        Ok(())
    }

    /// Set filter pattern mode
    pub fn set_filter_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.filter.set_mode(mode)?;
        self.rebuild_filtered_view();
        self.search.invalidate();
        Ok(())
    }

    /// Clear filter pattern
    pub fn clear_filter_pattern(&mut self) {
        self.filter.clear_pattern();
        self.rebuild_filtered_view();
        self.search.invalidate();
    }

    /// Get filter pattern
    pub fn filter_pattern(&self) -> Option<&str> {
        self.filter.pattern()
    }

    /// Get filter mode
    pub fn filter_mode(&self) -> PatternMode {
        self.filter.mode()
    }

    /// Get filter error
    pub fn filter_error(&self) -> Option<&str> {
        self.filter.error()
    }

    /// Set whether TX chunks are shown
    pub fn set_show_tx(&mut self, show: bool) {
        if self.filter.show_tx() != show {
            self.filter.set_show_tx(show);
            self.rebuild_filtered_view();
            self.search.invalidate();
        }
    }

    /// Set whether RX chunks are shown
    pub fn set_show_rx(&mut self, show: bool) {
        if self.filter.show_rx() != show {
            self.filter.set_show_rx(show);
            self.rebuild_filtered_view();
            self.search.invalidate();
        }
    }

    /// Check if TX chunks are shown
    pub fn show_tx(&self) -> bool {
        self.filter.show_tx()
    }

    /// Check if RX chunks are shown
    pub fn show_rx(&self) -> bool {
        self.filter.show_rx()
    }

    /// Rebuild the filtered view from all_chunks
    fn rebuild_filtered_view(&mut self) {
        self.filtered_chunks.clear();
        if self.filter.is_active() {
            for chunk in &self.all_chunks {
                if self.filter.matches(chunk) {
                    self.filtered_chunks.push_back(Rc::clone(chunk));
                }
            }
        }
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
