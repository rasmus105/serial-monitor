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
    buffer::graph::{GraphMode, GraphParserType, Csv, KeyValue, Json, RawNumbers, Regex},
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible, always_enabled},
    },
};

use crate::{app::Focus, theme::Theme, widget::{handle_config_key, ConnectionPanel}};

/// Helper to convert SystemTime to seconds since reference.
fn time_to_secs(time: SystemTime, reference: SystemTime) -> f64 {
    time.duration_since(reference)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Graph view state.
pub struct GraphView {
    /// Selected series index (for toggling visibility in series list).
    pub selected_series: usize,
    /// Graph config.
    pub config: GraphConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// Reference time for converting SystemTime to seconds.
    pub reference_time: Option<SystemTime>,
    /// Cached graph data for rendering (avoids allocation per frame).
    /// Vec of (series_name, data_points) pairs.
    cached_graph_data: Vec<Vec<(f64, f64)>>,
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
    /// Parser type: 0=KeyValue, 1=CSV, 2=JSON, 3=Regex, 4=Raw Numbers
    pub parser_type_index: usize,
    /// Regex pattern (for Regex parser)
    pub regex_pattern: String,
    /// CSV delimiter index: 0=Comma, 1=Semicolon, 2=Tab, 3=Space, 4=Pipe
    pub csv_delimiter_index: usize,
    /// CSV column names (comma-separated input string)
    pub csv_columns: String,
    
    // --- RX/TX Rate mode options ---
    /// Show RX rate
    pub show_rx: bool,
    /// Show TX rate  
    pub show_tx: bool,
    
    // --- Time range options (both modes) ---
    /// Time range preset: 0=All, 1=1h, 2=30m, 3=10m, 4=5m, 5=1m, 6=Custom
    pub time_range_index: usize,
    /// Custom time value (used when time_range_index == 6)
    pub custom_time_value: usize,
    /// Custom time unit: 0=seconds, 1=minutes, 2=hours
    pub custom_time_unit_index: usize,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            mode_index: 0, // Parsed Data
            parser_type_index: 0, // KeyValue
            regex_pattern: String::new(),
            csv_delimiter_index: 0, // Comma
            csv_columns: String::new(),
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
            0 => Some(GraphParserType::KeyValue(KeyValue)),
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
            4 => Some(GraphParserType::RawNumbers(RawNumbers)),
            _ => None,
        }
    }
    
    /// Get the time range as a Duration, or None for "All".
    pub fn time_range(&self) -> Option<Duration> {
        match self.time_range_index {
            0 => None, // All
            1 => Some(Duration::from_secs(3600)), // 1 hour
            2 => Some(Duration::from_secs(1800)), // 30 min
            3 => Some(Duration::from_secs(600)),  // 10 min
            4 => Some(Duration::from_secs(300)),  // 5 min
            5 => Some(Duration::from_secs(60)),   // 1 min
            6 => {
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

const MODE_OPTIONS: &[&str] = &["Parsed Data", "RX/TX Rate"];
const PARSER_TYPE_OPTIONS: &[&str] = &["Key=Value", "CSV", "JSON", "Regex", "Raw Numbers"];
const CSV_DELIMITER_OPTIONS: &[&str] = &["Comma (,)", "Semicolon (;)", "Tab", "Space", "Pipe (|)"];
const TIME_RANGE_OPTIONS: &[&str] = &["All", "1 hour", "30 min", "10 min", "5 min", "1 min", "Custom"];
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
                enabled: |c| c.time_range_index == 6, // Custom
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
                enabled: |c| c.time_range_index == 6, // Custom
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
                visible: always_visible,
                enabled: |c| c.mode_index == 0 && c.parser_type_index == 3, // Parsed Data + Regex
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
                visible: always_visible,
                enabled: |c| c.mode_index == 0 && c.parser_type_index == 1, // Parsed Data + CSV
                parent_id: Some("parser_type"),
                validate: always_valid,
            },
            FieldDef {
                id: "csv_columns",
                label: "CSV Columns",
                kind: FieldKind::TextInput { placeholder: "temp,humidity,..." },
                get: |c| FieldValue::String(Cow::Owned(c.csv_columns.clone())),
                set: |c, v| { if let FieldValue::String(s) = v { c.csv_columns = s.into_owned(); } },
                visible: always_visible,
                enabled: |c| c.mode_index == 0 && c.parser_type_index == 1, // Parsed Data + CSV
                parent_id: Some("parser_type"),
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

impl GraphView {
    pub fn new() -> Self {
        Self {
            selected_series: 0,
            config: GraphConfig::default(),
            config_nav: ConfigPanelNav::new(),
            reference_time: None,
            cached_graph_data: Vec::new(),
        }
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
                GraphMode::ParsedData => " Graph (Parsed Data) ",
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

        // Config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, handle, serial_config, focus);
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
                    .filter(|p| cutoff.map_or(true, |c| p.timestamp >= c))
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
            if cutoff.map_or(true, |c| sample_time >= c) {
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
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let buffer = handle.buffer();
        
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
                Constraint::Length(8),           // Connection
                Constraint::Min(5),              // Settings
                Constraint::Length(series_height), // Series (if any)
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
        let config_block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Config {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        ConfigPanel::new(GRAPH_CONFIG_SECTIONS, &self.config, &self.config_nav)
            .block(config_block)
            .focused(focus == Focus::Config)
            .render(chunks[1], buf);

        // Series visibility section (only for Parsed Data mode)
        if self.config.mode_index == 0 && series_height > 0 {
            self.draw_series_section(chunks[2], buf, &buffer, focus);
        }
    }

    fn draw_series_section(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buffer: &serial_core::DataBuffer,
        _focus: Focus,
    ) {
        let block = Block::default()
            .title(" Series ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

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
            let is_selected = i == self.selected_series;
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

            // Highlight selected row
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

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus, handle: &SessionHandle) {
        match focus {
            Focus::Main => self.handle_main_key(key, handle),
            Focus::Config => self.handle_config_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent, handle: &SessionHandle) {
        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('g') => {
                // Toggle graph enable/disable
                let mut buffer = handle.buffer_mut();
                if buffer.graph_enabled() {
                    buffer.disable_graph();
                } else {
                    buffer.enable_graph();
                }
            }
            KeyCode::Char('t') | KeyCode::Enter | KeyCode::Char(' ') => {
                // Toggle series visibility (only in Parsed Data mode)
                if self.config.mode_index == 0 {
                    let mut buffer = handle.buffer_mut();
                    if let Some(graph) = buffer.graph_mut() {
                        let series_names: Vec<String> = graph.series.keys().cloned().collect();
                        if let Some(name) = series_names.get(self.selected_series) {
                            if let Some(series) = graph.series.get_mut(name) {
                                series.visible = !series.visible;
                            }
                        }
                    }
                }
            }
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                // Navigate series (only in Parsed Data mode)
                if self.config.mode_index == 0 {
                    let series_count = handle.buffer().graph()
                        .map(|g| g.series.len())
                        .unwrap_or(0);
                    if series_count > 0 {
                        self.selected_series = (self.selected_series + 1) % series_count;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                // Navigate series (only in Parsed Data mode)
                if self.config.mode_index == 0 {
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
            }
            _ => {}
        }
    }

    fn handle_config_key(&mut self, key: KeyEvent) {
        let _ = handle_config_key(
            key,
            &mut self.config_nav,
            GRAPH_CONFIG_SECTIONS,
            &mut self.config,
        );
        // Graph view doesn't need to sync to buffer or request clear
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

use crate::widget::ConfigPanel;
