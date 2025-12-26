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

/// Extracts `key=value` or `key: value` patterns.
#[derive(Debug, Clone, Default)]
pub struct KeyValue;

impl GraphParser for KeyValue {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        todo!()
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
