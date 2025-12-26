use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, Display};

use crate::DataChunk;

#[enum_dispatch(GraphParser)]
#[derive(Display, AsRefStr)]
pub enum GraphParserType {
    /// Extract key-value patterns (e.g. `key=value`, `key: value`, etc.)
    #[strum(serialize = "Key Value")]
    KeyValue,
    /// User-defined patterns with capture groups
    #[strum(serialize = "Regex")]
    Regex,
    /// Parse comma-separated values
    #[strum(serialize = "CSV")]
    Csv,
    /// Parse JSON data
    #[strum(serialize = "JSON")]
    Json,
    /// Parse raw numbers found.
    #[strum(serialize = "Raw Numbers")]
    RawNumbers,
}

/// A parsed value with series name.
pub struct ParsedValue {
    /// Name of the series this value belongs to. Each parser
    /// is free to use it's own naming scheme. E.g., the "raw numbers"
    /// parser could use "1", "2", etc. for each value on a line,
    /// while the key-value parser uses actual names of right before values.
    pub series: String,

    /// Unifying all parsed data to 1 data type, to easily
    /// allow displaying different data on the same graph.
    pub value: f64,
}

/// Should implement `Send` and `Sync` to allow front-ends to do
/// async parsing.
#[enum_dispatch]
pub trait GraphParser: Send + Sync {
    /// Parse a chunk of data.
    fn parse(&self, chunk: &DataChunk) -> Vec<ParsedValue>;
}

// ============================================================================
// Key Value Parser
// ============================================================================

pub struct KeyValue {}

impl GraphParser for KeyValue {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        Vec::new()
    }
}

// ============================================================================
// Regex Parser
// ============================================================================

pub struct Regex {}

impl GraphParser for Regex {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        Vec::new()
    }
}

// ============================================================================
// CSV Parser
// ============================================================================

pub struct Csv {}

impl GraphParser for Csv {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        Vec::new()
    }
}

// ============================================================================
// JSON Parser
// ============================================================================

pub struct Json {}

impl GraphParser for Json {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        Vec::new()
    }
}

// ============================================================================
// Raw Numbers Parser
// ============================================================================

pub struct RawNumbers {}

impl GraphParser for RawNumbers {
    fn parse(&self, _chunk: &DataChunk) -> Vec<ParsedValue> {
        Vec::new()
    }
}
