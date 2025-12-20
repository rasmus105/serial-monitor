//! UI rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use serial_core::{encode, Direction as DataDirection};

use crate::app::{App, ConnectionState, InputMode, View};
use crate::wrap::wrap_line;

/// Render the application
pub fn render(frame: &mut Frame, app: &mut App) {
    // Clear the entire frame to prevent artifacts from previous renders
    frame.render_widget(Clear, frame.area());

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

fn render_port_select(frame: &mut Frame, app: &mut App, area: Rect) {
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

fn render_traffic(frame: &mut Frame, app: &mut App, area: Rect) {
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

    let block = Block::default().title(title).borders(Borders::ALL);

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

        let content_width = inner.width as usize;

        // Build all physical rows from logical chunks
        // We need to own the encoded strings since wrap_line borrows them
        let encoded_chunks: Vec<String> = chunks
            .iter()
            .map(|chunk| encode(&chunk.data, app.encoding))
            .collect();

        let mut all_physical_rows = Vec::new();

        for (idx, chunk) in chunks.iter().enumerate() {
            let is_match = app.search_match_index == Some(idx);
            let is_search_match = app.search_pattern.as_ref().is_some_and(|pattern| {
                encoded_chunks[idx]
                    .to_lowercase()
                    .contains(&pattern.to_lowercase())
            });

            let direction_style = match chunk.direction {
                DataDirection::Tx => Style::default().fg(Color::Green),
                DataDirection::Rx => Style::default().fg(Color::Cyan),
            };

            let prefix = match chunk.direction {
                DataDirection::Tx => "TX: ",
                DataDirection::Rx => "RX: ",
            };

            // Highlight the line if it's the current match or contains search pattern
            let content_style = if is_match {
                direction_style.bg(Color::Yellow).fg(Color::Black)
            } else if is_search_match {
                direction_style.bg(Color::DarkGray)
            } else {
                direction_style
            };

            let prefix_style = if is_match {
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if is_search_match {
                direction_style
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                direction_style.add_modifier(Modifier::BOLD)
            };

            // Wrap this chunk into physical rows
            let physical_rows = wrap_line(
                prefix,
                prefix_style,
                &encoded_chunks[idx],
                content_style,
                idx,
                content_width,
            );

            all_physical_rows.extend(physical_rows);
        }

        // Resolve scroll_to_chunk to physical row offset
        if let Some(target_chunk) = app.scroll_to_chunk.take() {
            // Find the first physical row belonging to target chunk
            if let Some(row_idx) = all_physical_rows
                .iter()
                .position(|pr| pr.chunk_index == target_chunk)
            {
                app.scroll_offset = row_idx;
            }
        }

        // Calculate scroll based on physical rows
        let visible_height = inner.height as usize;
        let total_rows = all_physical_rows.len();
        let max_scroll = total_rows.saturating_sub(visible_height);
        let scroll = app.scroll_offset.min(max_scroll);
        app.scroll_offset = scroll; // Persist clamped value so scrolling works after G

        // Extract the visible physical rows
        let visible_rows: Vec<Line> = all_physical_rows
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .map(|pr| pr.line)
            .collect();

        // Render without wrapping - we've already handled it
        let paragraph = Paragraph::new(visible_rows);
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
