//! Graph parsers
//!
//! Parsers extract numeric values from data chunks for graphing.
//! Each parser implements the [`GraphParser`] trait.

use regex::Regex;

use crate::buffer::DataChunk;

/// A parsed value with its series name
#[derive(Debug, Clone)]
pub struct ParsedValue {
    /// Name of the series this value belongs to
    pub series: String,
    /// The numeric value
    pub value: f64,
}

impl ParsedValue {
    /// Create a new parsed value
    pub fn new(series: impl Into<String>, value: f64) -> Self {
        Self {
            series: series.into(),
            value,
        }
    }
}

/// Available parser types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserType {
    /// Key-value parser: extracts `key=value` or `key: value` patterns
    #[default]
    KeyValue,
    /// Regex parser: user-defined pattern with capture groups
    Regex,
    /// CSV parser: parse comma-separated values
    Csv,
    /// JSON parser: extract numeric fields from JSON
    Json,
    /// Raw number parser: extract any numbers found
    RawNumber,
}

impl ParserType {
    /// Get a human-readable name for the parser type
    pub fn name(&self) -> &'static str {
        match self {
            ParserType::KeyValue => "Key=Value",
            ParserType::Regex => "Regex",
            ParserType::Csv => "CSV",
            ParserType::Json => "JSON",
            ParserType::RawNumber => "Raw Numbers",
        }
    }

    /// Get all parser types
    pub fn all() -> &'static [ParserType] {
        &[
            ParserType::KeyValue,
            ParserType::Regex,
            ParserType::Csv,
            ParserType::Json,
            ParserType::RawNumber,
        ]
    }
}

/// Configuration for creating a parser
#[derive(Debug, Clone)]
pub enum GraphParserConfig {
    /// Key-value parser config
    KeyValue(KeyValueParserConfig),
    /// Regex parser config
    Regex(RegexParserConfig),
    /// CSV parser config
    Csv(CsvParserConfig),
    /// JSON parser config (no extra config needed)
    Json,
    /// Raw number parser config
    RawNumber(RawNumberParserConfig),
}

impl Default for GraphParserConfig {
    fn default() -> Self {
        GraphParserConfig::KeyValue(KeyValueParserConfig::default())
    }
}

impl GraphParserConfig {
    /// Get the parser type for this config
    pub fn parser_type(&self) -> ParserType {
        match self {
            GraphParserConfig::KeyValue(_) => ParserType::KeyValue,
            GraphParserConfig::Regex(_) => ParserType::Regex,
            GraphParserConfig::Csv(_) => ParserType::Csv,
            GraphParserConfig::Json => ParserType::Json,
            GraphParserConfig::RawNumber(_) => ParserType::RawNumber,
        }
    }

    /// Create a parser from this config
    pub fn create_parser(&self) -> Box<dyn GraphParser> {
        match self {
            GraphParserConfig::KeyValue(cfg) => Box::new(KeyValueParser::new(cfg.clone())),
            GraphParserConfig::Regex(cfg) => Box::new(RegexParser::new(cfg.clone())),
            GraphParserConfig::Csv(cfg) => Box::new(CsvParser::new(cfg.clone())),
            GraphParserConfig::Json => Box::new(JsonParser::new()),
            GraphParserConfig::RawNumber(cfg) => Box::new(RawNumberParser::new(cfg.clone())),
        }
    }
}

/// Trait for graph data parsers
///
/// Parsers take a [`DataChunk`] and extract zero or more named numeric values.
pub trait GraphParser: Send + Sync {
    /// Get the name of this parser
    fn name(&self) -> &str;

    /// Parse a chunk and return extracted values
    ///
    /// Returns a list of (series_name, value) pairs. A single chunk can
    /// produce multiple values for multiple series.
    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue>;
}

// ============================================================================
// Key-Value Parser
// ============================================================================

