//! Search state for finding patterns in display content
//!
//! Internal module that manages search matches within the current view,
//! with support for incremental searching and match navigation.

use std::collections::VecDeque;
use std::rc::Rc;

use super::chunk::DisplayChunk;
use super::pattern::{PatternMatcher, PatternMode};

/// A single search match within a display chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Index of the chunk in the current view (chunks())
    pub chunk_index: usize,
    /// Byte offset where the match starts within the chunk content
    pub byte_start: usize,
    /// Byte offset where the match ends within the chunk content
    pub byte_end: usize,
}

/// Internal search state
///
/// Manages searching within the current chunk view, with incremental updates
/// and match navigation. Operates on whatever `chunks()` returns - it doesn't
/// know or care whether filtering is active.
#[derive(Debug, Default)]
pub(crate) struct SearchState {
    /// Pattern matcher for searching
    pattern: PatternMatcher,

    /// All matches found in the current view
    matches: Vec<SearchMatch>,

    /// Current match index for navigation
    current_match: Option<usize>,

    /// Number of chunks that have been searched
    searched_count: usize,
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
    }

    /// Check if a search pattern is set
    pub fn has_pattern(&self) -> bool {
        self.pattern.has_pattern()
    }

    /// Get the current pattern string
    pub fn pattern(&self) -> Option<&str> {
        self.pattern.pattern()
    }

    /// Get the current pattern mode
    pub fn mode(&self) -> PatternMode {
        self.pattern.mode()
    }

    /// Get pattern error if any
    pub fn error(&self) -> Option<&str> {
        self.pattern.error()
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
    }

    /// Update search with current chunk view
    ///
    /// Searches new chunks incrementally. The chunks parameter is the
    /// unified view from DisplayBuffer::chunks().
    ///
    /// Returns the slice of all matches.
    pub fn update(&mut self, chunks: &VecDeque<Rc<DisplayChunk>>) -> &[SearchMatch] {
        todo!("Implement incremental search")
    }

    // -------------------------------------------------------------------------
    // Match access
    // -------------------------------------------------------------------------

    /// Get all matches
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// Get match count
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get current match index
    pub fn current_match_index(&self) -> Option<usize> {
        self.current_match
    }

    /// Get current match
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current_match.and_then(|idx| self.matches.get(idx))
    }

    /// Get matches in a specific chunk
    pub fn matches_in_chunk(&self, chunk_index: usize) -> impl Iterator<Item = &SearchMatch> {
        self.matches
            .iter()
            .filter(move |m| m.chunk_index == chunk_index)
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
    /// Returns the chunk index of the new current match.
    pub fn goto_next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        let next_idx = match self.current_match {
            Some(current) => (current + 1) % self.matches.len(),
            None => 0,
        };

        self.current_match = Some(next_idx);
        Some(self.matches[next_idx].chunk_index)
    }

    /// Go to previous match (wrapping)
    ///
    /// Returns the chunk index of the new current match.
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
        Some(self.matches[prev_idx].chunk_index)
    }

    // -------------------------------------------------------------------------
    // Status
    // -------------------------------------------------------------------------

    /// Get status message for display
    pub fn status_message(&self) -> String {
        if let Some(error) = self.error() {
            return error.to_string();
        }

        if !self.has_pattern() {
            return String::new();
        }

        let pattern = self.pattern().unwrap_or("");

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
    fn test_set_pattern_invalidates() {
        let mut search = SearchState::default();
        search.searched_count = 10;
        search.matches.push(SearchMatch {
            chunk_index: 0,
            byte_start: 0,
            byte_end: 4,
        });

        search.set_pattern("test", PatternMode::Normal).unwrap();

        assert_eq!(search.searched_count, 0);
        assert!(search.matches.is_empty());
    }

    #[test]
    fn test_navigation_empty() {
        let mut search = SearchState::default();
        assert!(search.goto_next().is_none());
        assert!(search.goto_prev().is_none());
    }

    // TODO: Add more tests once update() is implemented
}
