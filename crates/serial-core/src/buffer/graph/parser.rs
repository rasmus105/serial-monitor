use std::collections::HashSet;
use std::time::SystemTime;

use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, Display};

use super::super::chunk::Direction;

/// A parsed value with series name.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedValue {
    /// Series name (e.g., "temperature", "col0"). Naming scheme is parser-specific.
    pub series: String,
    /// The numeric value.
    pub value: f64,
}

/// Parses data into named numeric values.
#[enum_dispatch]
pub trait GraphParser: Send + Sync + std::fmt::Debug {
    /// Parse a string, returning zero or more named values.
    ///
    /// The timestamp and direction are provided for context but most parsers
    /// only need the text content.
    fn parse_str(&self, text: &str, timestamp: SystemTime, direction: Direction) -> Vec<ParsedValue>;
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
    fn parse_str(&self, text: &str, _timestamp: SystemTime, _direction: Direction) -> Vec<ParsedValue> {
        let data = text.as_bytes();
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
///
/// Supports two modes:
/// 1. **Key/Value mode**: Pattern contains `(?P<key>...)` and `(?P<value>...)` groups.
///    Each match extracts a dynamic series name from `key` and its value from `value`.
/// 2. **Named groups mode**: Each named capture group becomes a series, and the
///    captured text is parsed as a number.
#[derive(Debug, Clone)]
pub struct Regex {
    /// Pre-compiled regex pattern
    regex: regex::Regex,
    /// True if pattern uses key/value capture groups
    has_key_value_groups: bool,
    /// Cached named capture group names (excludes "key" and "value")
    group_names: Vec<String>,
}

impl Regex {
    /// Create a new Regex parser with the given pattern.
    ///
    /// Returns an error if the pattern is invalid.
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        let regex = regex::Regex::new(pattern)?;

        let mut has_key_value_groups = false;
        let mut group_names = Vec::new();

        for name in regex.capture_names().flatten() {
            if name == "key" || name == "value" {
                has_key_value_groups = true;
            } else {
                group_names.push(name.to_string());
            }
        }

        Ok(Self {
            regex,
            has_key_value_groups,
            group_names,
        })
    }

    /// Returns the original pattern string.
    pub fn pattern(&self) -> &str {
        self.regex.as_str()
    }
}

impl GraphParser for Regex {
    fn parse_str(&self, text: &str, _timestamp: SystemTime, _direction: Direction) -> Vec<ParsedValue> {
        let mut results = Vec::with_capacity(8);

        for caps in self.regex.captures_iter(text) {
            if self.has_key_value_groups {
                // Generic key/value pattern: (?P<key>...) and (?P<value>...)
                if let (Some(key_match), Some(value_match)) = (caps.name("key"), caps.name("value"))
                    && let Ok(value) = value_match.as_str().parse::<f64>()
                {
                    results.push(ParsedValue {
                        series: key_match.as_str().to_string(),
                        value,
                    });
                }
            } else {
                // Named groups are series names, captured values are parsed as numbers
                for name in &self.group_names {
                    if let Some(m) = caps.name(name)
                        && let Ok(value) = m.as_str().parse::<f64>()
                    {
                        results.push(ParsedValue {
                            series: name.clone(),
                            value,
                        });
                    }
                }
            }
        }

        results
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
    fn parse_str(&self, text: &str, _timestamp: SystemTime, _direction: Direction) -> Vec<ParsedValue> {
        let data = text.as_bytes();

        // Only support ASCII delimiters for byte-level parsing
        if !self.delimiter.is_ascii() {
            return Vec::new();
        }
        let delim_byte = self.delimiter as u8;

        let mut results = Vec::with_capacity(self.column_names.len().max(8));
        let mut col_index = 0;
        let mut start = 0;

        // Process each field
        for (pos, &byte) in data.iter().enumerate() {
            if byte == delim_byte {
                self.parse_field(data, start, pos, col_index, &mut results);
                col_index += 1;
                start = pos + 1;
            }
        }

        // Parse the last field (after final delimiter or if no delimiters)
        if start <= data.len() {
            self.parse_field(data, start, data.len(), col_index, &mut results);
        }

        results
    }
}

impl Csv {
    /// Parse a single field and add to results if it's a valid number.
    fn parse_field(
        &self,
        data: &[u8],
        start: usize,
        end: usize,
        col_index: usize,
        results: &mut Vec<ParsedValue>,
    ) {
        let field = &data[start..end];

        // Trim whitespace
        let trimmed = Self::trim_bytes(field);
        if trimmed.is_empty() {
            return;
        }

        // Parse as UTF-8 string, then as f64
        let Ok(field_str) = std::str::from_utf8(trimmed) else {
            return;
        };

        let Ok(value) = field_str.parse::<f64>() else {
            return;
        };

        // Get series name
        let series = if col_index < self.column_names.len() {
            self.column_names[col_index].clone()
        } else {
            format!("col{}", col_index)
        };

        results.push(ParsedValue { series, value });
    }