/// Configuration for the key-value parser
#[derive(Debug, Clone)]
pub struct KeyValueParserConfig {
    /// Separators between key and value (e.g., "=" or ":")
    pub key_value_separators: Vec<char>,
    /// Separators between pairs (e.g., "," or " ")
    pub pair_separators: Vec<char>,
}

impl Default for KeyValueParserConfig {
    fn default() -> Self {
        Self {
            key_value_separators: vec!['=', ':'],
            pair_separators: vec![',', ' ', '\t', ';'],
        }
    }
}

/// Parser for key=value or key: value patterns
///
/// Examples:
/// - `temp=25.5` -> ("temp", 25.5)
/// - `temperature: 41.3, humidity: 60` -> [("temperature", 41.3), ("humidity", 60.0)]
pub struct KeyValueParser {
    config: KeyValueParserConfig,
}

impl KeyValueParser {
    /// Create a new key-value parser with default config
    pub fn new(config: KeyValueParserConfig) -> Self {
        Self { config }
    }
}

impl Default for KeyValueParser {
    fn default() -> Self {
        Self::new(KeyValueParserConfig::default())
    }
}

impl GraphParser for KeyValueParser {
    fn name(&self) -> &str {
        "Key=Value"
    }

    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let mut results = Vec::new();

        // Try to decode as UTF-8
        let text = match std::str::from_utf8(&chunk.data) {
            Ok(s) => s,
            Err(_) => return results,
        };

        // For each key-value separator, find all key=value patterns
        for &sep in &self.config.key_value_separators {
            let mut remaining = text;

            while let Some(sep_pos) = remaining.find(sep) {
                // Find the key (scan backwards from separator to find word boundary)
                let before_sep = &remaining[..sep_pos];
                let key = before_sep
                    .trim_end()
                    .split(|c: char| self.config.pair_separators.contains(&c))
                    .last()
                    .unwrap_or("")
                    .trim();

                // Find the value (scan forwards from separator to find end)
                let after_sep = &remaining[sep_pos + sep.len_utf8()..];
                let value_str = after_sep
                    .trim_start()
                    .split(|c: char| self.config.pair_separators.contains(&c))
                    .next()
                    .unwrap_or("")
                    .trim();

                // Try to parse the value as a number
                if !key.is_empty() {
                    if let Ok(value) = value_str.parse::<f64>() {
                        // Avoid duplicates (same key-value might be found by multiple separators)
                        if !results.iter().any(|r: &ParsedValue| r.series == key) {
                            results.push(ParsedValue::new(key, value));
                        }
                    }
                }

                // Move past this separator
                remaining = &remaining[sep_pos + sep.len_utf8()..];
            }
        }

        results
    }
}

// ============================================================================
// Regex Parser
// ============================================================================

/// Configuration for the regex parser
#[derive(Debug, Clone)]
pub struct RegexParserConfig {
    /// The regex pattern with named capture groups
    /// Named groups become series names, captured values should be numeric
    pub pattern: String,
}

impl Default for RegexParserConfig {
    fn default() -> Self {
        Self {
            // Default: match "name: number" or "name=number"
            pattern: r"(?P<key>\w+)[=:]\s*(?P<value>-?\d+\.?\d*)".to_string(),
        }
    }
}

/// Parser using user-defined regex with capture groups
///
/// Named capture groups become series names. The captured value should be numeric.
///
/// Example pattern: `temp=(?P<temperature>\d+\.?\d*)`
pub struct RegexParser {
    config: RegexParserConfig,
    regex: Option<Regex>,
}

impl RegexParser {
    /// Create a new regex parser
    pub fn new(config: RegexParserConfig) -> Self {
        let regex = Regex::new(&config.pattern).ok();
        Self { config, regex }
    }

    /// Check if the regex pattern is valid
    pub fn is_valid(&self) -> bool {
        self.regex.is_some()
    }

    /// Get the pattern
    pub fn pattern(&self) -> &str {
        &self.config.pattern
    }
}

