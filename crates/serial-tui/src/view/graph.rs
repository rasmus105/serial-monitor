//! Graph view: visualization of parsed data series.

use std::borrow::Cow;
use std::time::{Duration, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Widget,
    },
};
use serial_core::{
    SerialConfig, SessionHandle,
    buffer::graph::{GraphMode, GraphParserType, Csv, Json, Regex, Smart},
    ui::{
        config::{ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible, always_enabled},
    },
};

use crate::{app::{Focus, GraphAction}, theme::Theme, widget::{handle_config_key, ConfigKeyResult, ConnectionPanel, LoadingState, Toast}};

/// Helper to convert SystemTime to seconds since reference.
fn time_to_secs(time: SystemTime, reference: SystemTime) -> f64 {
    time.duration_since(reference)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Sub-focus within the config panel (Settings section vs Series section).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigSubFocus {
    #[default]
    Settings,
    Series,
}

/// Tracks the last applied parser configuration to detect changes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedParserConfig {
    parser_type_index: usize,
    regex_pattern: String,
    csv_delimiter_index: usize,
    csv_columns: String,
    parse_rx: bool,
    parse_tx: bool,
}

impl AppliedParserConfig {
    fn from_config(config: &GraphConfig) -> Self {
        Self {
            parser_type_index: config.parser_type_index,
            regex_pattern: config.regex_pattern.clone(),
            csv_delimiter_index: config.csv_delimiter_index,
            csv_columns: config.csv_columns.clone(),
            parse_rx: config.parse_rx,
            parse_tx: config.parse_tx,
        }
    }
}

/// Graph view state.
pub struct GraphView {
    /// Selected series index (for toggling visibility in series list).
    pub selected_series: usize,
    /// Graph config.
    pub config: GraphConfig,
    /// Config panel navigation.
    pub config_nav: ConfigNav,
    /// Sub-focus within config panel (Settings vs Series).
    pub config_sub_focus: ConfigSubFocus,
    /// Cached graph data for rendering (avoids allocation per frame).
    /// Vec of (series_name, data_points) pairs.
    cached_graph_data: Vec<Vec<(f64, f64)>>,
    /// Last applied parser configuration (to detect changes).
    last_applied_parser: Option<AppliedParserConfig>,
    /// Loading state for reparse operations.
    pub loading: Option<crate::widget::LoadingState>,
}

// =============================================================================
// Graph Configuration
// =============================================================================

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
    pub show_rx: bool,
    /// Show TX rate  
    pub show_tx: bool,
    
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
            mode_index: 0, // Parsed Data
            parser_type_index: 0, // Smart
            regex_pattern: String::new(),
            csv_delimiter_index: 0, // Comma
            csv_columns: String::new(),
            parse_rx: true,
            parse_tx: false,
            show_rx: true,
            show_tx: true,
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
            0 => None, // All
            1 => Some(Duration::from_secs(3600)), // 1 hour
            2 => Some(Duration::from_secs(300)),  // 5 min
            3 => {
                // Custom
                let multiplier = match self.custom_time_unit_index {
                    0 => 1,      // seconds
                    1 => 60,     // minutes
                    2 => 3600,   // hours
                    _ => 60,
                };
                Some(Duration::from_secs(self.custom_time_value as u64 * multiplier))
            }
            _ => None,
        }
    }
}

// =============================================================================
// Config Panel Definitions
// =============================================================================

const MODE_OPTIONS: &[&str] = &["Parse Data", "RX/TX Rate"];
const PARSER_TYPE_OPTIONS: &[&str] = &["Smart", "CSV", "JSON", "Regex"];
const CSV_DELIMITER_OPTIONS: &[&str] = &["Comma (,)", "Semicolon (;)", "Tab", "Space", "Pipe (|)"];
const TIME_RANGE_OPTIONS: &[&str] = &["All", "1 Hour", "5 Min", "Custom"];
const TIME_UNIT_OPTIONS: &[&str] = &["seconds", "minutes", "hours"];

