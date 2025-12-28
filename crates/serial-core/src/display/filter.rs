//! Filter state for pattern and direction filtering
//!
//! Internal module that manages which chunks pass filter criteria.
//! The actual filtered view (VecDeque<Rc<DisplayChunk>>) is maintained
//! by DisplayBuffer - this module just handles the filter logic.

use super::chunk::DisplayChunk;
use super::pattern::PatternMatcher;

/// Internal filter state
///
/// Manages pattern-based and direction-based filtering criteria.
/// The filtered view itself is maintained by DisplayBuffer using Rc clones.
#[derive(Debug, Default)]
pub(crate) struct FilterState {
    /// Pattern matcher for text filtering
    pub pattern: PatternMatcher,

    /// Show TX (transmitted) chunks
    pub show_tx: bool,

    /// Show RX (received) chunks
    pub show_rx: bool,
}

impl FilterState {
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
    // FilterState logic is straightforward - tests would just verify
    // basic boolean operations work. Meaningful tests will be at the
    // DisplayBuffer integration level.
}
