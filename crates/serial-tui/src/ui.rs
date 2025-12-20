//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use serial_core::Direction as DataDirection;

use crate::app::{App, ConnectionState, View};

/// Render the application
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    match app.view {
        View::PortSelect => render_port_select(frame, app, chunks[0]),
        View::Traffic => render_traffic(frame, app, chunks[0]),
    }

    render_status_bar(frame, app, chunks[1]);
}

fn render_port_select(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .ports
        .iter()
        .enumerate()
        .map(|(i, port)| {
            let style = if i == app.selected_port {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == app.selected_port { "> " } else { "  " };

            let label = if let Some(ref product) = port.product {
                format!("{}{} ({})", prefix, port.name, product)
            } else {
                format!("{}{}", prefix, port.name)
            };

            ListItem::new(label).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Port [j/k: navigate, Enter: connect, r: refresh, q: quit] ")
            .borders(Borders::ALL),
    );

    frame.render_widget(list, area);
}

fn render_traffic(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Traffic [j/k: scroll, g/G: top/bottom, q/Esc: disconnect] ")
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let ConnectionState::Connected(ref handle) = app.connection {
        let buffer = handle.buffer();
        let chunks: Vec<_> = buffer.chunks().collect();

        if chunks.is_empty() {
            let msg = Paragraph::new("Waiting for data...")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner);
            return;
        }

        // Build lines from chunks
        let mut lines: Vec<Line> = Vec::new();
        for chunk in &chunks {
            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::Cyan),
            };

            let prefix = match chunk.direction {
                DataDirection::Tx => "TX: ",
                DataDirection::Rx => "RX: ",
            };

            // Format as hex
            let hex: String = chunk
                .data
                .iter()
                .map(|b| format!("{:02X} ", b))
                .collect();

            lines.push(Line::from(vec![
                Span::styled(prefix, direction_style.add_modifier(Modifier::BOLD)),
                Span::styled(hex, direction_style),
            ]));
        }

        // Calculate scroll
        let visible_height = inner.height as usize;
        let max_scroll = lines.len().saturating_sub(visible_height);
        let scroll = app.scroll_offset.min(max_scroll);

        let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible_height).collect();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status = Paragraph::new(app.status.as_str())
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    frame.render_widget(status, area);
}
