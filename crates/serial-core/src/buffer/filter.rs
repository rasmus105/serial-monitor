//! Filter state for pattern filtering
//!
//! Internal module that holds the filter pattern matcher.
//! The actual filtering logic is in [`DataBuffer`](super::DataBuffer).

use super::pattern::PatternMatcher;

/// Internal filter state
///
/// Holds the pattern matcher for text filtering.
/// Direction filtering (show_tx/show_rx) is handled directly by DataBuffer.
#[derive(Debug, Default)]
pub(crate) struct FilterState {
    /// Pattern matcher for text filtering
    pub pattern: PatternMatcher,
}
