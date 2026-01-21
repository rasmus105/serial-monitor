//! Canvas-based chart rendering for graph visualization.

use std::time::SystemTime;

use iced::alignment;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Fill, Point, Rectangle, Renderer, Theme as IcedTheme};

use serial_core::buffer::graph::GraphMode;

use crate::app::{ConnectedState, Message};
use crate::theme::Theme;

/// Chart colors for different series (cycling through these).
pub const SERIES_COLORS: &[Color] = &[
    Color::from_rgb(0.34, 0.61, 0.84), // Blue (primary)
    Color::from_rgb(0.31, 0.79, 0.69), // Teal (success)
    Color::from_rgb(0.86, 0.86, 0.67), // Yellow (warning)
    Color::from_rgb(0.95, 0.45, 0.45), // Red
    Color::from_rgb(0.68, 0.51, 0.84), // Purple
    Color::from_rgb(0.95, 0.65, 0.45), // Orange
];

/// Render the chart canvas.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    let chart = ChartProgram { state };

    Canvas::new(chart).width(Fill).height(Fill).into()
}

/// Chart canvas program.
struct ChartProgram<'a> {
    state: &'a ConnectedState,
}

impl canvas::Program<Message> for ChartProgram<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &IcedTheme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Margins for axis labels
        let margin = Margins {
            left: 60.0,
            right: 20.0,
            top: 20.0,
            bottom: 40.0,
        };

        let chart_bounds = Rectangle {
            x: bounds.x + margin.left,
            y: bounds.y + margin.top,
            width: bounds.width - margin.left - margin.right,
            height: bounds.height - margin.top - margin.bottom,
        };

        if chart_bounds.width <= 0.0 || chart_bounds.height <= 0.0 {
            return vec![];
        }

        let buffer = self.state.handle.buffer();
        let graph_view = &self.state.graph_view;
        let mode = graph_view.config.mode();
        let time_range = graph_view.config.time_range();

        let geometry = match buffer.graph() {
            Some(graph) => match mode {
                GraphMode::ParsedData => {
                    self.draw_parsed_data(renderer, bounds, chart_bounds, graph, time_range)
                }
                GraphMode::PacketRate => {
                    self.draw_packet_rate(renderer, bounds, chart_bounds, graph, time_range)
                }
            },
            None => self.draw_empty_chart(renderer, bounds, chart_bounds, "No data"),
        };

        vec![geometry]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