impl GraphParser for RegexParser {
    fn name(&self) -> &str {
        "Regex"
    }

    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let mut results = Vec::new();

        let regex = match &self.regex {
            Some(r) => r,
            None => return results,
        };

        let text = match std::str::from_utf8(&chunk.data) {
            Ok(s) => s,
            Err(_) => return results,
        };

        // Check if we have 'key' and 'value' named groups (generic pattern)
        let has_key_value_groups = regex
            .capture_names()
            .flatten()
            .any(|n| n == "key" || n == "value");

        for caps in regex.captures_iter(text) {
            if has_key_value_groups {
                // Generic key/value pattern
                if let (Some(key_match), Some(value_match)) =
                    (caps.name("key"), caps.name("value"))
                {
                    if let Ok(value) = value_match.as_str().parse::<f64>() {
                        results.push(ParsedValue::new(key_match.as_str(), value));
                    }
                }
            } else {
                // Named groups are series names
                for name in regex.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        if let Ok(value) = m.as_str().parse::<f64>() {
                            results.push(ParsedValue::new(name, value));
                        }
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

/// Configuration for the CSV parser
#[derive(Debug, Clone)]
pub struct CsvParserConfig {
    /// Column delimiter
    pub delimiter: char,
    /// Column names (if known). If empty, uses column indices as names.
    pub column_names: Vec<String>,
    /// Whether the first row is a header row
    pub has_header: bool,
}

impl Default for CsvParserConfig {
    fn default() -> Self {
        Self {
            delimiter: ',',
            column_names: Vec::new(),
            has_header: false,
        }
    }
}

/// Parser for CSV data
///
/// Parses comma-separated (or other delimiter) values, extracting numeric columns.
pub struct CsvParser {
    config: CsvParserConfig,
    /// Cached header from first row (if has_header is true)
    header: Option<Vec<String>>,
}

impl CsvParser {
    /// Create a new CSV parser
    pub fn new(config: CsvParserConfig) -> Self {
        Self {
            config,
            header: None,
        }
    }
}

impl GraphParser for CsvParser {
    fn name(&self) -> &str {
        "CSV"
    }

    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let mut results = Vec::new();

        let text = match std::str::from_utf8(&chunk.data) {
            Ok(s) => s.trim(),
            Err(_) => return results,
        };

        // Split into fields
        let fields: Vec<&str> = text.split(self.config.delimiter).map(|s| s.trim()).collect();

        // Get column names
        let column_names: Vec<String> = if !self.config.column_names.is_empty() {
            self.config.column_names.clone()
        } else if let Some(ref header) = self.header {
            header.clone()
        } else {
            // Use column indices as names
            (0..fields.len()).map(|i| format!("col{}", i)).collect()
        };

        // Parse each field
        for (i, field) in fields.iter().enumerate() {
            if let Ok(value) = field.parse::<f64>() {
                let name = column_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("col{}", i));
                results.push(ParsedValue::new(name, value));
            }
        }

        results
    }
}

// ============================================================================
// JSON Parser
// ============================================================================

/// Parser for JSON data
///
/// Extracts numeric fields from JSON objects.
pub struct JsonParser;

impl JsonParser {
    /// Create a new JSON parser
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphParser for JsonParser {
    fn name(&self) -> &str {
        "JSON"
    }

    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let mut results = Vec::new();

        let text = match std::str::from_utf8(&chunk.data) {
            Ok(s) => s.trim(),
            Err(_) => return results,
        };

        // Simple JSON parsing without external dependencies
        // This is a basic implementation that handles simple cases
        Self::parse_json_values(text, "", &mut results);

        results
    }
}

impl JsonParser {
    /// Recursively parse JSON values and extract numbers
    fn parse_json_values(text: &str, prefix: &str, results: &mut Vec<ParsedValue>) {
        let text = text.trim();

        // Try to parse as a simple number
        if let Ok(value) = text.parse::<f64>() {
            if !prefix.is_empty() {
                results.push(ParsedValue::new(prefix, value));
            }
            return;
        }

        // Handle objects: {"key": value, ...}
        if text.starts_with('{') && text.ends_with('}') {
            let inner = &text[1..text.len() - 1];
            Self::parse_json_object(inner, prefix, results);
            return;
        }

        // Handle arrays: [value, value, ...]
        if text.starts_with('[') && text.ends_with(']') {
            let inner = &text[1..text.len() - 1];
            Self::parse_json_array(inner, prefix, results);
        }
    }