    /// Trim leading and trailing ASCII whitespace from bytes.
    #[inline]
    fn trim_bytes(data: &[u8]) -> &[u8] {
        let start = data
            .iter()
            .position(|&b| !b.is_ascii_whitespace())
            .unwrap_or(data.len());

        let end = data
            .iter()
            .rposition(|&b| !b.is_ascii_whitespace())
            .map(|p| p + 1)
            .unwrap_or(0);

        if start >= end { &[] } else { &data[start..end] }
    }
}

// ============================================================================
// JSON Parser
// ============================================================================

/// Extracts numeric fields from JSON objects.
///
/// Supports:
/// - Flat objects: `{"temperature": 25.5}` -> series "temperature"
/// - Nested objects: `{"sensor": {"temp": 25.5}}` -> series "sensor.temp"
/// - Arrays: `{"values": [1, 2, 3]}` -> series "values.0", "values.1", "values.2"
///
/// Non-numeric fields (strings, booleans, nulls) are silently skipped.
#[derive(Debug, Clone, Default)]
pub struct Json;

impl Json {
    /// Recursively extract numeric values from a JSON value.
    fn extract_values(value: &serde_json::Value, prefix: &str, results: &mut Vec<ParsedValue>) {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    results.push(ParsedValue {
                        series: prefix.to_string(),
                        value: f,
                    });
                }
            }
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::extract_values(val, &new_prefix, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for (idx, val) in arr.iter().enumerate() {
                    let new_prefix = if prefix.is_empty() {
                        idx.to_string()
                    } else {
                        format!("{}.{}", prefix, idx)
                    };
                    Self::extract_values(val, &new_prefix, results);
                }
            }
            // Skip strings, booleans, and nulls
            _ => {}
        }
    }
}

impl GraphParser for Json {
    fn parse_str(&self, text: &str, _timestamp: SystemTime, _direction: Direction) -> Vec<ParsedValue> {
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::with_capacity(8);
        Self::extract_values(&value, "", &mut results);
        results
    }
}

// ============================================================================
// Raw Numbers Parser
// ============================================================================

/// Extracts all numbers found in text. Series names are "0", "1", "2", etc.
///
/// Supports integers and floating-point numbers, including:
/// - Integers: `42`, `-17`
/// - Floats: `3.14`, `-0.5`, `.25`
/// - Scientific notation: `1e10`, `2.5E-3`
///
/// Numbers are extracted in order of appearance.
#[derive(Debug, Clone, Default)]
pub struct RawNumbers;

impl RawNumbers {
    /// Check if a byte can start a number (digit, minus, or decimal point).
    #[inline]
    fn can_start_number(b: u8) -> bool {
        b.is_ascii_digit() || b == b'-' || b == b'.'
    }

    /// Check if a byte can be part of a number.
    #[inline]
    fn is_number_char(b: u8) -> bool {
        b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' || b == b'e' || b == b'E'
    }

    /// Check if the byte before a potential number start is a valid boundary.
    /// Numbers shouldn't be extracted from the middle of identifiers.
    #[inline]
    fn is_valid_boundary(b: u8) -> bool {
        !b.is_ascii_alphanumeric() && b != b'_'
    }
}

impl GraphParser for RawNumbers {
    fn parse_str(&self, text: &str, _timestamp: SystemTime, _direction: Direction) -> Vec<ParsedValue> {
        let data = text.as_bytes();
        let mut results = Vec::with_capacity(8);
        let mut i = 0;

        while i < data.len() {
            // Look for potential number start
            if !Self::can_start_number(data[i]) {
                i += 1;
                continue;
            }

            // Check boundary before this position
            if i > 0 && !Self::is_valid_boundary(data[i - 1]) {
                i += 1;
                continue;
            }

            // Handle lone minus or dot - need at least one digit
            if (data[i] == b'-' || data[i] == b'.')
                && (i + 1 >= data.len() || !data[i + 1].is_ascii_digit())
            {
                // Special case: "-.5" pattern
                if data[i] == b'-'
                    && i + 2 < data.len()
                    && data[i + 1] == b'.'
                    && data[i + 2].is_ascii_digit()
                {
                    // Continue to parse
                } else {
                    i += 1;
                    continue;
                }
            }

            // Find end of number
            let start = i;
            while i < data.len() && Self::is_number_char(data[i]) {
                i += 1;
            }

            // Try to parse the candidate
            let candidate = &data[start..i];
            if let Ok(s) = std::str::from_utf8(candidate)
                && let Ok(value) = s.parse::<f64>()
            {
                results.push(ParsedValue {
                    series: results.len().to_string(),
                    value,
                });
            }
        }

        results
    }
}
