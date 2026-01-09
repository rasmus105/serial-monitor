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
    fn parse_str(
        &self,
        text: &str,
        timestamp: SystemTime,
        direction: Direction,
    ) -> Vec<ParsedValue>;
}

#[enum_dispatch(GraphParser)]
#[derive(Debug, Clone, Display, AsRefStr)]
pub enum GraphParserType {
    /// Smart parser: extracts numbers with optional labels
    #[strum(serialize = "Smart")]
    Smart,
    /// User-defined regex with named capture groups
    #[strum(serialize = "Regex")]
    Regex,
    /// Parse comma-separated values
    #[strum(serialize = "CSV")]
    Csv,
    /// Parse JSON data
    #[strum(serialize = "JSON")]
    Json,
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
    fn parse_str(
        &self,
        text: &str,
        _timestamp: SystemTime,
        _direction: Direction,
    ) -> Vec<ParsedValue> {
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
    fn parse_str(
        &self,
        text: &str,
        _timestamp: SystemTime,
        _direction: Direction,
    ) -> Vec<ParsedValue> {
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
    fn parse_str(
        &self,
        text: &str,
        _timestamp: SystemTime,
        _direction: Direction,
    ) -> Vec<ParsedValue> {
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
// Smart Parser
// ============================================================================

/// Smart parser that extracts numbers with optional labels.
///
/// Handles multiple formats intelligently:
/// - Key-value: `temp:32.2`, `temp=32.2`, `temp: 32.2`
/// - Labeled sequences: `acceleration: 49, 183, -321.4` → acceleration.0, acceleration.1, acceleration.2
/// - Raw numbers: `329, 412, 39` → 0, 1, 2
/// - Mixed: `acc 94, 11008` → acc.0, acc.1
///
/// Numbers are extracted by scanning for valid numeric characters, not by
/// looking for separators. This handles edge cases like `temp:32.2\r\n` correctly.
#[derive(Debug, Clone, Default)]
pub struct Smart;

/// A labeled group of values.
struct LabeledGroup {
    label: String,
    values: Vec<f64>,
}

impl Smart {
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

    /// Check if a byte is a valid label character (alphanumeric or underscore).
    #[inline]
    fn is_label_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    /// Check if a byte is a key-value separator.
    #[inline]
    fn is_kv_separator(b: u8) -> bool {
        b == b':' || b == b'='
    }

    /// Try to extract a number starting at the given position.
    /// Returns the parsed value and the end position (exclusive), or None if not a valid number.
    fn try_extract_number(data: &[u8], start: usize) -> Option<(f64, usize)> {
        if start >= data.len() {
            return None;
        }

        let b = data[start];
        if !Self::can_start_number(b) {
            return None;
        }

        // Handle lone minus or dot - need at least one digit
        if (b == b'-' || b == b'.')
            && (start + 1 >= data.len() || !data[start + 1].is_ascii_digit())
        {
            // Special case: "-.5" pattern
            if b == b'-'
                && start + 2 < data.len()
                && data[start + 1] == b'.'
                && data[start + 2].is_ascii_digit()
            {
                // Continue to parse
            } else {
                return None;
            }
        }

        // Find end of number
        let mut end = start;
        while end < data.len() && Self::is_number_char(data[end]) {
            end += 1;
        }

        // Try to parse the candidate
        let candidate = &data[start..end];
        let s = std::str::from_utf8(candidate).ok()?;
        let value = s.parse::<f64>().ok()?;
        Some((value, end))
    }

    /// Find all labeled groups in the data.
    /// Returns the groups and whether any labels were found.
    fn find_labeled_groups(data: &[u8]) -> Vec<LabeledGroup> {
        let mut groups = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Look for a label (word starting with letter or underscore)
            if !Self::is_label_char(data[i]) || (!data[i].is_ascii_alphabetic() && data[i] != b'_')
            {
                i += 1;
                continue;
            }

            // Found potential label start
            let label_start = i;
            while i < data.len() && Self::is_label_char(data[i]) {
                i += 1;
            }
            let label_end = i;

            // Skip whitespace after label
            while i < data.len() && (data[i] == b' ' || data[i] == b'\t') {
                i += 1;
            }

            // Check for separator (: or =)
            let has_separator = i < data.len() && Self::is_kv_separator(data[i]);
            if has_separator {
                i += 1;
                // Skip whitespace after separator
                while i < data.len() && (data[i] == b' ' || data[i] == b'\t') {
                    i += 1;
                }
            }

            // Now scan for numbers, skipping any special characters
            // We collect numbers until we hit another label
            let label_bytes = &data[label_start..label_end];
            let label = match std::str::from_utf8(label_bytes) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };

            let mut values = Vec::new();
            let mut j = i;

            // Collect numbers until we find another label
            while j < data.len() {
                // Check if we're at another label
                if Self::is_label_char(data[j])
                    && (data[j].is_ascii_alphabetic() || data[j] == b'_')
                {
                    // Look ahead to see if this is a label with separator or followed by number
                    let mut k = j;
                    while k < data.len() && Self::is_label_char(data[k]) {
                        k += 1;
                    }
                    // Skip whitespace
                    while k < data.len() && (data[k] == b' ' || data[k] == b'\t') {
                        k += 1;
                    }
                    // Check for separator
                    let next_has_sep = k < data.len() && Self::is_kv_separator(data[k]);
                    if next_has_sep {
                        k += 1;
                        while k < data.len() && (data[k] == b' ' || data[k] == b'\t') {
                            k += 1;
                        }
                    }
                    // Check if followed by number (directly or after separator)
                    if next_has_sep || (k < data.len() && Self::can_start_number(data[k])) {
                        // This looks like another label, stop collecting
                        break;
                    }
                    // Not a label, skip this word
                    j = k;
                    continue;
                }

                // Try to extract a number
                if let Some((value, end)) = Self::try_extract_number(data, j) {
                    values.push(value);
                    j = end;
                } else {
                    j += 1;
                }
            }

            if !values.is_empty() {
                groups.push(LabeledGroup { label, values });
            }

            i = j;
        }

        groups
    }

    /// Extract all numbers from data (for raw number mode).
    fn extract_all_numbers(data: &[u8]) -> Vec<f64> {
        let mut values = Vec::new();
        let mut i = 0;

        while i < data.len() {
            if let Some((value, end)) = Self::try_extract_number(data, i) {
                values.push(value);
                i = end;
            } else {
                i += 1;
            }
        }

        values
    }
}

impl GraphParser for Smart {
    fn parse_str(
        &self,
        text: &str,
        _timestamp: SystemTime,
        _direction: Direction,
    ) -> Vec<ParsedValue> {
        let data = text.as_bytes();
        let mut results = Vec::with_capacity(8);

        // First, find all labeled groups
        let groups = Self::find_labeled_groups(data);

        if groups.is_empty() {
            // No labels found - extract all raw numbers
            let values = Self::extract_all_numbers(data);
            for (idx, value) in values.into_iter().enumerate() {
                results.push(ParsedValue {
                    series: idx.to_string(),
                    value,
                });
            }
        } else {
            // We have labels - only output labeled values
            for group in groups {
                if group.values.len() == 1 {
                    // Single value: just use label name
                    results.push(ParsedValue {
                        series: group.label,
                        value: group.values[0],
                    });
                } else {
                    // Multiple values: use label.index
                    for (idx, value) in group.values.into_iter().enumerate() {
                        results.push(ParsedValue {
                            series: format!("{}.{}", group.label, idx),
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(parser: &impl GraphParser, text: &str) -> Vec<ParsedValue> {
        parser.parse_str(text, SystemTime::now(), Direction::Rx)
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Basic Key-Value
    // -------------------------------------------------------------------------

    #[test]
    fn smart_simple_key_value_colon() {
        let results = parse(&Smart, "temp:32.2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 32.2);
    }

    #[test]
    fn smart_simple_key_value_equals() {
        let results = parse(&Smart, "temp=32.2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 32.2);
    }

    #[test]
    fn smart_key_value_with_space() {
        let results = parse(&Smart, "temp: 32.2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 32.2);
    }

    #[test]
    fn smart_key_value_equals_with_space() {
        let results = parse(&Smart, "temp = 32.2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 32.2);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Multiple Key-Value Pairs
    // -------------------------------------------------------------------------

    #[test]
    fn smart_multiple_space_separated() {
        let results = parse(&Smart, "temp:32.2 humidity:57.8 pressure:1017.37");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 32.2);
        assert_eq!(results[1].series, "humidity");
        assert_eq!(results[1].value, 57.8);
        assert_eq!(results[2].series, "pressure");
        assert_eq!(results[2].value, 1017.37);
    }

    #[test]
    fn smart_multiple_comma_separated() {
        let results = parse(&Smart, "temp:32.2, humidity:57.8, pressure:1017.37");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[1].series, "humidity");
        assert_eq!(results[2].series, "pressure");
    }

    #[test]
    fn smart_with_crlf_ending() {
        // The original bug - \r\n at end was included in value
        let results = parse(&Smart, "temp:32.2 humidity:57.8 pressure:1017.37\r\n");
        assert_eq!(results.len(), 3);
        assert_eq!(results[2].series, "pressure");
        assert_eq!(results[2].value, 1017.37);
    }

    #[test]
    fn smart_with_lf_ending() {
        let results = parse(&Smart, "temp:32.2 humidity:57.8 pressure:1017.37\n");
        assert_eq!(results.len(), 3);
        assert_eq!(results[2].value, 1017.37);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Labeled Sequences (name followed by multiple numbers)
    // -------------------------------------------------------------------------

    #[test]
    fn smart_labeled_sequence_colon() {
        let results = parse(&Smart, "acceleration: 49, 183, -321.4");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "acceleration.0");
        assert_eq!(results[0].value, 49.0);
        assert_eq!(results[1].series, "acceleration.1");
        assert_eq!(results[1].value, 183.0);
        assert_eq!(results[2].series, "acceleration.2");
        assert_eq!(results[2].value, -321.4);
    }

    #[test]
    fn smart_labeled_sequence_space_only() {
        // Name followed by numbers without separator
        let results = parse(&Smart, "acc 94, 11008, 3271");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "acc.0");
        assert_eq!(results[0].value, 94.0);
        assert_eq!(results[1].series, "acc.1");
        assert_eq!(results[1].value, 11008.0);
        assert_eq!(results[2].series, "acc.2");
        assert_eq!(results[2].value, 3271.0);
    }

    #[test]
    fn smart_labeled_sequence_single_value() {
        // Single value after label should just use label name (no index)
        let results = parse(&Smart, "temperature: 25.5");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "temperature");
        assert_eq!(results[0].value, 25.5);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Raw Numbers (no labels)
    // -------------------------------------------------------------------------

    #[test]
    fn smart_raw_numbers_comma_separated() {
        let results = parse(&Smart, "329, 412, 39");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "0");
        assert_eq!(results[0].value, 329.0);
        assert_eq!(results[1].series, "1");
        assert_eq!(results[1].value, 412.0);
        assert_eq!(results[2].series, "2");
        assert_eq!(results[2].value, 39.0);
    }

    #[test]
    fn smart_raw_numbers_space_separated() {
        let results = parse(&Smart, "1.5 2.5 3.5");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "0");
        assert_eq!(results[1].series, "1");
        assert_eq!(results[2].series, "2");
    }

    #[test]
    fn smart_single_raw_number() {
        let results = parse(&Smart, "42.5");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].series, "0");
        assert_eq!(results[0].value, 42.5);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Edge Cases and Special Characters
    // -------------------------------------------------------------------------

    #[test]
    fn smart_with_special_chars_in_between() {
        // Special characters between values shouldn't break parsing
        let results = parse(&Smart, "acceleration:[49,'319,':93118.9495\"]");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "acceleration.0");
        assert_eq!(results[0].value, 49.0);
        assert_eq!(results[1].series, "acceleration.1");
        assert_eq!(results[1].value, 319.0);
        assert_eq!(results[2].series, "acceleration.2");
        assert_eq!(results[2].value, 93118.9495);
    }

    #[test]
    fn smart_negative_numbers() {
        let results = parse(&Smart, "offset: -12.5, delta: -0.001");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].series, "offset");
        assert_eq!(results[0].value, -12.5);
        assert_eq!(results[1].series, "delta");
        assert_eq!(results[1].value, -0.001);
    }

    #[test]
    fn smart_scientific_notation() {
        let results = parse(&Smart, "big:1e10 small:2.5E-3");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].series, "big");
        assert_eq!(results[0].value, 1e10);
        assert_eq!(results[1].series, "small");
        assert_eq!(results[1].value, 2.5e-3);
    }

    #[test]
    fn smart_integer_values() {
        let results = parse(&Smart, "count:42 total:100");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].value, 42.0);
        assert_eq!(results[1].value, 100.0);
    }

    #[test]
    fn smart_decimal_starting_with_dot() {
        let results = parse(&Smart, "value:.5");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value, 0.5);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Mixed Formats
    // -------------------------------------------------------------------------

    #[test]
    fn smart_mixed_key_value_and_sequence() {
        // temp is single value, accel is sequence
        let results = parse(&Smart, "temp:25.5 accel: 1, 2, 3");
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 25.5);
        assert_eq!(results[1].series, "accel.0");
        assert_eq!(results[2].series, "accel.1");
        assert_eq!(results[3].series, "accel.2");
    }

    #[test]
    fn smart_underscore_in_name() {
        let results = parse(&Smart, "sensor_temp:32.2 motor_rpm:1500");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].series, "sensor_temp");
        assert_eq!(results[1].series, "motor_rpm");
    }

    #[test]
    fn smart_numbers_in_name() {
        let results = parse(&Smart, "temp1:32.2 temp2:33.1");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].series, "temp1");
        assert_eq!(results[1].series, "temp2");
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Empty and Invalid Input
    // -------------------------------------------------------------------------

    #[test]
    fn smart_empty_input() {
        let results = parse(&Smart, "");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn smart_no_numbers() {
        let results = parse(&Smart, "hello world");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn smart_only_whitespace() {
        let results = parse(&Smart, "   \t\n\r  ");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn smart_label_without_value() {
        // A label followed by non-numeric content
        let results = parse(&Smart, "temp: abc");
        assert_eq!(results.len(), 0);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Duplicate Keys
    // -------------------------------------------------------------------------

    #[test]
    fn smart_duplicate_keys_both_parsed() {
        // Unlike old KeyValue, duplicates should create separate series entries
        // or append to the same series - behavior TBD based on implementation
        let results = parse(&Smart, "temp:25 temp:26");
        assert_eq!(results.len(), 2);
        // Both should be captured (either same name or temp, temp.1)
        assert_eq!(results[0].value, 25.0);
        assert_eq!(results[1].value, 26.0);
    }

    // -------------------------------------------------------------------------
    // Smart Parser Tests - Real-World Examples
    // -------------------------------------------------------------------------

    #[test]
    fn smart_sensor_reading_format() {
        let results = parse(&Smart, "temp:32.2 humidity:57.8 pressure:1017.37\r\n");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[1].series, "humidity");
        assert_eq!(results[2].series, "pressure");
    }

    #[test]
    fn smart_imu_data_format() {
        let results = parse(&Smart, "gyro: 0.12, -0.05, 0.98 accel: 0.01, 9.81, 0.02");
        assert_eq!(results.len(), 6);
        assert_eq!(results[0].series, "gyro.0");
        assert_eq!(results[1].series, "gyro.1");
        assert_eq!(results[2].series, "gyro.2");
        assert_eq!(results[3].series, "accel.0");
        assert_eq!(results[4].series, "accel.1");
        assert_eq!(results[5].series, "accel.2");
    }

    #[test]
    fn smart_arduino_serial_plotter_format() {
        // Arduino Serial Plotter uses space or tab separated values
        let results = parse(&Smart, "100\t200\t300");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].series, "0");
        assert_eq!(results[1].series, "1");
        assert_eq!(results[2].series, "2");
    }

    #[test]
    fn smart_log_line_with_timestamp() {
        // Should extract numbers even with extra text
        let results = parse(&Smart, "[2024-01-15 10:30:45] temp=25.5 humidity=60");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].series, "temp");
        assert_eq!(results[0].value, 25.5);
        assert_eq!(results[1].series, "humidity");
        assert_eq!(results[1].value, 60.0);
    }
}
