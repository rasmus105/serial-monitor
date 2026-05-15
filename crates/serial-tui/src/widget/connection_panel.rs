//! Connection panel widget showing port config and statistics.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use serial_core::{SerialConfig, Statistics};

use super::util::{format_bytes, format_flow_control, format_serial_config_compact};
use crate::theme::Theme;

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

        let duration = self.statistics.duration();
        let hours = duration.as_secs() / 3600;
        let mins = (duration.as_secs() % 3600) / 60;
        let secs = duration.as_secs() % 60;
        let time_str = format!("{:02}:{:02}:{:02}", hours, mins, secs);

        let lines = vec![
            Line::from(vec![
                Span::styled("Port:   ", Theme::muted()),
                Span::styled(self.port_name, Theme::base()),
            ]),
            Line::from(vec![
                Span::styled("Config: ", Theme::muted()),
                Span::styled(
                    format_serial_config_compact(self.serial_config),
                    Theme::base(),
                ),
                Span::styled(", Flow: ", Theme::muted()),
                Span::styled(
                    format_flow_control(self.serial_config.flow_control),
                    Theme::base(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Bytes:  ", Theme::muted()),
                Span::styled("RX", Theme::rx()),
                Span::styled(": ", Theme::muted()),
                Span::styled(format_bytes(self.statistics.bytes_rx()), Theme::base()),
                Span::styled(", ", Theme::muted()),
                Span::styled("TX", Theme::tx()),
                Span::styled(": ", Theme::muted()),
                Span::styled(format_bytes(self.statistics.bytes_tx()), Theme::base()),
            ]),
            Line::from(vec![
                Span::styled("Time:   ", Theme::muted()),
                Span::styled(time_str, Theme::base()),
            ]),
        ];

        for (i, line) in lines.into_iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            Paragraph::new(line)
                .render(Rect::new(inner.x, inner.y + i as u16, inner.width, 1), buf);
        }
    }
}
