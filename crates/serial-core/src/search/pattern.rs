//! Pattern matching utilities with caching
//!
//! Provides efficient pattern matching for search and filter operations.
//! Supports both literal string matching (case-insensitive) and regex matching.
//! Regex patterns are compiled once and cached for reuse.

use regex::Regex;
use std::fmt;

/// Pattern matching mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternMode {
    /// Literal string matching (case-insensitive)
    #[default]
    Normal,
    /// Regular expression matching
    Regex,
}

impl PatternMode {
    /// Toggle between Normal and Regex modes
    pub fn toggle(&self) -> Self {
        match self {
            PatternMode::Normal => PatternMode::Regex,
            PatternMode::Regex => PatternMode::Normal,
        }
    }

    /// Get the display name for this mode
    pub fn name(&self) -> &'static str {
        match self {
            PatternMode::Normal => "Normal",
            PatternMode::Regex => "Regex",
        }
    }

    /// Get a description of this mode
    pub fn description(&self) -> &'static str {
        match self {
            PatternMode::Normal => "Pattern is interpreted as a literal string (case-insensitive)",
            PatternMode::Regex => "Pattern is interpreted as a regular expression",
        }
    }

    /// Get all available modes
    pub fn all() -> &'static [PatternMode] {
        &[PatternMode::Normal, PatternMode::Regex]
    }
}

impl fmt::Display for PatternMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Compiled pattern for efficient matching
///
/// This enum holds either a pre-lowercased literal string for case-insensitive
/// matching, or a compiled regex. Creating a `CompiledPattern` validates and
/// compiles the pattern once, allowing for efficient repeated matching.
#[derive(Debug, Clone)]
enum CompiledPattern {
    /// Lowercased literal for case-insensitive matching
    Literal(String),
    /// Compiled regex
    Regex(Regex),
}

/// A cached pattern matcher for efficient search/filter operations
///
/// `PatternMatcher` compiles and caches patterns for efficient reuse.
/// It supports both literal string matching (case-insensitive) and regex matching.
///
/// # Example
///
/// ```
/// use serial_core::{PatternMatcher, PatternMode};
///
/// let mut matcher = PatternMatcher::new();
///
/// // Set a literal pattern (case-insensitive)
/// matcher.set_pattern("hello", PatternMode::Normal).unwrap();
/// assert!(matcher.is_match("Hello World"));
/// assert!(matcher.is_match("HELLO"));
///
/// // Set a regex pattern
/// matcher.set_pattern(r"\d+", PatternMode::Regex).unwrap();
/// assert!(matcher.is_match("value: 42"));
/// assert!(!matcher.is_match("no numbers"));
/// ```
#[derive(Debug, Default)]
pub struct PatternMatcher {
    /// The original pattern string
    pattern: Option<String>,
    /// The matching mode
    mode: PatternMode,
    /// Compiled pattern for efficient matching
    compiled: Option<CompiledPattern>,
    /// Error message if pattern compilation failed
    error: Option<String>,
}

impl PatternMatcher {
    /// Create a new empty PatternMatcher
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a PatternMatcher with an initial pattern
    ///
    /// Returns an error if the pattern is invalid (e.g., invalid regex).
    pub fn with_pattern(pattern: &str, mode: PatternMode) -> Result<Self, String> {
        let mut matcher = Self::new();
        matcher.set_pattern(pattern, mode)?;
        Ok(matcher)
    }

    /// Set a new pattern and mode
    ///
    /// This compiles the pattern (for regex mode) and caches it for efficient matching.
    /// Returns an error if the pattern is invalid.
    pub fn set_pattern(&mut self, pattern: &str, mode: PatternMode) -> Result<(), String> {
        self.error = None;

        if pattern.is_empty() {
            self.pattern = None;
            self.compiled = None;
            self.mode = mode;
            return Ok(());
        }

        let compiled = match mode {
            PatternMode::Normal => CompiledPattern::Literal(pattern.to_lowercase()),
            PatternMode::Regex => {
                match Regex::new(pattern) {
                    Ok(re) => CompiledPattern::Regex(re),
                    Err(e) => {
                        let error_msg = format!("Invalid regex: {}", e);
                        self.error = Some(error_msg.clone());
                        return Err(error_msg);
                    }
                }
            }
        };

        self.pattern = Some(pattern.to_string());
        self.mode = mode;
        self.compiled = Some(compiled);
        Ok(())
    }

    /// Update just the mode, keeping the same pattern
    ///
    /// This re-compiles the pattern with the new mode.
    pub fn set_mode(&mut self, mode: PatternMode) -> Result<(), String> {
        if let Some(ref pattern) = self.pattern.clone() {
            self.set_pattern(pattern, mode)
        } else {
            self.mode = mode;
            Ok(())
        }
    }

    /// Clear the pattern
    pub fn clear(&mut self) {
        self.pattern = None;
        self.compiled = None;
        self.error = None;
    }

