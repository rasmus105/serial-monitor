//! Graph view state and messages.

use std::collections::HashMap;
use std::time::Duration;

use serial_core::{
    SessionHandle,
    buffer::graph::{Csv, GraphMode, GraphParserType, Json, Regex, Smart},
};

/// Messages for the graph view.
#[derive(Debug, Clone)]
pub enum GraphMsg {
    // Mode & Parser
    SetMode(usize),
    SetParserType(usize),
    SetRegexPattern(String),
    SetCsvDelimiter(usize),
    SetCsvColumns(String),
    ApplyParserChanges,

    // Direction filters
    ToggleParseRx,
    ToggleParseTx,

    // Time range
    SetTimeRange(usize),
    SetCustomTimeValue(String),
    SetCustomTimeUnit(usize),

    // Series
    ToggleSeriesVisibility(String),

    // Interaction
    ChartHover(Option<iced::Point>),
    ResetView,
}

/// Graph view state.
#[derive(Debug)]
pub struct GraphView {
    /// Graph configuration.
    pub config: GraphConfig,
    /// Hovered point position (canvas coordinates).
    pub hover_position: Option<iced::Point>,
    /// Cached series visibility (local UI state, synced with core).
    pub series_visibility: HashMap<String, bool>,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            config: GraphConfig::default(),
            hover_position: None,
            series_visibility: HashMap::new(),
        }
    }
}

impl GraphView {
    /// Handle a graph message, updating state and core as needed.
    pub fn update(&mut self, msg: GraphMsg, handle: &SessionHandle) {
        match msg {
            GraphMsg::SetMode(index) => {
                self.config.mode_index = index;
            }
            GraphMsg::SetParserType(index) => {
                self.config.parser_type_index = index;
            }
            GraphMsg::SetRegexPattern(pattern) => {
                self.config.regex_pattern = pattern;
            }
            GraphMsg::SetCsvDelimiter(index) => {
                self.config.csv_delimiter_index = index;
            }
            GraphMsg::SetCsvColumns(columns) => {
                self.config.csv_columns = columns;
            }
            GraphMsg::ApplyParserChanges => {
                self.apply_parser_changes(handle);
            }
            GraphMsg::ToggleParseRx => {
                self.config.parse_rx = !self.config.parse_rx;
                self.apply_direction_changes(handle);
            }
            GraphMsg::ToggleParseTx => {
                self.config.parse_tx = !self.config.parse_tx;
                self.apply_direction_changes(handle);
            }
            GraphMsg::SetTimeRange(index) => {
                self.config.time_range_index = index;
            }
            GraphMsg::SetCustomTimeValue(value) => {
                if let Ok(v) = value.parse() {
                    self.config.custom_time_value = v;
                }
            }
            GraphMsg::SetCustomTimeUnit(index) => {
                self.config.custom_time_unit_index = index;
            }
            GraphMsg::ToggleSeriesVisibility(series_name) => {
                // Toggle local visibility tracking
                let visible = self
                    .series_visibility
                    .entry(series_name.clone())
                    .or_insert(true);
                *visible = !*visible;

                // Sync with core
                let mut buffer = handle.buffer_mut();
                if let Some(graph) = buffer.graph_mut() {
                    if let Some(series) = graph.series.get_mut(&series_name) {
                        series.visible = !series.visible;
                    }
                }
            }
            GraphMsg::ChartHover(pos) => {
                self.hover_position = pos;
            }
            GraphMsg::ResetView => {
                // Reset any view transformations (future: zoom/pan state)
            }
        }
    }

    /// Apply parser configuration changes to the core.
    fn apply_parser_changes(&self, handle: &SessionHandle) {
        if let Some(parser) = self.config.build_parser() {
            let mut buffer = handle.buffer_mut();
            buffer.set_graph_parser(parser);
            buffer.set_graph_parse_directions(self.config.parse_rx, self.config.parse_tx);
        }
    }

    /// Apply direction filter changes to the core.
    fn apply_direction_changes(&self, handle: &SessionHandle) {
        handle
            .buffer_mut()
            .set_graph_parse_directions(self.config.parse_rx, self.config.parse_tx);
    }

