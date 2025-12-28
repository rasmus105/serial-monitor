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

    /// Number of visible chunks that have been searched
    searched_count: usize,

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
        self.searched_count = 0;
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
        self.searched_count = 0;
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
        filtered_indices: &[usize],
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

        // Determine which chunks to search
        let is_filtered = !filtered_indices.is_empty();

        if is_filtered {
            // Search filtered chunks
            for (visible_idx, &chunk_idx) in filtered_indices.iter().enumerate() {
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

        self.searched_count = if is_filtered {
            filtered_indices.len()
        } else {
            encoded.len()
        };
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

    // -------------------------------------------------------------------------
    // Match access
    // -------------------------------------------------------------------------

    /// Get current match
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current_match.and_then(|idx| self.matches.get(idx))
    }

    /// Get matches in a specific visible chunk
    pub fn matches_in_chunk(&self, visible_index: usize) -> impl Iterator<Item = &SearchMatch> {
        self.matches
            .iter()
            .filter(move |m| m.visible_index == visible_index)
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

        let matches = search.update(&[], &encoded);
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
        let matches = search.update(&filtered_indices, &encoded);

        // Only one match - "hello again" at visible index 1
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].visible_index, 1);
    }
}