    fn parse_json_object(text: &str, prefix: &str, results: &mut Vec<ParsedValue>) {
        // Very basic JSON object parsing
        // Split by commas that are not inside braces or brackets
        let mut depth = 0;
        let mut current_key: Option<String> = None;
        let mut value_start: Option<usize> = None;
        let mut in_string = false;
        let mut escape_next = false;

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }

            if c == '\\' {
                escape_next = true;
                i += 1;
                continue;
            }

            if c == '"' {
                in_string = !in_string;
                if in_string && current_key.is_none() {
                    // Start of key
                    let key_start = i + 1;
                    // Find end of key
                    let mut key_end = key_start;
                    while key_end < chars.len() {
                        if chars[key_end] == '\\' {
                            key_end += 2;
                            continue;
                        }
                        if chars[key_end] == '"' {
                            break;
                        }
                        key_end += 1;
                    }
                    current_key = Some(chars[key_start..key_end].iter().collect());
                    i = key_end;
                    in_string = false;
                }
                i += 1;
                continue;
            }

            if in_string {
                i += 1;
                continue;
            }

            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ':' if depth == 0 && current_key.is_some() => {
                    value_start = Some(i + 1);
                }
                ',' if depth == 0 => {
                    if let (Some(key), Some(start)) = (current_key.take(), value_start.take()) {
                        let value_text: String = chars[start..i].iter().collect();
                        let full_key = if prefix.is_empty() {
                            key
                        } else {
                            format!("{}.{}", prefix, key)
                        };
                        Self::parse_json_values(&value_text, &full_key, results);
                    }
                }
                _ => {}
            }

            i += 1;
        }

        // Handle last key-value pair
        if let (Some(key), Some(start)) = (current_key, value_start) {
            let value_text: String = chars[start..].iter().collect();
            let full_key = if prefix.is_empty() {
                key
            } else {
                format!("{}.{}", prefix, key)
            };
            Self::parse_json_values(&value_text, &full_key, results);
        }
    }

    fn parse_json_array(text: &str, prefix: &str, results: &mut Vec<ParsedValue>) {
        let mut depth = 0;
        let mut start = 0;
        let mut index = 0;
        let mut in_string = false;
        let mut escape_next = false;

        let chars: Vec<char> = text.chars().collect();

        for (i, &c) in chars.iter().enumerate() {
            if escape_next {
                escape_next = false;
                continue;
            }

            if c == '\\' {
                escape_next = true;
                continue;
            }

            if c == '"' {
                in_string = !in_string;
                continue;
            }

            if in_string {
                continue;
            }

            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    let value_text: String = chars[start..i].iter().collect();
                    let key = if prefix.is_empty() {
                        format!("[{}]", index)
                    } else {
                        format!("{}[{}]", prefix, index)
                    };
                    Self::parse_json_values(&value_text, &key, results);
                    start = i + 1;
                    index += 1;
                }
                _ => {}
            }
        }

        // Handle last element
        if start < chars.len() {
            let value_text: String = chars[start..].iter().collect();
            let key = if prefix.is_empty() {
                format!("[{}]", index)
            } else {
                format!("{}[{}]", prefix, index)
            };
            Self::parse_json_values(&value_text, &key, results);
        }
    }
}

// ============================================================================
// Raw Number Parser
// ============================================================================

