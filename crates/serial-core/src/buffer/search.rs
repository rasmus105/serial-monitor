//! Search state for finding patterns in buffer content
//!
//! Internal module that manages search matches within the current view,
//! with support for incremental searching and match navigation.

use std::collections::VecDeque;

use super::pattern::{PatternMatcher, PatternMode};

/// A single search match within a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Index in the visible view (respects filtering)
    pub visible_index: usize,
    /// Byte offset where the match starts within the encoded content
    pub byte_start: usize,
    /// Byte offset where the match ends within the encoded content
    pub byte_end: usize,
}

/// Internal search state
///
/// Manages searching within the current visible view. Works with indices
/// to support both filtered and unfiltered views efficiently.
#[derive(Debug, Default)]
pub(crate) struct SearchState {
    /// Pattern matcher for searching
    pub(crate) pattern: PatternMatcher,

    /// All matches found in the current view
    pub(crate) matches: Vec<SearchMatch>,

    /// Current match index for navigation
    pub(crate) current_match: Option<usize>,

    /// Whether search results are valid
    valid: bool,
}

impl SearchState {
    // -------------------------------------------------------------------------
    // Pattern configuration
    // -------------------------------------------------------------------------

    /// Set search pattern
    ///
    /// Invalidates search results, requiring re-search.
    pub fn set_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.pattern.set_pattern(pattern, mode)?;
        self.invalidate();
        Ok(())
    }

    /// Set pattern mode, keeping the same pattern
    pub fn set_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.pattern.set_mode(mode)?;
        self.invalidate();
        Ok(())
    }

    /// Clear the search
    pub fn clear(&mut self) {
        self.pattern.clear();
        self.matches.clear();
        self.current_match = None;
        self.valid = false;
    }

    // -------------------------------------------------------------------------
    // Search execution
    // -------------------------------------------------------------------------

    /// Invalidate search results
    ///
    /// Called when search pattern changes or filter changes.
    pub fn invalidate(&mut self) {
        self.matches.clear();
        self.current_match = None;
        self.valid = false;
    }

    /// Update search with current view
    ///
    /// Searches visible chunks. If `filtered_indices` is empty, searches all
    /// chunks (0..encoded.len()). Otherwise searches only filtered indices.
    ///
    /// Returns the slice of all matches.
    pub fn update(
        &mut self,
        filtered_indices: impl Iterator<Item = usize>,
        is_filtered: bool,
        encoded: &VecDeque<String>,
    ) -> &[SearchMatch] {
        // Already up to date
        if self.valid {
            return &self.matches;
        }

        // No pattern = no matches
        if !self.pattern.has_pattern() {
            self.valid = true;
            return &self.matches;
        }

        self.matches.clear();

        if is_filtered {
            // Search filtered chunks
            for (visible_idx, chunk_idx) in filtered_indices.enumerate() {
                if let Some(content) = encoded.get(chunk_idx) {
                    self.search_chunk(visible_idx, content);
                }
            }
        } else {
            // Search all chunks
            for (visible_idx, content) in encoded.iter().enumerate() {
                self.search_chunk(visible_idx, content);
            }
        }

        self.valid = true;

        &self.matches
    }

    /// Search a single chunk and add matches
    fn search_chunk(&mut self, visible_index: usize, content: &str) {
        for (start, end) in self.pattern.find_matches(content) {
            self.matches.push(SearchMatch {
                visible_index,
                byte_start: start,
                byte_end: end,
            });
        }
    }

    /// Add matches from a single new chunk (incremental update)
    ///
    /// Called when new data arrives that passes the filter. Instead of
    /// invalidating all results, we just search the new chunk and append.
    /// `visible_index` is the index this chunk will have in the visible view.
    pub fn add_chunk(&mut self, visible_index: usize, content: &str) {
        if !self.pattern.has_pattern() || !self.valid {
            return;
        }
        self.search_chunk(visible_index, content);
    }

    /// Called when the oldest chunk is dropped from the buffer.
    ///
    /// Removes matches from chunk 0 and decrements visible_index for all
    /// remaining matches. Also adjusts current_match if needed.
    pub fn drop_oldest_chunk(&mut self) {
        if !self.valid {
            return;
        }

        // Count how many matches were in chunk 0
        let removed_count = self.matches.iter().filter(|m| m.visible_index == 0).count();

        // Remove matches from chunk 0
        self.matches.retain(|m| m.visible_index != 0);

        // Decrement visible_index for all remaining matches
        for m in &mut self.matches {
            m.visible_index -= 1;
        }

        // Adjust current_match index
        if let Some(current) = self.current_match {
            if current < removed_count {
                // Current match was in the dropped chunk
                self.current_match = None;
            } else {
                // Shift the index
                self.current_match = Some(current - removed_count);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Match access
    // -------------------------------------------------------------------------

    /// Get current match
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current_match.and_then(|idx| self.matches.get(idx))
    }

    /// Get matches in a specific visible chunk
    ///
    /// Uses binary search for O(log n) lookup since matches are ordered by `visible_index`.
    pub fn matches_in_chunk(&self, visible_index: usize) -> &[SearchMatch] {
        // Find first match with visible_index >= target
        let start = self
            .matches
            .partition_point(|m| m.visible_index < visible_index);

        // Find end of matches with this visible_index
        let end = self.matches[start..]
            .partition_point(|m| m.visible_index == visible_index)
            + start;

        &self.matches[start..end]
    }

    /// Check if a match is the current one
    pub fn is_current_match(&self, m: &SearchMatch) -> bool {
        self.current_match().is_some_and(|current| current == m)
    }

    // -------------------------------------------------------------------------
    // Navigation
    // -------------------------------------------------------------------------

    /// Go to next match (wrapping)
    ///
    /// Returns the visible index of the new current match.
    pub fn goto_next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        let next_idx = match self.current_match {
            Some(current) => (current + 1) % self.matches.len(),
            None => 0,
        };

        self.current_match = Some(next_idx);
        Some(self.matches[next_idx].visible_index)
    }

    /// Go to previous match (wrapping)
    ///
    /// Returns the visible index of the new current match.
    pub fn goto_prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        let prev_idx = match self.current_match {
            Some(current) => {
                if current == 0 {
                    self.matches.len() - 1
                } else {
                    current - 1
                }
            }
            None => self.matches.len() - 1,
        };

        self.current_match = Some(prev_idx);
        Some(self.matches[prev_idx].visible_index)
    }

    // -------------------------------------------------------------------------
    // Status
    // -------------------------------------------------------------------------

    /// Get status message for display
    pub fn status_message(&self) -> String {
        if let Some(error) = self.pattern.error() {
            return error.to_string();
        }

        if !self.pattern.has_pattern() {
            return String::new();
        }

        let pattern = self.pattern.pattern().unwrap_or("");

        if self.matches.is_empty() {
            return format!("Pattern not found: {}", pattern);
        }

        match self.current_match {
            Some(idx) => format!("Match {}/{}: {}", idx + 1, self.matches.len(), pattern),
            None => format!(
                "Found {} match{}",
                self.matches.len(),
                if self.matches.len() == 1 { "" } else { "es" }
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_matches() {
        let mut search = SearchState::default();
        search.set_pattern("hello", PatternMode::Normal).unwrap();

        let encoded: VecDeque<String> = vec![
            "hello world".to_string(),
            "goodbye".to_string(),
            "hello again".to_string(),
        ]
        .into();

        let matches = search.update(std::iter::empty(), false, &encoded);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].visible_index, 0);
        assert_eq!(matches[1].visible_index, 2);
    }

    #[test]
    fn search_respects_filter() {
        let mut search = SearchState::default();
        search.set_pattern("hello", PatternMode::Normal).unwrap();

        let encoded: VecDeque<String> = vec![
            "hello world".to_string(), // index 0
            "goodbye".to_string(),     // index 1
            "hello again".to_string(), // index 2
        ]
        .into();

        // Only search indices 1 and 2 (filtered view)
        let filtered_indices = vec![1, 2];
        let matches = search.update(filtered_indices.into_iter(), true, &encoded);

        // Only one match - "hello again" at visible index 1
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].visible_index, 1);
    }

    #[test]
    fn incremental_add_chunk() {
        let mut search = SearchState::default();
        search.set_pattern("hello", PatternMode::Normal).unwrap();

        // Initial search
        let encoded: VecDeque<String> = vec![
            "hello world".to_string(),
            "goodbye".to_string(),
        ]
        .into();
        search.update(std::iter::empty(), false, &encoded);
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].visible_index, 0);

        // Navigate to first match
        search.goto_next();
        assert_eq!(search.current_match, Some(0));

        // Add a new chunk with a match
        search.add_chunk(2, "hello again");
        assert_eq!(search.matches.len(), 2);
        assert_eq!(search.matches[1].visible_index, 2);

        // Current match should be preserved
        assert_eq!(search.current_match, Some(0));
    }

    #[test]
    fn incremental_drop_oldest() {
        let mut search = SearchState::default();
        search.set_pattern("hello", PatternMode::Normal).unwrap();

        // Initial search with 3 matches
        let encoded: VecDeque<String> = vec![
            "hello one".to_string(),   // index 0, match at bytes 0-5
            "goodbye".to_string(),     // index 1, no match
            "hello two".to_string(),   // index 2, match at bytes 0-5
            "hello three".to_string(), // index 3, match at bytes 0-5
        ]
        .into();
        search.update(std::iter::empty(), false, &encoded);
        assert_eq!(search.matches.len(), 3);
        assert_eq!(search.matches[0].visible_index, 0);
        assert_eq!(search.matches[1].visible_index, 2);
        assert_eq!(search.matches[2].visible_index, 3);

        // Navigate to second match (index 1 in matches vec)
        search.goto_next(); // match 0
        search.goto_next(); // match 1
        assert_eq!(search.current_match, Some(1));
        assert_eq!(search.matches[1].visible_index, 2);

        // Drop oldest chunk (which had a match)
        search.drop_oldest_chunk();
        
        // Should now have 2 matches with adjusted indices
        assert_eq!(search.matches.len(), 2);
        assert_eq!(search.matches[0].visible_index, 1); // was 2, now 1
        assert_eq!(search.matches[1].visible_index, 2); // was 3, now 2

        // Current match should be adjusted: was at index 1, but index 0 was removed
        // So we lost one match, new current should be 0
        assert_eq!(search.current_match, Some(0));
    }

    #[test]
    fn drop_oldest_removes_current_match() {
        let mut search = SearchState::default();
        search.set_pattern("hello", PatternMode::Normal).unwrap();

        let encoded: VecDeque<String> = vec![
            "hello one".to_string(),
            "hello two".to_string(),
        ]
        .into();
        search.update(std::iter::empty(), false, &encoded);
        
        // Navigate to first match
        search.goto_next();
        assert_eq!(search.current_match, Some(0));
        assert_eq!(search.matches[0].visible_index, 0);

        // Drop oldest - this removes the current match
        search.drop_oldest_chunk();
        
        // Current match should be None since it was in the dropped chunk
        assert_eq!(search.current_match, None);
        // One match remaining
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].visible_index, 0); // was 1, now 0
    }

    #[test]
    fn navigation_preserves_through_new_data() {
        let mut search = SearchState::default();
        search.set_pattern("test", PatternMode::Normal).unwrap();

        // Initial 3 chunks with matches
        let encoded: VecDeque<String> = vec![
            "test 1".to_string(),
            "test 2".to_string(),
            "test 3".to_string(),
        ]
        .into();
        search.update(std::iter::empty(), false, &encoded);
        assert_eq!(search.matches.len(), 3);

        // Navigate to match 2 (index 1)
        search.goto_next(); // match 0
        search.goto_next(); // match 1
        assert_eq!(search.current_match, Some(1));

        // Add new chunk with match
        search.add_chunk(3, "test 4");
        
        // Navigation state preserved
        assert_eq!(search.current_match, Some(1));
        assert_eq!(search.matches.len(), 4);

        // Can continue navigating
        search.goto_next();
        assert_eq!(search.current_match, Some(2));
        search.goto_next();
        assert_eq!(search.current_match, Some(3)); // New match!
        search.goto_next(); // Wraps to beginning
        assert_eq!(search.current_match, Some(0));
    }

    #[test]
    fn matches_in_chunk_binary_search() {
        let mut search = SearchState::default();
        search.set_pattern("x", PatternMode::Normal).unwrap();

        // Create chunks with varying numbers of matches
        let encoded: VecDeque<String> = vec![
            "x x x".to_string(),    // chunk 0: 3 matches
            "no match".to_string(), // chunk 1: 0 matches
            "x".to_string(),        // chunk 2: 1 match
            "x x".to_string(),      // chunk 3: 2 matches
        ]
        .into();
        search.update(std::iter::empty(), false, &encoded);

        // Verify total matches
        assert_eq!(search.matches.len(), 6);

        // Test binary search retrieval
        assert_eq!(search.matches_in_chunk(0).len(), 3);
        assert_eq!(search.matches_in_chunk(1).len(), 0);
        assert_eq!(search.matches_in_chunk(2).len(), 1);
        assert_eq!(search.matches_in_chunk(3).len(), 2);

        // Non-existent chunk
        assert_eq!(search.matches_in_chunk(99).len(), 0);
    }
}