static GRAPH_CONFIG_SECTIONS: &[Section<GraphConfig>] = &[
    Section {
        header: Some("Display"),
        fields: &[
            FieldDef {
                id: "mode",
                label: "Mode",
                kind: FieldKind::Select { options: MODE_OPTIONS },
                get: |c| FieldValue::OptionIndex(c.mode_index),
                set: |c, v| { if let FieldValue::OptionIndex(i) = v { c.mode_index = i; } },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "time_range",
                label: "Time Range",
                kind: FieldKind::Select { options: TIME_RANGE_OPTIONS },
                get: |c| FieldValue::OptionIndex(c.time_range_index),
                set: |c, v| { if let FieldValue::OptionIndex(i) = v { c.time_range_index = i; } },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "custom_time_value",
                label: "Custom Value",
                kind: FieldKind::NumericInput { min: Some(1), max: Some(9999) },
                get: |c| FieldValue::Usize(c.custom_time_value),
                set: |c, v| { if let FieldValue::Usize(n) = v { c.custom_time_value = n; } },
                visible: always_visible,
                enabled: |c| c.time_range_index == 3, // Custom
                parent_id: Some("time_range"),
                validate: always_valid,
            },
            FieldDef {
                id: "custom_time_unit",
                label: "Custom Unit",
                kind: FieldKind::Select { options: TIME_UNIT_OPTIONS },
                get: |c| FieldValue::OptionIndex(c.custom_time_unit_index),
                set: |c, v| { if let FieldValue::OptionIndex(i) = v { c.custom_time_unit_index = i; } },
                visible: always_visible,
                enabled: |c| c.time_range_index == 3, // Custom
                parent_id: Some("time_range"),
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Parser"),
        fields: &[
            FieldDef {
                id: "parser_type",
                label: "Parser Type",
                kind: FieldKind::Select { options: PARSER_TYPE_OPTIONS },
                get: |c| FieldValue::OptionIndex(c.parser_type_index),
                set: |c, v| { if let FieldValue::OptionIndex(i) = v { c.parser_type_index = i; } },
                visible: always_visible,
                enabled: |c| c.mode_index == 0, // Parsed Data mode
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "regex_pattern",
                label: "Regex Pattern",
                kind: FieldKind::TextInput { placeholder: "(?P<temp>\\d+\\.?\\d*)" },
                get: |c| FieldValue::String(Cow::Owned(c.regex_pattern.clone())),
                set: |c, v| { if let FieldValue::String(s) = v { c.regex_pattern = s.into_owned(); } },
                visible: |c| c.mode_index == 0 && c.parser_type_index == 3, // Parse Data + Regex
                enabled: always_enabled,
                parent_id: Some("parser_type"),
                validate: |v| {
                    if let FieldValue::String(s) = v {
                        if s.is_empty() || Regex::new(s).is_ok() {
                            Ok(())
                        } else {
                            Err(Cow::Borrowed("Invalid regex pattern"))
                        }
                    } else {
                        Ok(())
                    }
                },
            },
            FieldDef {
                id: "csv_delimiter",
                label: "CSV Delimiter",
                kind: FieldKind::Select { options: CSV_DELIMITER_OPTIONS },
                get: |c| FieldValue::OptionIndex(c.csv_delimiter_index),
                set: |c, v| { if let FieldValue::OptionIndex(i) = v { c.csv_delimiter_index = i; } },
                visible: |c| c.mode_index == 0 && c.parser_type_index == 1, // Parse Data + CSV
                enabled: always_enabled,
                parent_id: Some("parser_type"),
                validate: always_valid,
            },
            FieldDef {
                id: "csv_columns",
                label: "CSV Columns",
                kind: FieldKind::TextInput { placeholder: "temp,humidity,..." },
                get: |c| FieldValue::String(Cow::Owned(c.csv_columns.clone())),
                set: |c, v| { if let FieldValue::String(s) = v { c.csv_columns = s.into_owned(); } },
                visible: |c| c.mode_index == 0 && c.parser_type_index == 1, // Parse Data + CSV
                enabled: always_enabled,
                parent_id: Some("parser_type"),
                validate: always_valid,
            },
            FieldDef {
                id: "parse_rx",
                label: "Parse RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.parse_rx),
                set: |c, v| { if let FieldValue::Bool(b) = v { c.parse_rx = b; } },
                visible: always_visible,
                enabled: |c| c.mode_index == 0, // Parsed Data mode
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "parse_tx",
                label: "Parse TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.parse_tx),
                set: |c, v| { if let FieldValue::Bool(b) = v { c.parse_tx = b; } },
                visible: always_visible,
                enabled: |c| c.mode_index == 0, // Parsed Data mode
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Rate Display"),
        fields: &[
            FieldDef {
                id: "show_rx",
                label: "Show RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_rx),
                set: |c, v| { if let FieldValue::Bool(b) = v { c.show_rx = b; } },
                visible: always_visible,
                enabled: |c| c.mode_index == 1, // RX/TX Rate mode
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "show_tx",
                label: "Show TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_tx),
                set: |c, v| { if let FieldValue::Bool(b) = v { c.show_tx = b; } },
                visible: always_visible,
                enabled: |c| c.mode_index == 1, // RX/TX Rate mode
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
];

impl Default for GraphView {
    fn default() -> Self {
        Self {
            selected_series: 0,
            config: GraphConfig::default(),
            config_nav: ConfigNav::new(),
            config_sub_focus: ConfigSubFocus::default(),
            cached_graph_data: Vec::new(),
            last_applied_parser: None,
            loading: None,
        }
    }
}

impl GraphView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        handle: &SessionHandle,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let buffer = handle.buffer();
        let mode = self.config.mode();
        let time_range = self.config.time_range();

        // Draw chart area
        let chart_block = Block::default()
            .title(match mode {
                GraphMode::ParsedData => " Graph (Parse Data) ",
                GraphMode::PacketRate => " Graph (RX/TX Rate) ",
            })
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let chart_inner = chart_block.inner(main_area);
        chart_block.render(main_area, buf);

        match mode {
            GraphMode::ParsedData => {
                self.draw_parsed_data_chart(chart_inner, buf, &buffer, time_range);
            }
            GraphMode::PacketRate => {
                self.draw_packet_rate_chart(chart_inner, buf, &buffer, time_range);
            }
        }

        // Config panel - pass buffer reference to avoid RwLock deadlock
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, handle, &buffer, serial_config, focus);
        }
    }

    fn draw_parsed_data_chart(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        buffer: &serial_core::DataBuffer,
        time_range: Option<Duration>,
    ) {
        // Check if graph is enabled
        let Some(graph) = buffer.graph() else {
            let msg = Line::from(vec![
                Span::raw("Graph not enabled. Press "),
                Span::styled("g", Theme::keybind()),
                Span::raw(" to enable graph parsing."),
            ]);
            Paragraph::new(msg)
                .style(Theme::muted())
                .render(area, buf);
            return;
        };

        if graph.series.is_empty() {
            let msg = "No data series found. Ensure data contains parseable values.";
            Paragraph::new(msg)
                .style(Theme::muted())
                .render(area, buf);
            return;
        }

        // Find reference time (earliest timestamp)
        let reference_time = graph
            .series
            .values()
            .flat_map(|s| s.points.front())
            .map(|p| p.timestamp)
            .min()
            .unwrap_or_else(SystemTime::now);

        let now = SystemTime::now();
        let cutoff = time_range.map(|d| now.checked_sub(d).unwrap_or(reference_time));

        // Build datasets - reuse cached storage
        let colors = [
            Theme::PRIMARY,
            Theme::SUCCESS,
            Theme::WARNING,
            Theme::ERROR,
            Theme::ACCENT,
        ];

        // Collect visible series data into cache
        let visible_series: Vec<_> = graph
            .series
            .iter()
            .enumerate()
            .filter(|(_, (_, series))| series.visible)
            .collect();

        // Resize cache to match number of visible series
        self.cached_graph_data.resize(visible_series.len(), Vec::new());

        // Fill cache with data points, filtering by time range
        for (cache_idx, (_, (_, series))) in visible_series.iter().enumerate() {
            let cache = &mut self.cached_graph_data[cache_idx];
            cache.clear();
            cache.extend(
                series
                    .points
                    .iter()
                    .filter(|p| cutoff.is_none_or(|c| p.timestamp >= c))
                    .map(|p| (time_to_secs(p.timestamp, reference_time), p.value)),
            );
        }

        // Build datasets referencing cached data
        let datasets: Vec<Dataset> = visible_series
            .iter()
            .enumerate()
            .map(|(cache_idx, (series_idx, (name, _)))| {
                Dataset::default()
                    .name(name.as_str())
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(colors[*series_idx % colors.len()]))
                    .data(&self.cached_graph_data[cache_idx])
            })
            .collect();

        if datasets.is_empty() {
            return;
        }

        // Calculate bounds from visible/filtered data
        let all_x: Vec<f64> = self.cached_graph_data.iter()
            .flat_map(|pts| pts.iter().map(|(x, _)| *x))
            .collect();

        let (x_min, x_max) = if all_x.is_empty() {
            (0.0, 60.0)
        } else {
            let min = all_x.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            (min, max.max(min + 1.0))
        };

        let all_y: Vec<f64> = self.cached_graph_data.iter()
            .flat_map(|pts| pts.iter().map(|(_, y)| *y))
            .collect();

        let (y_min, y_max) = if all_y.is_empty() {
            (0.0, 100.0)
        } else {
            let min = all_y.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = all_y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let margin = (max - min).abs() * 0.1;
            (min - margin, max + margin)
        };

        let x_axis = Axis::default()
            .title("Time (s)")
            .style(Theme::muted())
            .bounds([x_min, x_max])
            .labels(vec![
                Span::raw(format!("{:.1}", x_min)),
                Span::raw(format!("{:.1}", x_max)),
            ]);

        let y_axis = Axis::default()
            .title("Value")
            .style(Theme::muted())
            .bounds([y_min, y_max])
            .labels(vec![
                Span::raw(format!("{:.1}", y_min)),
                Span::raw(format!("{:.1}", y_max)),
            ]);

        let chart = Chart::new(datasets)
            .x_axis(x_axis)
            .y_axis(y_axis);

        chart.render(area, buf);
    }

    fn draw_packet_rate_chart(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        buffer: &serial_core::DataBuffer,
        time_range: Option<Duration>,
    ) {
        let Some(graph) = buffer.graph() else {
            let msg = Line::from(vec![
                Span::raw("Graph not enabled. Press "),
                Span::styled("g", Theme::keybind()),
                Span::raw(" to enable."),
            ]);
            Paragraph::new(msg)
                .style(Theme::muted())
                .render(area, buf);
            return;
        };

        let packet_rate = &graph.config.packet_rate;
        if packet_rate.samples.is_empty() {
            Paragraph::new("No packet rate data yet.")
                .style(Theme::muted())
                .render(area, buf);
            return;
        }

        // Get reference time from first sample
        let reference_time = packet_rate.samples.front()
            .map(|s| s.window_start())
            .unwrap_or_else(SystemTime::now);

        let now = SystemTime::now();
        let cutoff = time_range.map(|d| now.checked_sub(d).unwrap_or(reference_time));

        // Build RX and TX rate data
        let show_rx = self.config.show_rx;
        let show_tx = self.config.show_tx;

        // We need 2 data series max (RX and TX)
        self.cached_graph_data.resize(2, Vec::new());
        
        // Clear both first
        self.cached_graph_data[0].clear();
        self.cached_graph_data[1].clear();

        let window_secs = packet_rate.window_size.as_secs_f64();
        for sample in packet_rate.samples.iter() {
            let sample_time = sample.window_start();
            if cutoff.is_none_or(|c| sample_time >= c) {
                let t = time_to_secs(sample_time, reference_time);
                if show_rx {
                    // Packets per second
                    self.cached_graph_data[0].push((t, sample.rx_count as f64 / window_secs));
                }
                if show_tx {
                    self.cached_graph_data[1].push((t, sample.tx_count as f64 / window_secs));
                }
            }
        }

        let mut datasets = Vec::new();
        if show_rx && !self.cached_graph_data[0].is_empty() {
            datasets.push(
                Dataset::default()
                    .name("RX")
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(Theme::SUCCESS))
                    .data(&self.cached_graph_data[0])
            );
        }
        if show_tx && !self.cached_graph_data[1].is_empty() {
            datasets.push(
                Dataset::default()
                    .name("TX")
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(Theme::PRIMARY))
                    .data(&self.cached_graph_data[1])
            );
        }

        if datasets.is_empty() {
            Paragraph::new("No data to display. Enable RX or TX.")
                .style(Theme::muted())
                .render(area, buf);
            return;
        }

        // Calculate bounds
        let all_x: Vec<f64> = self.cached_graph_data[0].iter()
            .chain(self.cached_graph_data[1].iter())
            .map(|(x, _)| *x)
            .collect();
        let all_y: Vec<f64> = self.cached_graph_data[0].iter()
            .chain(self.cached_graph_data[1].iter())
            .map(|(_, y)| *y)
            .collect();

        let (x_min, x_max) = if all_x.is_empty() {
            (0.0, 60.0)
        } else {
            let min = all_x.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            (min, max.max(min + 1.0))
        };

        let (y_min, y_max) = if all_y.is_empty() {
            (0.0, 10.0)
        } else {
            let min = 0.0; // Rate always starts at 0
            let max = all_y.iter().cloned().fold(0.0_f64, f64::max);
            (min, max.max(1.0) * 1.1) // 10% margin on top
        };

        let x_axis = Axis::default()
            .title("Time (s)")
            .style(Theme::muted())
            .bounds([x_min, x_max])
            .labels(vec![
                Span::raw(format!("{:.1}", x_min)),
                Span::raw(format!("{:.1}", x_max)),
            ]);

        let y_axis = Axis::default()
            .title("Packets/s")
            .style(Theme::muted())
            .bounds([y_min, y_max])
            .labels(vec![
                Span::raw(format!("{:.0}", y_min)),
                Span::raw(format!("{:.0}", y_max)),
            ]);

        let chart = Chart::new(datasets)
            .x_axis(x_axis)
            .y_axis(y_axis);

        chart.render(area, buf);
    }

    fn draw_config(
        &self,
        area: Rect,
        buf: &mut Buffer,
        handle: &SessionHandle,
        buffer: &serial_core::DataBuffer,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        // Check if we need to show a hint for pending text changes
        let show_hint = self.has_pending_text_changes();
        let hint_height = if show_hint { 1 } else { 0 };
        
        // Calculate how much space we need for series section
        let series_count = buffer.graph()
            .map(|g| g.series.len())
            .unwrap_or(0);
        let series_height = if self.config.mode_index == 0 && series_count > 0 {
            // Header (2) + series items + 1 for borders
            (series_count as u16 + 3).min(12)
        } else {
            0
        };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),                          // Connection
                Constraint::Min(5),                             // Settings
                Constraint::Length(hint_height),                // Hint (if editing)
                Constraint::Length(series_height),              // Series (if any)
            ])
            .split(area);

        // Connection info with statistics
        let conn_block = Block::default()
            .title(" Connection ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        ConnectionPanel::new(handle.port_name(), serial_config, handle.statistics())
            .block(conn_block)
            .render(chunks[0], buf);

        // Graph config
        let settings_focused = focus == Focus::Config && self.config_sub_focus == ConfigSubFocus::Settings;
        let config_block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(if settings_focused {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        ConfigPanel::new(GRAPH_CONFIG_SECTIONS, &self.config, &self.config_nav)
            .block(config_block)
            .focused(settings_focused)
            .render(chunks[1], buf);

        // Show hint when editing text (press Enter to apply)
        if show_hint {
            let hint = Line::from(vec![
                Span::styled("Press ", Theme::muted()),
                Span::styled("Enter", Theme::keybind()),
                Span::styled(" to apply", Theme::muted()),
            ]);
            Paragraph::new(hint).render(chunks[2], buf);
        }

        // Series visibility section (only for Parsed Data mode)
        if self.config.mode_index == 0 && series_height > 0 {
            self.draw_series_section(chunks[3], buf, buffer, focus);
        }
    }

    fn draw_series_section(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buffer: &serial_core::DataBuffer,
        focus: Focus,
    ) {
        let is_focused = focus == Focus::Config && self.config_sub_focus == ConfigSubFocus::Series;
        
        let block = Block::default()
            .title(" Series ")
            .borders(Borders::ALL)
            .border_style(if is_focused {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 1 {
            return;
        }

        let Some(graph) = buffer.graph() else {
            return;
        };

        // Render series toggles
        let colors = [
            Theme::PRIMARY,
            Theme::SUCCESS,
            Theme::WARNING,
            Theme::ERROR,
            Theme::ACCENT,
        ];

        let mut y = inner.y;
        for (i, (name, series)) in graph.series.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            let visibility = if series.visible { "[x]" } else { "[ ]" };
            let is_selected = i == self.selected_series && is_focused;
            let prefix = if is_selected { "> " } else { "  " };
            let color = colors[i % colors.len()];
            
            let latest = series
                .points
                .back()
                .map(|p| format!(" = {:.2}", p.value))
                .unwrap_or_default();

            // Truncate name if needed
            let max_name_len = (inner.width as usize).saturating_sub(prefix.len() + visibility.len() + latest.len() + 2);
            let display_name = if name.len() > max_name_len {
                format!("{}...", &name[..max_name_len.saturating_sub(3)])
            } else {
                name.clone()
            };

            let line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(visibility, if series.visible { Style::default().fg(color) } else { Theme::muted() }),
                Span::raw(" "),
                Span::styled(display_name, Style::default().fg(color)),
                Span::styled(latest, Theme::muted()),
            ]);

            // Highlight selected row only when focused
            if is_selected {
                for x in inner.x..inner.x + inner.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(Theme::SELECTION);
                    }
                }
            }

            Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
            y += 1;
        }
    }

    /// Returns true if the view is in a mode that captures text input.
    ///
    /// This is used to prevent global keybindings (like 'd' to disconnect)
    /// from being triggered while the user is typing in a text field.
    pub fn is_input_mode(&self) -> bool {
        self.config_nav.edit_mode.is_text_input() || self.config_nav.edit_mode.is_dropdown()
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus, handle: &SessionHandle) -> Option<GraphAction> {
        match focus {
            Focus::Main => {
                self.handle_main_key(key, handle);
                None
            }
            Focus::Config => self.handle_config_key(key, handle),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent, handle: &SessionHandle) {
        if let KeyCode::Char('g') = key.code {
            // Toggle graph enable/disable
            let mut buffer = handle.buffer_mut();
            if buffer.graph_enabled() {
                buffer.disable_graph();
                self.last_applied_parser = None;
            } else {
                // Enable with current parser config
                if let Some(parser) = self.config.build_parser() {
                    buffer.enable_graph_with_parser(parser);
                } else {
                    // Fallback to default if parser can't be built (e.g., invalid regex)
                    buffer.enable_graph();
                }
                // Apply parse direction settings
                buffer.set_graph_parse_directions(self.config.parse_rx, self.config.parse_tx);
                self.last_applied_parser = Some(AppliedParserConfig::from_config(&self.config));
            }
        }
    }

    fn handle_config_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<GraphAction> {
        // Check if we have series to show (for Tab navigation)
        let has_series = self.config.mode_index == 0 
            && handle.buffer().graph().map(|g| !g.series.is_empty()).unwrap_or(false);
        
        // Track if we were editing text before this key
        let was_text_editing = self.config_nav.edit_mode.is_text_input();
        
        // Handle Tab to switch between Settings and Series
        // Apply pending text changes when leaving text edit via Tab
        if key.code == KeyCode::Tab && has_series && !self.config_nav.edit_mode.is_dropdown() {
            if was_text_editing {
                // Apply text edit before switching
                let _ = self.config_nav.apply_text_edit(GRAPH_CONFIG_SECTIONS, &mut self.config);
                self.maybe_apply_parser(handle);
            }
            self.config_sub_focus = match self.config_sub_focus {
                ConfigSubFocus::Settings => ConfigSubFocus::Series,
                ConfigSubFocus::Series => ConfigSubFocus::Settings,
            };
            return None;
        }
        
        // If no series available, force Settings sub-focus
        if !has_series {
            self.config_sub_focus = ConfigSubFocus::Settings;
        }
        
        match self.config_sub_focus {
            ConfigSubFocus::Settings => {
                // Check if navigation is about to leave current field
                let current_field_before = self.config_nav.selected;
                
                let result = handle_config_key(
                    key,
                    &mut self.config_nav,
                    GRAPH_CONFIG_SECTIONS,
                    &mut self.config,
                );
                
                let current_field_after = self.config_nav.selected;
                let navigated_away = current_field_before != current_field_after;
                
                // Apply text changes when navigating away from a text field
                if was_text_editing && navigated_away {
                    // Text edit was implicitly confirmed by navigation
                    self.maybe_apply_parser(handle);
                }
                
                // Handle result
                match result {
                    ConfigKeyResult::Changed => {
                        // Check if this was a dropdown change (immediate apply)
                        // or text edit confirm (also apply)
                        self.maybe_apply_parser(handle);
                        None
                    }
                    ConfigKeyResult::ValidationFailed(msg) => {
                        // Show error toast and keep editing mode active
                        Some(GraphAction::Toast(Toast::error(msg.into_owned())))
                    }
                    _ => None,
                }
            }
            ConfigSubFocus::Series => {
                // Handle series navigation and toggling
                let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                        let series_count = handle.buffer().graph()
                            .map(|g| g.series.len())
                            .unwrap_or(0);
                        if series_count > 0 {
                            self.selected_series = (self.selected_series + 1) % series_count;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                        let series_count = handle.buffer().graph()
                            .map(|g| g.series.len())
                            .unwrap_or(0);
                        if series_count > 0 {
                            self.selected_series = if self.selected_series == 0 {
                                series_count - 1
                            } else {
                                self.selected_series - 1
                            };
                        }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('t') 
                    | KeyCode::Char('h') | KeyCode::Char('l') | KeyCode::Left | KeyCode::Right => {
                        // Toggle series visibility
                        let mut buffer = handle.buffer_mut();
                        if let Some(graph) = buffer.graph_mut() {
                            let series_names: Vec<String> = graph.series.keys().cloned().collect();
                            if let Some(name) = series_names.get(self.selected_series)
                                && let Some(series) = graph.series.get_mut(name)
                            {
                                series.visible = !series.visible;
                            }
                        }
                    }
                    _ => {}
                }
                None
            }
        }
    }
    
    /// Apply parser changes to the core if the config has changed.
    ///
    /// This compares the current config against the last applied config
    /// and triggers a reparse if needed.
    fn maybe_apply_parser(&mut self, handle: &SessionHandle) {
        // Only apply if graph is enabled
        if !handle.buffer().graph_enabled() {
            return;
        }
        
        let current = AppliedParserConfig::from_config(&self.config);
        
        // Check if anything relevant changed
        let needs_update = match &self.last_applied_parser {
            Some(last) => *last != current,
            None => true, // First time, always apply
        };
        
        if !needs_update {
            return;
        }
        
        // Check what kind of update is needed
        let parser_changed = self.last_applied_parser.as_ref().is_none_or(|last| {
            last.parser_type_index != current.parser_type_index
                || last.regex_pattern != current.regex_pattern
                || last.csv_delimiter_index != current.csv_delimiter_index
                || last.csv_columns != current.csv_columns
        });
        
        let direction_changed = self.last_applied_parser.as_ref().is_some_and(|last| {
            last.parse_rx != current.parse_rx || last.parse_tx != current.parse_tx
        });
        
        // Start loading indicator
        self.loading = Some(LoadingState::new("Reparsing data..."));
        
        if parser_changed {
            // Parser changed - need to rebuild everything
            if let Some(parser) = self.config.build_parser() {
                let mut buffer = handle.buffer_mut();
                buffer.set_graph_parser(parser);
                // Also update direction settings
                buffer.set_graph_parse_directions(self.config.parse_rx, self.config.parse_tx);
            }
        } else if direction_changed {
            // Only direction changed - just update directions
            handle.buffer_mut().set_graph_parse_directions(self.config.parse_rx, self.config.parse_tx);
        }
        
        // Update tracking
        self.last_applied_parser = Some(current);
        
        // Clear loading (in a real async scenario, this would be done on completion)
        // For now, since set_graph_parser is synchronous, we clear it
        // but mark_visible won't have been called yet, so it won't flash
        if let Some(ref loading) = self.loading
            && loading.can_dismiss()
        {
            self.loading = None;
        }
    }
    
    /// Check if there are pending text input changes that haven't been applied.
    ///
    /// Returns true if the user is editing a parser-related text field and the
    /// content differs from what's been applied.
    pub fn has_pending_text_changes(&self) -> bool {
        // Only relevant when editing text
        if !self.config_nav.edit_mode.is_text_input() {
            return false;
        }
        
        // Get current field ID to check if it's a parser-related field
        if let Some(field) = self.config_nav.current_field(GRAPH_CONFIG_SECTIONS, &self.config) {
            match field.id {
                "regex_pattern" | "csv_columns" => {
                    // Compare current text buffer with the applied value
                    let buffer = self.config_nav.edit_mode.text_buffer()
                        .map(|b| b.content())
                        .unwrap_or("");
                    match field.id {
                        "regex_pattern" => {
                            self.last_applied_parser
                                .as_ref()
                                .map(|p| p.regex_pattern != buffer)
                                .unwrap_or(true)
                        }
                        "csv_columns" => {
                            self.last_applied_parser
                                .as_ref()
                                .map(|p| p.csv_columns != buffer)
                                .unwrap_or(true)
                        }
                        _ => false,
                    }
                }
                _ => false,
            }
        } else {
            false
        }
    }
    
    /// Dismiss the loading overlay if it can be dismissed.
    pub fn dismiss_loading_if_ready(&mut self) {
        if let Some(ref loading) = self.loading
            && loading.can_dismiss()
        {
            self.loading = None;
        }
    }
}