/// Configuration for the raw number parser
#[derive(Debug, Clone)]
pub struct RawNumberParserConfig {
    /// Prefix for series names (e.g., "value" -> "value0", "value1", ...)
    pub series_prefix: String,
}

impl Default for RawNumberParserConfig {
    fn default() -> Self {
        Self {
            series_prefix: "value".to_string(),
        }
    }
}

/// Parser that extracts all numbers found in the data
///
/// This is the simplest parser - it just finds any numbers in the text.
pub struct RawNumberParser {
    config: RawNumberParserConfig,
    regex: Regex,
}

impl RawNumberParser {
    /// Create a new raw number parser
    pub fn new(config: RawNumberParserConfig) -> Self {
        // Match integers and floats, including negative numbers
        let regex = Regex::new(r"-?\d+\.?\d*").unwrap();
        Self { config, regex }
    }
}

impl Default for RawNumberParser {
    fn default() -> Self {
        Self::new(RawNumberParserConfig::default())
    }
}

impl GraphParser for RawNumberParser {
    fn name(&self) -> &str {
        "Raw Numbers"
    }

    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue> {
        let mut results = Vec::new();

        let text = match std::str::from_utf8(&chunk.data) {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, m) in self.regex.find_iter(text).enumerate() {
            if let Ok(value) = m.as_str().parse::<f64>() {
                let series_name = format!("{}{}", self.config.series_prefix, i);
                results.push(ParsedValue::new(series_name, value));
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;

    fn make_chunk(data: &str) -> DataChunk {
        DataChunk::new(Direction::Rx, data.as_bytes().to_vec())
    }

    #[test]
    fn test_key_value_parser() {
        let parser = KeyValueParser::default();

        let chunk = make_chunk("temp=25.5, humidity=60");
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].series, "temp");
        assert!((values[0].value - 25.5).abs() < f64::EPSILON);
        assert_eq!(values[1].series, "humidity");
        assert!((values[1].value - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_key_value_parser_colon() {
        let parser = KeyValueParser::default();

        let chunk = make_chunk("temperature: 41.3");
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].series, "temperature");
        assert!((values[0].value - 41.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_regex_parser_named_groups() {
        let config = RegexParserConfig {
            pattern: r"T:(?P<temp>\d+\.?\d*)\s+H:(?P<humidity>\d+\.?\d*)".to_string(),
        };
        let parser = RegexParser::new(config);

        let chunk = make_chunk("T:25.5 H:60");
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| v.series == "temp" && (v.value - 25.5).abs() < f64::EPSILON));
        assert!(
            values
                .iter()
                .any(|v| v.series == "humidity" && (v.value - 60.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn test_csv_parser() {
        let config = CsvParserConfig {
            delimiter: ',',
            column_names: vec!["time".to_string(), "temp".to_string(), "humidity".to_string()],
            has_header: false,
        };
        let parser = CsvParser::new(config);

        let chunk = make_chunk("1000, 25.5, 60");
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 3);
        assert_eq!(values[0].series, "time");
        assert_eq!(values[1].series, "temp");
        assert_eq!(values[2].series, "humidity");
    }

    #[test]
    fn test_json_parser() {
        let parser = JsonParser::new();

        let chunk = make_chunk(r#"{"temp": 25.5, "humidity": 60}"#);
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| v.series == "temp" && (v.value - 25.5).abs() < f64::EPSILON));
        assert!(
            values
                .iter()
                .any(|v| v.series == "humidity" && (v.value - 60.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn test_raw_number_parser() {
        let parser = RawNumberParser::default();

        let chunk = make_chunk("Reading: 25.5 degrees at 60% humidity");
        let values = parser.parse(&chunk);

        assert_eq!(values.len(), 2);
        assert!((values[0].value - 25.5).abs() < f64::EPSILON);
        assert!((values[1].value - 60.0).abs() < f64::EPSILON);
    }
}
