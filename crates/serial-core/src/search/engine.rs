//! Search engine for encoded data
//!
//! Provides efficient search functionality with:
//! - Incremental search (only search new chunks)
//! - Match tracking with chunk indices and byte ranges
//! - Navigation helpers (next/prev match)

use super::pattern::{PatternMatcher, PatternMode};

/// A single search match occurrence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Index of the chunk containing this match
    pub chunk_index: usize,
    /// Byte offset where the match starts within the encoded content
    pub byte_start: usize,
    /// Byte offset where the match ends within the encoded content
    pub byte_end: usize,
}

/// Search engine for finding patterns in encoded chunk data
///
/// The `SearchEngine` provides efficient search with:
/// - Pattern caching (regex compiled once)
/// - Incremental search (automatically tracks which chunks have been searched)
/// - Match navigation (current match, next/prev)
///
/// # Design
///
/// The search engine operates on **encoded strings**, not raw bytes.
/// The frontend is responsible for encoding chunks and storing them.
/// This keeps encoding logic in the frontend while search/matching logic is shared.
///
/// # Example
///
/// ```
/// use serial_core::search::{SearchEngine, PatternMode};
///
/// let mut engine = SearchEngine::new();
///
/// // Set search pattern (case-sensitive in Normal mode)
/// engine.set_pattern("ERROR", PatternMode::Normal).unwrap();
///
/// // Search chunks (frontend provides pre-encoded strings)
/// let chunks = vec![
///     "INFO: Starting up".to_string(),
///     "ERROR: Connection failed".to_string(),
///     "INFO: Retrying".to_string(),
///     "ERROR: Timeout".to_string(),
/// ];
///
/// // Search using accessor closure - only new chunks are searched
/// let matches = engine.search(chunks.len(), |i| &chunks[i]);
/// assert_eq!(matches.len(), 2);
///
/// // Navigate matches
/// assert_eq!(engine.current_match_index(), None);
/// engine.goto_next_match();
/// assert_eq!(engine.current_match_index(), Some(0));
/// engine.goto_next_match();
/// assert_eq!(engine.current_match_index(), Some(1));
/// ```
#[derive(Debug, Default)]
pub struct SearchEngine {
    /// Pattern matcher with caching
    matcher: PatternMatcher,
    /// All matches found
    matches: Vec<SearchMatch>,
    /// Current match index (for navigation)
    current_match: Option<usize>,
    /// Number of chunks that have been searched (for incremental search)
    searched_chunk_count: usize,
}

