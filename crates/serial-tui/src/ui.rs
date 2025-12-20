//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use serial_core::{encode, Direction as DataDirection};

use crate::app::{App, ConnectionState, InputMode, View};

/// Render the application
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main content
            Constraint::Length(1), // Status bar / input
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

    let title = if app.ports.is_empty() {
        " Select Port [:: enter path, r: refresh, q: quit] "
    } else {
        " Select Port [j/k: navigate, Enter: connect, :: enter path, r: refresh, q: quit] "
    };

    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));

    frame.render_widget(list, area);
}

fn render_traffic(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.file_send.is_some() {
        // Show file send in progress
        let progress = app.file_send_progress.as_ref();
        let pct = progress.map(|p| (p.percentage() * 100.0) as u8).unwrap_or(0);
        format!(
            " Traffic [{}] [Sending file: {}% - press 'f' to cancel] ",
            app.encoding, pct
        )
    } else if app.search_pattern.is_some() {
        format!(
            " Traffic [{}] [/: search, n/N: next/prev, Esc: clear] ",
            app.encoding
        )
    } else {
        format!(
            " Traffic [{}] [/: search, e: encoding, i: send, f: file, q: quit] ",
            app.encoding
        )
    };
    
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let ConnectionState::Connected(ref handle) = app.connection {
        let buffer = handle.buffer();
        let chunks: Vec<_> = buffer.chunks().collect();

        if chunks.is_empty() {
            let msg =
                Paragraph::new("Waiting for data...").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner);
            return;
        }

        // Build lines from chunks
        let mut lines: Vec<Line> = Vec::new();
        for (idx, chunk) in chunks.iter().enumerate() {
            let is_match = app.search_match_index == Some(idx);
            let is_search_match = app.search_pattern.as_ref().is_some_and(|pattern| {
                let encoded = encode(&chunk.data, app.encoding);
                encoded.to_lowercase().contains(&pattern.to_lowercase())
            });
            
            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::Cyan),
            };

            let prefix = match chunk.direction {
                DataDirection::Tx => "TX: ",
                DataDirection::Rx => "RX: ",
            };

            // Encode data according to selected encoding
            let encoded = encode(&chunk.data, app.encoding);

            // Highlight the line if it's the current match or contains search pattern
            let line_style = if is_match {
                // Current match - bright yellow background
                direction_style.bg(Color::Yellow).fg(Color::Black)
            } else if is_search_match {
                // Other matches - dim highlight
                direction_style.bg(Color::DarkGray)
            } else {
                direction_style
            };

            let prefix_style = if is_match {
                Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)
            } else if is_search_match {
                direction_style.bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                direction_style.add_modifier(Modifier::BOLD)
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(encoded, line_style),
            ]));
        }

        // Calculate scroll
        let visible_height = inner.height as usize;
        let max_scroll = lines.len().saturating_sub(visible_height);
        let scroll = app.scroll_offset.min(max_scroll);

        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .collect();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    match app.input_mode {
        InputMode::Normal => {
            let status = Paragraph::new(app.status.as_str())
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            frame.render_widget(status, area);
        }
        InputMode::PortInput => {
            let input_line = Line::from(vec![
                Span::styled(":", Style::default().fg(Color::Yellow)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
            ]);
            let input = Paragraph::new(input_line)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            frame.render_widget(input, area);
        }
        InputMode::SendInput => {
            let input_line = Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Green)), // Cursor
            ]);
            let input = Paragraph::new(input_line)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            frame.render_widget(input, area);
        }
        InputMode::SearchInput => {
            let input_line = Line::from(vec![
                Span::styled("/", Style::default().fg(Color::Magenta)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Magenta)), // Cursor
            ]);
            let input = Paragraph::new(input_line)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            frame.render_widget(input, area);
        }
        InputMode::FilePathInput => {
            let input_line = Line::from(vec![
                Span::styled("File: ", Style::default().fg(Color::Blue)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Blue)), // Cursor
            ]);
            let input = Paragraph::new(input_line)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            frame.render_widget(input, area);
        }
    }
}