    /// Check if a pattern is set
    pub fn has_pattern(&self) -> bool {
        self.pattern.is_some() && self.compiled.is_some()
    }

    /// Get the current pattern string
    pub fn pattern(&self) -> Option<&str> {
        self.pattern.as_deref()
    }

    /// Get the current mode
    pub fn mode(&self) -> PatternMode {
        self.mode
    }

    /// Get the error message if pattern compilation failed
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Check if the text matches the pattern
    ///
    /// Returns `true` if no pattern is set (matches everything).
    pub fn is_match(&self, text: &str) -> bool {
        match &self.compiled {
            None => true, // No pattern = match everything
            Some(CompiledPattern::Literal(pattern_lower)) => {
                text.to_lowercase().contains(pattern_lower)
            }
            Some(CompiledPattern::Regex(re)) => re.is_match(text),
        }
    }

    /// Find all matches in the text, returning byte ranges
    ///
    /// Returns an empty vec if no pattern is set.
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        match &self.compiled {
            None => vec![],
            Some(CompiledPattern::Literal(pattern_lower)) => {
                let text_lower = text.to_lowercase();
                let mut matches = Vec::new();
                let mut search_start = 0;

                while let Some(rel_pos) = text_lower[search_start..].find(pattern_lower) {
                    let byte_start = search_start + rel_pos;
                    let byte_end = byte_start + pattern_lower.len();
                    matches.push((byte_start, byte_end));
                    search_start = byte_end;
                }

                matches
            }
            Some(CompiledPattern::Regex(re)) => {
                re.find_iter(text)
                    .map(|m| (m.start(), m.end()))
                    .collect()
            }
        }
    }
}

impl Clone for PatternMatcher {
    fn clone(&self) -> Self {
        // Re-create by re-compiling the pattern (Regex doesn't implement Clone easily)
        let mut new = Self::new();
        if let Some(ref pattern) = self.pattern {
            // Ignore error on clone - pattern was already validated
            let _ = new.set_pattern(pattern, self.mode);
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_pattern_matches_all() {
        let matcher = PatternMatcher::new();
        assert!(matcher.is_match("anything"));
        assert!(matcher.is_match(""));
    }

    #[test]
    fn test_literal_case_insensitive() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern("hello", PatternMode::Normal).unwrap();

        assert!(matcher.is_match("hello"));
        assert!(matcher.is_match("HELLO"));
        assert!(matcher.is_match("Hello World"));
        assert!(matcher.is_match("say hello there"));
        assert!(!matcher.is_match("hi there"));
    }

    #[test]
    fn test_regex_matching() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern(r"\d{3}", PatternMode::Regex).unwrap();

        assert!(matcher.is_match("code: 123"));
        assert!(matcher.is_match("456"));
        assert!(!matcher.is_match("12")); // Only 2 digits
        assert!(!matcher.is_match("abc"));
    }

    #[test]
    fn test_invalid_regex() {
        let mut matcher = PatternMatcher::new();
        let result = matcher.set_pattern(r"[invalid", PatternMode::Regex);

        assert!(result.is_err());
        assert!(matcher.error().is_some());
        assert!(!matcher.has_pattern());
    }

    #[test]
    fn test_find_matches_literal() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern("ab", PatternMode::Normal).unwrap();

        let matches = matcher.find_matches("ab AB aB Ab");
        // "ab AB aB Ab" lowercased is "ab ab ab ab"
        // Positions: 0-2, 3-5, 6-8, 9-11
        assert_eq!(matches.len(), 4);
        assert_eq!(matches[0], (0, 2));
        assert_eq!(matches[1], (3, 5));
    }

    #[test]
    fn test_find_matches_regex() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern(r"\d+", PatternMode::Regex).unwrap();

        let matches = matcher.find_matches("a1b23c456");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0], (1, 2));  // "1"
        assert_eq!(matches[1], (3, 5));  // "23"
        assert_eq!(matches[2], (6, 9));  // "456"
    }

    #[test]
    fn test_mode_toggle() {
        assert_eq!(PatternMode::Normal.toggle(), PatternMode::Regex);
        assert_eq!(PatternMode::Regex.toggle(), PatternMode::Normal);
    }

    #[test]
    fn test_set_mode_recompiles() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern("hello", PatternMode::Normal).unwrap();

        // Should work - "hello" is a valid regex too
        matcher.set_mode(PatternMode::Regex).unwrap();
        assert_eq!(matcher.mode(), PatternMode::Regex);
        assert!(matcher.is_match("hello world"));
    }

    #[test]
    fn test_clear() {
        let mut matcher = PatternMatcher::new();
        matcher.set_pattern("test", PatternMode::Normal).unwrap();
        assert!(matcher.has_pattern());

        matcher.clear();
        assert!(!matcher.has_pattern());
        assert!(matcher.is_match("anything")); // Empty pattern matches all
    }
}