impl SearchEngine {
    /// Create a new search engine
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the search pattern
    ///
    /// This clears all existing matches and resets the searched chunk count,
    /// so the next `search()` call will search all chunks.
    pub fn set_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.matcher.set_pattern(pattern, mode)?;
        self.matches.clear();
        self.current_match = None;
        self.searched_chunk_count = 0;
        Ok(())
    }

    /// Update the pattern mode, keeping the same pattern string
    ///
    /// This triggers a full re-search on the next `search()` call.
    pub fn set_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.matcher.set_mode(mode)?;
        self.matches.clear();
        self.current_match = None;
        self.searched_chunk_count = 0;
        Ok(())
    }

    /// Clear the search pattern and all matches
    pub fn clear(&mut self) {
        self.matcher.clear();
        self.matches.clear();
        self.current_match = None;
        self.searched_chunk_count = 0;
    }

    /// Check if a pattern is set
    pub fn has_pattern(&self) -> bool {
        self.matcher.has_pattern()
    }

    /// Get the current pattern string
    pub fn pattern(&self) -> Option<&str> {
        self.matcher.pattern()
    }

    /// Get the current mode
    pub fn mode(&self) -> PatternMode {
        self.matcher.mode()
    }

    /// Get any error from pattern compilation
    pub fn error(&self) -> Option<&str> {
        self.matcher.error()
    }

    /// Search chunks for the current pattern
    ///
    /// This automatically handles incremental search - only chunks that haven't
    /// been searched yet will be processed. Call `invalidate()` to force a
    /// full re-search.
    ///
    /// * `total_chunks` - Total number of chunks available
    /// * `get_chunk` - Accessor closure to get the encoded string for a chunk index
    ///
    /// Returns slice of all matches found (including previously found matches)
    ///
    /// # Example
    ///
    /// ```
    /// use serial_core::search::{SearchEngine, PatternMode};
    /// use std::collections::VecDeque;
    ///
    /// let mut engine = SearchEngine::new();
    /// engine.set_pattern("test", PatternMode::Normal).unwrap();
    ///
    /// let chunks: VecDeque<String> = ["test 1", "no match", "test 2"]
    ///     .iter()
    ///     .map(|s| s.to_string())
    ///     .collect();
    ///
    /// let matches = engine.search(chunks.len(), |i| &chunks[i]);
    /// assert_eq!(matches.len(), 2);
    /// ```
    pub fn search<'a, F>(&mut self, total_chunks: usize, get_chunk: F) -> &[SearchMatch]
    where
        F: Fn(usize) -> &'a str,
    {
        if !self.matcher.has_pattern() {
            return &self.matches;
        }

        // Only search chunks we haven't seen yet
        for chunk_idx in self.searched_chunk_count..total_chunks {
            let encoded = get_chunk(chunk_idx);
            let chunk_matches = self.matcher.find_matches(encoded);
            for (byte_start, byte_end) in chunk_matches {
                self.matches.push(SearchMatch {
                    chunk_index: chunk_idx,
                    byte_start,
                    byte_end,
                });
            }
        }

        self.searched_chunk_count = total_chunks;
        &self.matches
    }

    /// Get all matches
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// Get the number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get the current match index
    pub fn current_match_index(&self) -> Option<usize> {
        self.current_match
    }

    /// Get the current match (if any)
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current_match.and_then(|idx| self.matches.get(idx))
    }

    /// Handle buffer truncation when old chunks are dropped
    ///
    /// Call this when the buffer drops old chunks to keep match indices valid.
    /// `dropped_count` is the number of chunks that were dropped from the front.
    pub fn handle_buffer_truncation(&mut self, dropped_count: usize) {
        if dropped_count == 0 {
            return;
        }

        // Remove matches from dropped chunks and adjust indices
        self.matches.retain_mut(|m| {
            if m.chunk_index < dropped_count {
                false // Remove matches from dropped chunks
            } else {
                m.chunk_index -= dropped_count;
                true
            }
        });

        // Adjust current match index
        if let Some(current) = self.current_match {
            if self.matches.is_empty() {
                self.current_match = None;
            } else if current >= self.matches.len() {
                // Current match was dropped, clamp to last
                self.current_match = Some(self.matches.len() - 1);
            }
        }

        // Adjust searched count
        self.searched_chunk_count = self.searched_chunk_count.saturating_sub(dropped_count);
    }

    /// Invalidate search results (e.g., when encoding changes)
    ///
    /// Keeps the pattern but clears matches, requiring a full re-search
    /// on the next `search()` call.
    pub fn invalidate(&mut self) {
        self.matches.clear();
        self.current_match = None;
        self.searched_chunk_count = 0;
    }

    /// Navigate to the next match (wrapping)
    ///
    /// Returns the chunk index of the new current match, if any.
    pub fn goto_next_match(&mut self) -> Option<usize> {
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

    /// Navigate to the previous match (wrapping)
    ///
    /// Returns the chunk index of the new current match, if any.
    pub fn goto_prev_match(&mut self) -> Option<usize> {
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

    /// Get matches for a specific chunk
    ///
    /// Useful for rendering highlights - returns only matches within the given chunk.
    pub fn matches_for_chunk(&self, chunk_index: usize) -> impl Iterator<Item = &SearchMatch> {
        self.matches
            .iter()
            .filter(move |m| m.chunk_index == chunk_index)
    }

    /// Check if a specific match is the current one
    pub fn is_current_match(&self, match_ref: &SearchMatch) -> bool {
        self.current_match()
            .is_some_and(|current| current == match_ref)
    }

    /// Get a status message describing the current search state
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
    use std::collections::VecDeque;

    fn sample_chunks() -> Vec<String> {
        vec![
            "Hello World".to_string(),
            "Error: Connection failed".to_string(),
            "Info: Retrying".to_string(),
            "Error: Timeout".to_string(),
        ]
    }

    #[test]
    fn test_search_literal() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("Error", PatternMode::Normal).unwrap();

        let chunks = sample_chunks();
        let matches = engine.search(chunks.len(), |i| &chunks[i]);

        assert_eq!(matches.len(), 2);
        assert_eq!(engine.matches()[0].chunk_index, 1);
        assert_eq!(engine.matches()[1].chunk_index, 3);
    }

    #[test]
    fn test_search_regex() {
        let mut engine = SearchEngine::default();
        engine
            .set_pattern(r"Error:\s+\w+", PatternMode::Regex)
            .unwrap();

        let chunks = sample_chunks();
        let matches = engine.search(chunks.len(), |i| &chunks[i]);

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_with_vecdeque() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("test", PatternMode::Normal).unwrap();

        let chunks: VecDeque<String> = ["test one", "no match", "test two"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let matches = engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_incremental_search() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("test", PatternMode::Normal).unwrap();

        // Initial search with 2 chunks
        let chunks = vec!["test one".to_string(), "no match".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 1);

        // Add more chunks and search again - only new chunks are searched
        let all_chunks = vec![
            "test one".to_string(),
            "no match".to_string(),
            "test two".to_string(),
            "test three".to_string(),
        ];

        let matches = engine.search(all_chunks.len(), |i| &all_chunks[i]);
        assert_eq!(matches.len(), 3); // 1 original + 2 new
    }

    #[test]
    fn test_invalidate_forces_full_research() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("test", PatternMode::Normal).unwrap();

        let chunks = vec!["test".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 1);

        // Invalidate and search again
        engine.invalidate();
        assert_eq!(engine.match_count(), 0);
        assert!(engine.has_pattern()); // Pattern preserved

        // Search again - should find the match
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 1);
    }

    #[test]
    fn test_navigation() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("Error", PatternMode::Normal).unwrap();

        let chunks = sample_chunks();
        engine.search(chunks.len(), |i| &chunks[i]);

        assert_eq!(engine.current_match_index(), None);

        // First next goes to match 0
        let chunk = engine.goto_next_match();
        assert_eq!(chunk, Some(1));
        assert_eq!(engine.current_match_index(), Some(0));

        // Second next goes to match 1
        let chunk = engine.goto_next_match();
        assert_eq!(chunk, Some(3));
        assert_eq!(engine.current_match_index(), Some(1));

        // Third next wraps to match 0
        let chunk = engine.goto_next_match();
        assert_eq!(chunk, Some(1));
        assert_eq!(engine.current_match_index(), Some(0));

        // Prev wraps to last match
        let chunk = engine.goto_prev_match();
        assert_eq!(chunk, Some(3));
        assert_eq!(engine.current_match_index(), Some(1));
    }

    #[test]
    fn test_buffer_truncation() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("match", PatternMode::Normal).unwrap();

        let chunks = vec![
            "match 0".to_string(),
            "match 1".to_string(),
            "match 2".to_string(),
            "match 3".to_string(),
        ];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 4);

        // Drop first 2 chunks
        engine.handle_buffer_truncation(2);

        assert_eq!(engine.match_count(), 2);
        // Indices should be adjusted
        assert_eq!(engine.matches()[0].chunk_index, 0); // Was 2
        assert_eq!(engine.matches()[1].chunk_index, 1); // Was 3
    }

    #[test]
    fn test_matches_for_chunk() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("a", PatternMode::Normal).unwrap();

        let chunks = vec!["aaa".to_string(), "bbb".to_string(), "aba".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);

        // Chunk 0 has 3 matches
        let chunk0_matches: Vec<_> = engine.matches_for_chunk(0).collect();
        assert_eq!(chunk0_matches.len(), 3);

        // Chunk 1 has no matches
        let chunk1_matches: Vec<_> = engine.matches_for_chunk(1).collect();
        assert_eq!(chunk1_matches.len(), 0);

        // Chunk 2 has 2 matches
        let chunk2_matches: Vec<_> = engine.matches_for_chunk(2).collect();
        assert_eq!(chunk2_matches.len(), 2);
    }

    #[test]
    fn test_status_message() {
        let mut engine = SearchEngine::default();

        // No pattern
        assert_eq!(engine.status_message(), "");

        // Pattern with no matches
        engine.set_pattern("xyz", PatternMode::Normal).unwrap();
        let chunks = vec!["abc".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert!(engine.status_message().contains("not found"));

        // Pattern with matches
        engine.set_pattern("abc", PatternMode::Normal).unwrap();
        let chunks = vec!["abc def abc".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert!(engine.status_message().contains("2 matches"));

        // Navigate to match
        engine.goto_next_match();
        assert!(engine.status_message().contains("Match 1/2"));
    }

    #[test]
    fn test_no_pattern_returns_empty() {
        let mut engine = SearchEngine::default();

        let chunks = vec!["test".to_string()];
        let matches = engine.search(chunks.len(), |i| &chunks[i]);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_set_pattern_resets_search() {
        let mut engine = SearchEngine::default();
        engine.set_pattern("test", PatternMode::Normal).unwrap();

        let chunks = vec!["test".to_string(), "test".to_string()];
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 2);

        // Set new pattern - should reset
        engine.set_pattern("other", PatternMode::Normal).unwrap();
        assert_eq!(engine.match_count(), 0);

        // Search with new pattern
        engine.search(chunks.len(), |i| &chunks[i]);
        assert_eq!(engine.match_count(), 0); // "other" not found
    }
}
