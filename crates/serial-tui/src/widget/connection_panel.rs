//! Connection panel widget showing port config and statistics.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use serial_core::{SerialConfig, Statistics};

use crate::theme::Theme;
use super::util::{format_bytes, format_rate};

/// Widget displaying connection info (port config + statistics).
pub struct ConnectionPanel<'a> {
    port_name: &'a str,
    serial_config: &'a SerialConfig,
    statistics: &'a Statistics,
    block: Option<Block<'a>>,
}

impl<'a> ConnectionPanel<'a> {
    pub fn new(
        port_name: &'a str,
        serial_config: &'a SerialConfig,
        statistics: &'a Statistics,
    ) -> Self {
        Self {
            port_name,
            serial_config,
            statistics,
            block: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = block.into();
        self
    }
}

impl Widget for ConnectionPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render the outer block and get inner area
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Split horizontally: left for config, right for statistics
        // Use a vertical line as separator
        let chunks = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let left_area = chunks[0];
        let right_area = chunks[1];

        // Draw vertical separator line
        if right_area.x > 0 {
            let sep_x = right_area.x.saturating_sub(1);
            for y in inner.y..inner.y.saturating_add(inner.height) {
                if sep_x >= inner.x && sep_x < inner.x + inner.width {
                    buf[(sep_x, y)].set_symbol("│").set_style(Theme::border());
                }
            }
        }

        // Left side: Connection config
        let config_lines = vec![
            Line::from(vec![
                Span::styled("Port:   ", Theme::muted()),
                Span::raw(self.port_name),
            ]),
            Line::from(vec![
                Span::styled("Baud:   ", Theme::muted()),
                Span::raw(self.serial_config.baud_rate.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Data:   ", Theme::muted()),
                Span::raw(format!("{:?}", self.serial_config.data_bits)),
            ]),
            Line::from(vec![
                Span::styled("Parity: ", Theme::muted()),
                Span::raw(format!("{:?}", self.serial_config.parity)),
            ]),
            Line::from(vec![
                Span::styled("Stop:   ", Theme::muted()),
                Span::raw(format!("{:?}", self.serial_config.stop_bits)),
            ]),
            Line::from(vec![
                Span::styled("Flow:   ", Theme::muted()),
                Span::raw(format!("{:?}", self.serial_config.flow_control)),
            ]),
        ];

        for (i, line) in config_lines.into_iter().enumerate() {
            if i >= left_area.height as usize {
                break;
            }
            Paragraph::new(line).render(
                Rect::new(
                    left_area.x,
                    left_area.y + i as u16,
                    left_area.width.saturating_sub(1),
                    1,
                ),
                buf,
            );
        }

        // Right side: Statistics
        let duration = self.statistics.duration();
        let hours = duration.as_secs() / 3600;
        let mins = (duration.as_secs() % 3600) / 60;
        let secs = duration.as_secs() % 60;
        let time_str = format!("{:02}:{:02}:{:02}", hours, mins, secs);

        let stats_lines = vec![
            Line::from(vec![
                Span::styled("RX: ", Theme::muted()),
                Span::raw(format_bytes(self.statistics.bytes_rx())),
            ]),
            Line::from(vec![
                Span::styled("TX: ", Theme::muted()),
                Span::raw(format_bytes(self.statistics.bytes_tx())),
            ]),
            Line::from(vec![
                Span::styled("Time: ", Theme::muted()),
                Span::raw(time_str),
            ]),
            Line::from(vec![
                Span::styled("Avg RX: ", Theme::muted()),
                Span::raw(format_rate(self.statistics.avg_bytes_rx_per_sec())),
            ]),
            Line::from(vec![
                Span::styled("Avg TX: ", Theme::muted()),
                Span::raw(format_rate(self.statistics.avg_bytes_tx_per_sec())),
            ]),
            Line::from(vec![
                Span::styled("Packets (rx/tx): ", Theme::muted()),
                Span::raw(format!(
                    "{}/{}",
                    self.statistics.packets_rx(),
                    self.statistics.packets_tx()
                )),
            ]),
        ];

        // Add 1 to x to add spacing after separator
        let stats_area = Rect::new(
            right_area.x + 1,
            right_area.y,
            right_area.width.saturating_sub(1),
            right_area.height,
        );

        for (i, line) in stats_lines.into_iter().enumerate() {
            if i >= stats_area.height as usize {
                break;
            }
            Paragraph::new(line).render(
                Rect::new(stats_area.x, stats_area.y + i as u16, stats_area.width, 1),
                buf,
            );
        }
    }
}
