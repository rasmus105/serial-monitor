//! Search and filter utilities
//!
//! This module provides shared search and filter functionality that can be
//! used by any frontend (TUI, GUI, etc.).
//!
//! # Key Types
//!
//! - [`PatternMatcher`]: Cached pattern matching with regex and literal modes
//! - [`SearchEngine`]: Full search functionality with match tracking
//! - [`PatternMode`]: Search mode selection (literal vs regex)
//! - [`SearchMatch`]: A single match occurrence with position info

mod engine;
mod pattern;

pub use engine::{SearchEngine, SearchMatch, SearchResult};
pub use pattern::{PatternMatcher, PatternMode};
