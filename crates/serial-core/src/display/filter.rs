//! Filter state for pattern and direction filtering
//!
//! Internal module that manages which chunks pass filter criteria.
//! The actual filtered view (VecDeque<Rc<DisplayChunk>>) is maintained
//! by DisplayBuffer - this module just handles the filter logic.

use super::chunk::DisplayChunk;
use super::pattern::{PatternMatcher, PatternMode};

/// Internal filter state
///
/// Manages pattern-based and direction-based filtering criteria.
/// The filtered view itself is maintained by DisplayBuffer using Rc clones.
#[derive(Debug)]
pub(crate) struct FilterState {
    /// Pattern matcher for text filtering
    pattern: PatternMatcher,

    /// Show TX (transmitted) chunks
    show_tx: bool,

    /// Show RX (received) chunks
    show_rx: bool,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            pattern: PatternMatcher::default(),
            show_tx: true,
            show_rx: true,
        }
    }
}

impl FilterState {
    // -------------------------------------------------------------------------
    // Pattern configuration
    // -------------------------------------------------------------------------

    /// Set filter pattern
    pub fn set_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.pattern.set_pattern(pattern, mode)
    }

    /// Set pattern mode, keeping the same pattern
    pub fn set_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        self.pattern.set_mode(mode)
    }

    /// Clear the filter pattern
    pub fn clear_pattern(&mut self) {
        self.pattern.clear();
    }

    /// Check if a pattern is set
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
    // Direction configuration
    // -------------------------------------------------------------------------

    /// Set whether TX chunks are shown
    pub fn set_show_tx(&mut self, show: bool) {
        self.show_tx = show;
    }

    /// Set whether RX chunks are shown
    pub fn set_show_rx(&mut self, show: bool) {
        self.show_rx = show;
    }

    /// Check if TX chunks are shown
    pub fn show_tx(&self) -> bool {
        self.show_tx
    }

    /// Check if RX chunks are shown
    pub fn show_rx(&self) -> bool {
        self.show_rx
    }

    // -------------------------------------------------------------------------
    // Filter logic
    // -------------------------------------------------------------------------

    /// Check if any filter is active
    ///
    /// A filter is active if:
    /// - A pattern is set, OR
    /// - TX is hidden, OR
    /// - RX is hidden
    pub fn is_active(&self) -> bool {
        self.has_pattern() || !self.show_tx || !self.show_rx
    }

    /// Check if a chunk passes all filter criteria
    pub fn matches(&self, chunk: &DisplayChunk) -> bool {
        // Direction check
        let direction_ok = match chunk.direction {
            crate::Direction::Tx => self.show_tx,
            crate::Direction::Rx => self.show_rx,
        };

        if !direction_ok {
            return false;
        }

        // Pattern check (no pattern = match everything)
        self.pattern.is_match(&chunk.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;

    #[test]
    fn test_is_active() {
        let mut filter = FilterState::default();

        // Initially not active
        assert!(!filter.is_active());

        // Pattern makes it active
        filter.set_pattern("test", PatternMode::Normal).unwrap();
        assert!(filter.is_active());

        filter.clear_pattern();
        assert!(!filter.is_active());

        // Hiding TX makes it active
        filter.set_show_tx(false);
        assert!(filter.is_active());

        filter.set_show_tx(true);
        assert!(!filter.is_active());

        // Hiding RX makes it active
        filter.set_show_rx(false);
        assert!(filter.is_active());
    }

    #[test]
    fn test_direction_filtering() {
        let mut filter = FilterState::default();

        let tx_chunk = DisplayChunk {
            content: "sent".to_string(),
            direction: Direction::Tx,
        };
        let rx_chunk = DisplayChunk {
            content: "received".to_string(),
            direction: Direction::Rx,
        };

        // Both shown by default
        assert!(filter.matches(&tx_chunk));
        assert!(filter.matches(&rx_chunk));

        // Hide TX
        filter.set_show_tx(false);
        assert!(!filter.matches(&tx_chunk));
        assert!(filter.matches(&rx_chunk));

        // Hide RX too
        filter.set_show_rx(false);
        assert!(!filter.matches(&tx_chunk));
        assert!(!filter.matches(&rx_chunk));
    }

    #[test]
    fn test_pattern_filtering() {
        let mut filter = FilterState::default();
        filter.set_pattern("error", PatternMode::Normal).unwrap();

        let matching = DisplayChunk {
            content: "an error occurred".to_string(),
            direction: Direction::Rx,
        };
        let non_matching = DisplayChunk {
            content: "all good".to_string(),
            direction: Direction::Rx,
        };

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn test_combined_filtering() {
        let mut filter = FilterState::default();
        filter.set_pattern("data", PatternMode::Normal).unwrap();
        filter.set_show_tx(false);

        // TX with matching content - fails (TX hidden)
        let tx_match = DisplayChunk {
            content: "sending data".to_string(),
            direction: Direction::Tx,
        };
        assert!(!filter.matches(&tx_match));

        // RX with matching content - passes
        let rx_match = DisplayChunk {
            content: "received data".to_string(),
            direction: Direction::Rx,
        };
        assert!(filter.matches(&rx_match));

        // RX with non-matching content - fails (pattern)
        let rx_nomatch = DisplayChunk {
            content: "nothing here".to_string(),
            direction: Direction::Rx,
        };
        assert!(!filter.matches(&rx_nomatch));
    }
}