impl ChartProgram<'_> {
    /// Draw parsed data chart.
    fn draw_parsed_data(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
        chart_bounds: Rectangle,
        graph: &serial_core::buffer::graph::GraphEngine,
        time_range: Option<std::time::Duration>,
    ) -> Geometry {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Collect visible series data
        let visible_series: Vec<_> = graph.series.iter().filter(|(_, s)| s.visible).collect();

        if visible_series.is_empty() {
            return self.draw_empty_chart(renderer, bounds, chart_bounds, "No visible series");
        }

        // Find data bounds
        let now = SystemTime::now();
        let cutoff = time_range.and_then(|d| now.checked_sub(d));

        // Find reference time (earliest point) and data bounds
        let mut reference_time: Option<SystemTime> = None;
        let mut x_max: f64 = 0.0;
        let mut y_min: f64 = f64::INFINITY;
        let mut y_max: f64 = f64::NEG_INFINITY;

        for (_, series) in &visible_series {
            for point in &series.points {
                if cutoff.is_some_and(|c| point.timestamp < c) {
                    continue;
                }
                reference_time = Some(match reference_time {
                    Some(r) if point.timestamp < r => point.timestamp,
                    Some(r) => r,
                    None => point.timestamp,
                });
            }
        }

        let reference_time = reference_time.unwrap_or(now);

        // Second pass to get actual bounds
        for (_, series) in &visible_series {
            for point in &series.points {
                if cutoff.is_some_and(|c| point.timestamp < c) {
                    continue;
                }
                let x = point
                    .timestamp
                    .duration_since(reference_time)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                x_max = x_max.max(x);
                y_min = y_min.min(point.value);
                y_max = y_max.max(point.value);
            }
        }

        // Handle edge cases
        if y_min == f64::INFINITY {
            y_min = 0.0;
            y_max = 100.0;
        }
        if (y_max - y_min).abs() < 1e-10 {
            y_min -= 1.0;
            y_max += 1.0;
        }
        if x_max < 1.0 {
            x_max = 60.0;
        }

        // Add margins to Y bounds
        let y_margin = (y_max - y_min) * 0.1;
        y_min -= y_margin;
        y_max += y_margin;

        // Draw grid and axes
        self.draw_grid(&mut frame, chart_bounds, 0.0, x_max, y_min, y_max);
        self.draw_axes(
            &mut frame,
            chart_bounds,
            0.0,
            x_max,
            y_min,
            y_max,
            "Time (s)",
            "Value",
        );

        // Draw series
        for (idx, (_name, series)) in visible_series.iter().enumerate() {
            let color = SERIES_COLORS[idx % SERIES_COLORS.len()];

            let points: Vec<Point> = series
                .points
                .iter()
                .filter(|p| cutoff.is_none_or(|c| p.timestamp >= c))
                .map(|p| {
                    let x = p
                        .timestamp
                        .duration_since(reference_time)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    self.data_to_canvas(chart_bounds, x, p.value, 0.0, x_max, y_min, y_max)
                })
                .collect();

            self.draw_line_series(&mut frame, &points, color);
        }

        frame.into_geometry()
    }

    /// Draw packet rate chart.
    fn draw_packet_rate(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
        chart_bounds: Rectangle,
        graph: &serial_core::buffer::graph::GraphEngine,
        time_range: Option<std::time::Duration>,
    ) -> Geometry {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let packet_rate = &graph.config.packet_rate;
        if packet_rate.samples.is_empty() {
            return self.draw_empty_chart(renderer, bounds, chart_bounds, "No rate data");
        }

        let graph_view = &self.state.graph_view;
        let show_rx = graph_view.config.show_rx_rate;
        let show_tx = graph_view.config.show_tx_rate;

        if !show_rx && !show_tx {
            return self.draw_empty_chart(renderer, bounds, chart_bounds, "Enable RX or TX");
        }

        // Find reference time and bounds
        let now = SystemTime::now();
        let cutoff = time_range.and_then(|d| now.checked_sub(d));

        let reference_time = packet_rate
            .samples
            .front()
            .map(|s| s.window_start())
            .unwrap_or(now);

        let window_secs = packet_rate.window_size.as_secs_f64();
        let mut x_max: f64 = 0.0;
        let mut y_max: f64 = 0.0;

        for sample in &packet_rate.samples {
            let sample_time = sample.window_start();
            if cutoff.is_some_and(|c| sample_time < c) {
                continue;
            }
            let x = sample_time
                .duration_since(reference_time)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            x_max = x_max.max(x);

            if show_rx {
                y_max = y_max.max(sample.rx_count as f64 / window_secs);
            }
            if show_tx {
                y_max = y_max.max(sample.tx_count as f64 / window_secs);
            }
        }

        if x_max < 1.0 {
            x_max = 60.0;
        }
        if y_max < 1.0 {
            y_max = 10.0;
        }
        y_max *= 1.1; // 10% margin

        // Draw grid and axes
        self.draw_grid(&mut frame, chart_bounds, 0.0, x_max, 0.0, y_max);
        self.draw_axes(
            &mut frame,
            chart_bounds,
            0.0,
            x_max,
            0.0,
            y_max,
            "Time (s)",
            "Packets/s",
        );

        // Draw RX rate
        if show_rx {
            let points: Vec<Point> = packet_rate
                .samples
                .iter()
                .filter(|s| cutoff.is_none_or(|c| s.window_start() >= c))
                .map(|s| {
                    let x = s
                        .window_start()
                        .duration_since(reference_time)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    let y = s.rx_count as f64 / window_secs;
                    self.data_to_canvas(chart_bounds, x, y, 0.0, x_max, 0.0, y_max)
                })
                .collect();

            self.draw_line_series(&mut frame, &points, Theme::RX);
        }

        // Draw TX rate
        if show_tx {
            let points: Vec<Point> = packet_rate
                .samples
                .iter()
                .filter(|s| cutoff.is_none_or(|c| s.window_start() >= c))
                .map(|s| {
                    let x = s
                        .window_start()
                        .duration_since(reference_time)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    let y = s.tx_count as f64 / window_secs;
                    self.data_to_canvas(chart_bounds, x, y, 0.0, x_max, 0.0, y_max)
                })
                .collect();

            self.draw_line_series(&mut frame, &points, Theme::TX);
        }

        frame.into_geometry()
    }

    /// Draw an empty chart with a message.
    fn draw_empty_chart(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
        chart_bounds: Rectangle,
        message: &str,
    ) -> Geometry {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Draw placeholder grid
        self.draw_grid(&mut frame, chart_bounds, 0.0, 60.0, 0.0, 100.0);
        self.draw_axes(
            &mut frame,
            chart_bounds,
            0.0,
            60.0,
            0.0,
            100.0,
            "Time (s)",
            "Value",
        );

        // Draw centered message
        let text = Text {
            content: message.to_string(),
            position: Point::new(
                chart_bounds.x + chart_bounds.width / 2.0,
                chart_bounds.y + chart_bounds.height / 2.0,
            ),
            color: Theme::TEXT_SECONDARY,
            size: 14.0.into(),
            align_x: alignment::Horizontal::Center.into(),
            align_y: alignment::Vertical::Center.into(),
            ..Default::default()
        };
        frame.fill_text(text);

        frame.into_geometry()
    }

    /// Convert data coordinates to canvas coordinates.
    fn data_to_canvas(
        &self,
        bounds: Rectangle,
        x: f64,
        y: f64,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> Point {
        let x_range = x_max - x_min;
        let y_range = y_max - y_min;

        let canvas_x = if x_range > 0.0 {
            bounds.x + ((x - x_min) / x_range) as f32 * bounds.width
        } else {
            bounds.x + bounds.width / 2.0
        };

        let canvas_y = if y_range > 0.0 {
            bounds.y + bounds.height - ((y - y_min) / y_range) as f32 * bounds.height
        } else {
            bounds.y + bounds.height / 2.0
        };

        Point::new(canvas_x, canvas_y)
    }

    /// Draw grid lines.
    fn draw_grid(
        &self,
        frame: &mut canvas::Frame,
        bounds: Rectangle,
        _x_min: f64,
        _x_max: f64,
        _y_min: f64,
        _y_max: f64,
    ) {
        let grid_color = Color::from_rgba(0.3, 0.3, 0.3, 0.5);
        let stroke = Stroke::default().with_color(grid_color).with_width(1.0);

        // Vertical grid lines (5 divisions)
        for i in 0..=5 {
            let x = bounds.x + (i as f32 / 5.0) * bounds.width;
            let path = Path::line(
                Point::new(x, bounds.y),
                Point::new(x, bounds.y + bounds.height),
            );
            frame.stroke(&path, stroke.clone());
        }

        // Horizontal grid lines (5 divisions)
        for i in 0..=5 {
            let y = bounds.y + (i as f32 / 5.0) * bounds.height;
            let path = Path::line(
                Point::new(bounds.x, y),
                Point::new(bounds.x + bounds.width, y),
            );
            frame.stroke(&path, stroke.clone());
        }
    }

    /// Draw axis labels.
    fn draw_axes(
        &self,
        frame: &mut canvas::Frame,
        bounds: Rectangle,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        x_label: &str,
        y_label: &str,
    ) {
        let label_color = Theme::TEXT_SECONDARY;
        let font_size = 11.0;

        // X-axis labels
        for i in 0..=5 {
            let x = bounds.x + (i as f32 / 5.0) * bounds.width;
            let value = x_min + (i as f64 / 5.0) * (x_max - x_min);
            let text = Text {
                content: format!("{:.0}", value),
                position: Point::new(x, bounds.y + bounds.height + 15.0),
                color: label_color,
                size: font_size.into(),
                align_x: alignment::Horizontal::Center.into(),
                ..Default::default()
            };
            frame.fill_text(text);
        }

        // X-axis title
        let x_title = Text {
            content: x_label.to_string(),
            position: Point::new(
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height + 30.0,
            ),
            color: label_color,
            size: font_size.into(),
            align_x: alignment::Horizontal::Center.into(),
            ..Default::default()
        };
        frame.fill_text(x_title);

        // Y-axis labels
        for i in 0..=5 {
            let y = bounds.y + bounds.height - (i as f32 / 5.0) * bounds.height;
            let value = y_min + (i as f64 / 5.0) * (y_max - y_min);
            let text = Text {
                content: format_value(value),
                position: Point::new(bounds.x - 8.0, y),
                color: label_color,
                size: font_size.into(),
                align_x: alignment::Horizontal::Right.into(),
                align_y: alignment::Vertical::Center.into(),
                ..Default::default()
            };
            frame.fill_text(text);
        }

        // Y-axis title (rotated text not supported, so use short label)
        let y_title = Text {
            content: y_label.to_string(),
            position: Point::new(bounds.x - 50.0, bounds.y + bounds.height / 2.0),
            color: label_color,
            size: font_size.into(),
            align_x: alignment::Horizontal::Center.into(),
            align_y: alignment::Vertical::Center.into(),
            ..Default::default()
        };
        frame.fill_text(y_title);
    }

    /// Draw a line series.
    fn draw_line_series(&self, frame: &mut canvas::Frame, points: &[Point], color: Color) {
        if points.len() < 2 {
            // Draw dots for single points
            for point in points {
                let circle = Path::circle(*point, 3.0);
                frame.fill(&circle, color);
            }
            return;
        }

        // Build path
        let path = Path::new(|builder| {
            builder.move_to(points[0]);
            for point in &points[1..] {
                builder.line_to(*point);
            }
        });

        let stroke = Stroke::default().with_color(color).with_width(2.0);
        frame.stroke(&path, stroke);

        // Draw small dots at data points (optional, for visibility)
        for point in points {
            let circle = Path::circle(*point, 2.0);
            frame.fill(&circle, color);
        }
    }
}

/// Format a value for display on axis labels.
fn format_value(value: f64) -> String {
    if value.abs() >= 1000.0 {
        format!("{:.1}k", value / 1000.0)
    } else if value.abs() >= 1.0 {
        format!("{:.1}", value)
    } else if value.abs() >= 0.01 {
        format!("{:.2}", value)
    } else {
        format!("{:.3}", value)
    }
}

/// Chart margins.
struct Margins {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}
