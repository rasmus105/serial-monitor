//! Graph view: visualization of parsed data series.

use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Widget,
    },
};
use serial_core::{
    SerialConfig, SessionHandle,
    buffer::graph::GraphMode,
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
    },
};

use crate::{app::Focus, theme::Theme};

/// Helper to convert SystemTime to seconds since reference.
fn time_to_secs(time: SystemTime, reference: SystemTime) -> f64 {
    time.duration_since(reference)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Graph view state.
pub struct GraphView {
    /// Current graph mode.
    pub mode: GraphMode,
    /// Selected series index (for toggling visibility).
    pub selected_series: usize,
    /// Graph config.
    pub config: GraphConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// X-axis range (seconds).
    pub x_range: (f64, f64),
    /// Whether to auto-scale X axis.
    pub auto_scale_x: bool,
    /// Reference time for converting SystemTime to seconds.
    pub reference_time: Option<SystemTime>,
}

/// Graph configuration.
#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub parser_type_index: usize,
    pub show_packet_rate: bool,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            parser_type_index: 0, // KeyValue
            show_packet_rate: false,
        }
    }
}

const PARSER_TYPE_OPTIONS: &[&str] = &["Key=Value", "CSV", "JSON", "Regex", "Raw Numbers"];

static GRAPH_CONFIG_SECTIONS: &[Section<GraphConfig>] = &[Section {
    header: Some("Parser"),
    fields: &[
        FieldDef {
            id: "parser_type",
            label: "Parser Type",
            kind: FieldKind::Select {
                options: PARSER_TYPE_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.parser_type_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.parser_type_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "show_packet_rate",
            label: "Show Packet Rate",
            kind: FieldKind::Toggle,
            get: |c| FieldValue::Bool(c.show_packet_rate),
            set: |c, v| {
                if let FieldValue::Bool(b) = v {
                    c.show_packet_rate = b;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
    ],
}];

impl GraphView {
    pub fn new() -> Self {
        Self {
            mode: GraphMode::ParsedData,
            selected_series: 0,
            config: GraphConfig::default(),
            config_nav: ConfigPanelNav::new(),
            x_range: (0.0, 60.0),
            auto_scale_x: true,
            reference_time: None,
        }
    }

    pub fn draw(
        &self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        handle: &SessionHandle,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let buffer = handle.buffer();

        // Main area: chart + series list
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(8)])
            .split(main_area);

        // Draw chart area
        let chart_block = Block::default()
            .title(" Graph ")
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let chart_inner = chart_block.inner(main_chunks[0]);
        chart_block.render(main_chunks[0], buf);

        // Check if graph is enabled
        if let Some(graph) = buffer.graph() {
            if graph.series.is_empty() {
                let msg = "No data series found. Ensure data contains parseable values.";
                Paragraph::new(msg)
                    .style(Theme::muted())
                    .render(chart_inner, buf);
            } else {
                // Find reference time (earliest timestamp)
                let reference_time = graph
                    .series
                    .values()
                    .flat_map(|s| s.points.front())
                    .map(|p| p.timestamp)
                    .min()
                    .unwrap_or_else(SystemTime::now);

                // Build datasets
                let colors = [
                    Theme::PRIMARY,
                    Theme::SUCCESS,
                    Theme::WARNING,
                    Theme::ERROR,
                    Theme::ACCENT,
                ];

                let datasets: Vec<Dataset> = graph
                    .series
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, series))| series.visible)
                    .map(|(i, (name, series))| {
                        let data: Vec<(f64, f64)> = series
                            .points
                            .iter()
                            .map(|p| (time_to_secs(p.timestamp, reference_time), p.value))
                            .collect();

                        Dataset::default()
                            .name(name.as_str())
                            .marker(Marker::Braille)
                            .graph_type(GraphType::Line)
                            .style(Style::default().fg(colors[i % colors.len()]))
                            .data(Box::leak(data.into_boxed_slice()))
                    })
                    .collect();

                if !datasets.is_empty() {
                    // Calculate bounds
                    let (x_min, x_max) = if self.auto_scale_x {
                        let all_x: Vec<f64> = graph
                            .series
                            .values()
                            .flat_map(|s| s.points.iter().map(|p| time_to_secs(p.timestamp, reference_time)))
                            .collect();

                        if all_x.is_empty() {
                            (0.0, 60.0)
                        } else {
                            let min = all_x.iter().cloned().fold(f64::INFINITY, f64::min);
                            let max = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                            (min, max.max(min + 1.0))
                        }
                    } else {
                        self.x_range
                    };

                    let all_y: Vec<f64> = graph
                        .series
                        .values()
                        .filter(|s| s.visible)
                        .flat_map(|s| s.points.iter().map(|p| p.value))
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

                    chart.render(chart_inner, buf);
                }
            }
        } else {
            let msg = Line::from(vec![
                Span::raw("Graph not enabled. Press "),
                Span::styled("g", Theme::keybind()),
                Span::raw(" to enable graph parsing."),
            ]);
            Paragraph::new(msg)
                .style(Theme::muted())
                .render(chart_inner, buf);
        }

        // Series list
        let series_block = Block::default()
            .title(" Series ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let series_inner = series_block.inner(main_chunks[1]);
        series_block.render(main_chunks[1], buf);

        if let Some(graph) = buffer.graph() {
            let items: Vec<ListItem> = graph
                .series
                .iter()
                .enumerate()
                .map(|(i, (name, series))| {
                    let visibility = if series.visible { "[x]" } else { "[ ]" };
                    let selected = if i == self.selected_series { "> " } else { "  " };
                    let latest = series
                        .points
                        .back()
                        .map(|p| format!("{:.2}", p.value))
                        .unwrap_or_else(|| "N/A".to_string());

                    ListItem::new(Line::from(vec![
                        Span::raw(selected),
                        Span::raw(visibility),
                        Span::raw(" "),
                        Span::styled(name, Theme::highlight()),
                        Span::raw(format!(" = {}", latest)),
                    ]))
                })
                .collect();

            List::new(items).render(series_inner, buf);
        }

        // Config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, serial_config, focus);
        }
    }

    fn draw_config(
        &self,
        area: Rect,
        buf: &mut Buffer,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(5)])
            .split(area);

        // Connection info (read-only)
        let conn_block = Block::default()
            .title(" Connection ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let conn_inner = conn_block.inner(chunks[0]);
        conn_block.render(chunks[0], buf);

        let conn_lines = vec![
            Line::from(vec![
                Span::styled("Baud:  ", Theme::muted()),
                Span::raw(serial_config.baud_rate.to_string()),
            ]),
        ];

        for (i, line) in conn_lines.into_iter().enumerate() {
            if i >= conn_inner.height as usize {
                break;
            }
            Paragraph::new(line).render(
                Rect::new(conn_inner.x, conn_inner.y + i as u16, conn_inner.width, 1),
                buf,
            );
        }

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
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus, handle: &SessionHandle) {
        match focus {
            Focus::Main => self.handle_main_key(key, handle),
            Focus::Config => self.handle_config_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent, handle: &SessionHandle) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected_series = self.selected_series.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_series = self.selected_series.saturating_sub(1);
            }
            KeyCode::Char('t') | KeyCode::Enter | KeyCode::Char(' ') => {
                // Toggle series visibility
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
            KeyCode::Char('g') => {
                // Toggle graph enable/disable
                let mut buffer = handle.buffer_mut();
                if buffer.graph_enabled() {
                    buffer.disable_graph();
                } else {
                    buffer.enable_graph();
                }
            }
            _ => {}
        }
    }

    fn handle_config_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.config_nav
                    .next_field(GRAPH_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.config_nav
                    .prev_field(GRAPH_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(field) = self
                    .config_nav
                    .current_field(GRAPH_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(GRAPH_CONFIG_SECTIONS, &mut self.config);
                    } else {
                        self.config_nav
                            .dropdown_prev(GRAPH_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(GRAPH_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(field) = self
                    .config_nav
                    .current_field(GRAPH_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(GRAPH_CONFIG_SECTIONS, &mut self.config);
                    } else {
                        self.config_nav
                            .dropdown_next(GRAPH_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(GRAPH_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            _ => {}
        }
        self.config_nav
            .sync_dropdown_index(GRAPH_CONFIG_SECTIONS, &self.config);
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

use crate::widget::ConfigPanel;