    /// Sync local series visibility with core graph engine.
    pub fn sync_series_visibility(&mut self, handle: &SessionHandle) {
        let buffer = handle.buffer();
        if let Some(graph) = buffer.graph() {
            // Add any new series we haven't seen
            for (name, series) in &graph.series {
                self.series_visibility
                    .entry(name.clone())
                    .or_insert(series.visible);
            }
            // Remove series that no longer exist
            self.series_visibility
                .retain(|name, _| graph.series.contains_key(name));
        }
    }
}

/// Graph configuration.
#[derive(Debug, Clone)]
pub struct GraphConfig {
    /// Graph mode: 0=Parsed Data, 1=RX/TX Rate
    pub mode_index: usize,

    // --- Parsed Data mode options ---
    /// Parser type: 0=Smart, 1=CSV, 2=JSON, 3=Regex
    pub parser_type_index: usize,
    /// Regex pattern (for Regex parser)
    pub regex_pattern: String,
    /// CSV delimiter index: 0=Comma, 1=Semicolon, 2=Tab, 3=Space, 4=Pipe
    pub csv_delimiter_index: usize,
    /// CSV column names (comma-separated input string)
    pub csv_columns: String,
    /// Parse RX (received) data for graphing
    pub parse_rx: bool,
    /// Parse TX (transmitted) data for graphing
    pub parse_tx: bool,

    // --- RX/TX Rate mode options ---
    /// Show RX rate
    pub show_rx_rate: bool,
    /// Show TX rate
    pub show_tx_rate: bool,

    // --- Time range options (both modes) ---
    /// Time range preset: 0=All, 1=1 Hour, 2=5 Min, 3=Custom
    pub time_range_index: usize,
    /// Custom time value (used when time_range_index == 3)
    pub custom_time_value: usize,
    /// Custom time unit: 0=seconds, 1=minutes, 2=hours
    pub custom_time_unit_index: usize,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            mode_index: 0,        // Parsed Data
            parser_type_index: 0, // Smart
            regex_pattern: String::new(),
            csv_delimiter_index: 0, // Comma
            csv_columns: String::new(),
            parse_rx: true,
            parse_tx: false,
            show_rx_rate: true,
            show_tx_rate: true,
            time_range_index: 0, // All
            custom_time_value: 60,
            custom_time_unit_index: 1, // minutes
        }
    }
}

impl GraphConfig {
    /// Get the current graph mode.
    pub fn mode(&self) -> GraphMode {
        match self.mode_index {
            0 => GraphMode::ParsedData,
            1 => GraphMode::PacketRate,
            _ => GraphMode::ParsedData,
        }
    }

    /// Get the CSV delimiter character.
    pub fn csv_delimiter(&self) -> char {
        match self.csv_delimiter_index {
            0 => ',',
            1 => ';',
            2 => '\t',
            3 => ' ',
            4 => '|',
            _ => ',',
        }
    }

    /// Parse CSV column names from the input string.
    pub fn csv_column_names(&self) -> Vec<String> {
        if self.csv_columns.is_empty() {
            Vec::new()
        } else {
            self.csv_columns
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    /// Build the parser from current config.
    pub fn build_parser(&self) -> Option<GraphParserType> {
        match self.parser_type_index {
            0 => Some(GraphParserType::Smart(Smart)),
            1 => Some(GraphParserType::Csv(Csv {
                delimiter: self.csv_delimiter(),
                column_names: self.csv_column_names(),
            })),
            2 => Some(GraphParserType::Json(Json)),
            3 => {
                // Regex - only build if pattern is valid
                if self.regex_pattern.is_empty() {
                    None
                } else {
                    Regex::new(&self.regex_pattern)
                        .ok()
                        .map(GraphParserType::Regex)
                }
            }
            _ => None,
        }
    }

    /// Get the time range as a Duration, or None for "All".
    pub fn time_range(&self) -> Option<Duration> {
        match self.time_range_index {
            0 => None,                            // All
            1 => Some(Duration::from_secs(3600)), // 1 hour
            2 => Some(Duration::from_secs(300)),  // 5 min
            3 => {
                // Custom
                let multiplier = match self.custom_time_unit_index {
                    0 => 1,    // seconds
                    1 => 60,   // minutes
                    2 => 3600, // hours
                    _ => 60,
                };
                Some(Duration::from_secs(
                    self.custom_time_value as u64 * multiplier,
                ))
            }
            _ => None,
        }
    }
}
