use std::collections::HashSet;

use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, Display};

use crate::DataChunk;

/// A parsed value with series name.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedValue {
    /// Series name (e.g., "temperature", "col0"). Naming scheme is parser-specific.
    pub series: String,
    /// The numeric value.
    pub value: f64,
}

/// Parses data chunks into named numeric values.
#[enum_dispatch]
pub trait GraphParser: Send + Sync + std::fmt::Debug {
    /// Parse a chunk of data, returning zero or more named values.
    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue>;
}

#[enum_dispatch(GraphParser)]
#[derive(Debug, Clone, Display, AsRefStr)]
pub enum GraphParserType {
    /// Extract key-value patterns (e.g. `key=value`, `key: value`, etc.)
    #[strum(serialize = "Key Value")]
    KeyValue,
    /// User-defined regex with named capture groups
    #[strum(serialize = "Regex")]
    Regex,
    /// Parse comma-separated values
    #[strum(serialize = "CSV")]
    Csv,
    /// Parse JSON data
    #[strum(serialize = "JSON")]
    Json,
    /// Extract all numbers found in text
    #[strum(serialize = "Raw Numbers")]
    RawNumbers,
}

// ============================================================================
// Key Value Parser
// ============================================================================

/// Extracts `key=value`, `key:value` or `key: value` patterns.
#[derive(Debug, Clone, Default)]
pub struct KeyValue;

impl KeyValue {
    /// Check if byte is a pair separator
    #[inline]
    fn is_pair_separator(b: u8) -> bool {
        matches!(b, b',' | b' ' | b'\t' | b';')
    }

    /// Check if byte is a key-value separator
    #[inline]
    fn is_kv_separator(b: u8) -> bool {
        matches!(b, b'=' | b':')
    }

    /// Extract key bytes by scanning backwards from separator position.
    /// Returns the trimmed key slice.
    fn extract_key(data: &[u8], sep_pos: usize) -> &[u8] {
        let before_sep = &data[..sep_pos];

        // Trim trailing whitespace
        let end = before_sep
            .iter()
            .rposition(|&b| b != b' ' && b != b'\t')
            .map(|p| p + 1)
            .unwrap_or(0);

        // Find start of key (after last pair separator)
        let start = before_sep[..end]
            .iter()
            .rposition(|&b| Self::is_pair_separator(b))
            .map(|p| p + 1)
            .unwrap_or(0);

        // Trim leading whitespace from key
        let key = &before_sep[start..end];
        let trim_start = key
            .iter()
            .position(|&b| b != b' ' && b != b'\t')
            .unwrap_or(key.len());

        &key[trim_start..]
    }

    /// Extract value bytes by scanning forwards from separator position.
    /// Returns the trimmed value slice.
    fn extract_value(data: &[u8], sep_pos: usize) -> &[u8] {
        let after_sep = &data[sep_pos + 1..];

        // Trim leading whitespace
        let start = after_sep
            .iter()
            .position(|&b| b != b' ' && b != b'\t')
            .unwrap_or(after_sep.len());

        // Find end of value (before next pair separator)
        let end = after_sep[start..]
            .iter()
            .position(|&b| Self::is_pair_separator(b))
            .unwrap_or(after_sep.len() - start);

        // Trim trailing whitespace from value
        let value = &after_sep[start..start + end];
        let trim_end = value
            .iter()
            .rposition(|&b| b != b' ' && b != b'\t')
            .map(|p| p + 1)
            .unwrap_or(0);

        &value[..trim_end]
    }
}

impl GraphParser for KeyValue {
    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let data = &chunk.data;
        let mut results = Vec::with_capacity(8);
        let mut seen_keys: HashSet<&[u8]> = HashSet::new();

        // Single pass: find all key-value separators
        for (pos, &byte) in data.iter().enumerate() {
            if !Self::is_kv_separator(byte) {
                continue;
            }

            let key_bytes = Self::extract_key(data, pos);
            if key_bytes.is_empty() || seen_keys.contains(key_bytes) {
                continue;
            }

            let value_bytes = Self::extract_value(data, pos);

            // Only convert to UTF-8 for the value we need to parse
            let Ok(value_str) = std::str::from_utf8(value_bytes) else {
                continue;
            };

            let Ok(value) = value_str.parse::<f64>() else {
                continue;
            };

            // Only convert key to string when we have a valid value
            let Ok(key_str) = std::str::from_utf8(key_bytes) else {
                continue;
            };

            seen_keys.insert(key_bytes);
            results.push(ParsedValue {
                series: key_str.to_string(),
                value,
            });
        }

        results
    }
}

// ============================================================================
// Regex Parser
// ============================================================================

/// User-defined regex with named capture groups becoming series names.
#[derive(Debug, Clone)]
pub struct Regex {
    pub pattern: String,
}

impl GraphParser for Regex {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        todo!()
    }
}

// ============================================================================
// CSV Parser
// ============================================================================

/// Parses delimiter-separated numeric values.
#[derive(Debug, Clone)]
pub struct Csv {
    pub delimiter: char,
    /// Column names. If empty, uses "col0", "col1", etc.
    pub column_names: Vec<String>,
}

impl Default for Csv {
    fn default() -> Self {
        Self {
            delimiter: ',',
            column_names: Vec::new(),
        }
    }
}

impl GraphParser for Csv {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        todo!()
    }
}

// ============================================================================
// JSON Parser
// ============================================================================

/// Extracts numeric fields from JSON objects.
#[derive(Debug, Clone, Default)]
pub struct Json;

impl GraphParser for Json {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        todo!()
    }
}

// ============================================================================
// Raw Numbers Parser
// ============================================================================

/// Extracts all numbers found in text. Series names are "0", "1", "2", etc.
#[derive(Debug, Clone, Default)]
pub struct RawNumbers;

impl GraphParser for RawNumbers {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        todo!()
    }
}