use crate::widget::ConfigPanel;

// =============================================================================
// Settings integration
// =============================================================================

use crate::settings::GraphSettings;

impl GraphView {
    /// Apply settings loaded from disk.
    pub fn apply_settings(&mut self, settings: &GraphSettings) {
        self.config.mode_index = settings.mode_index;
        self.config.parser_type_index = settings.parser_type_index;
        self.config.regex_pattern = settings.regex_pattern.clone();
        self.config.csv_delimiter_index = settings.csv_delimiter_index;
        self.config.csv_columns = settings.csv_columns.clone();
        self.config.parse_rx = settings.parse_rx;
        self.config.parse_tx = settings.parse_tx;
        self.config.show_rx = settings.show_rx;
        self.config.show_tx = settings.show_tx;
        self.config.time_range_index = settings.time_range_index;
        self.config.custom_time_value = settings.custom_time_value;
        self.config.custom_time_unit_index = settings.custom_time_unit_index;
    }
    
    /// Extract settings for saving to disk.
    pub fn to_settings(&self) -> GraphSettings {
        GraphSettings {
            mode_index: self.config.mode_index,
            parser_type_index: self.config.parser_type_index,
            regex_pattern: self.config.regex_pattern.clone(),
            csv_delimiter_index: self.config.csv_delimiter_index,
            csv_columns: self.config.csv_columns.clone(),
            parse_rx: self.config.parse_rx,
            parse_tx: self.config.parse_tx,
            show_rx: self.config.show_rx,
            show_tx: self.config.show_tx,
            time_range_index: self.config.time_range_index,
            custom_time_value: self.config.custom_time_value,
            custom_time_unit_index: self.config.custom_time_unit_index,
        }
    }
}
