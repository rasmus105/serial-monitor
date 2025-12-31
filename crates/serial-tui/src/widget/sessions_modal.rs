//! Sessions modal widget for viewing and managing active sessions.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::{
    app::{SessionEntry, SessionState},
    theme::Theme,
};

/// State for the sessions modal.
#[derive(Debug, Clone, Default)]
pub struct SessionsModalState {
    /// Whether the modal is visible.
    pub visible: bool,
    /// Currently selected index in the list.
    pub selected: usize,
    /// Whether we're in disconnect confirmation mode.
    pub confirming_disconnect: bool,
}

/// Actions returned from the sessions modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionsModalAction {
    /// No action.
    None,
    /// User pressed Esc, close the modal.
    Close,
    /// User pressed Enter on a session (return its index).
    SwitchTo(usize),
    /// User confirmed disconnect (return session index).
    ConfirmDisconnect(usize),
}

impl SessionsModalState {
    /// Show the modal, reset selection to 0.
    pub fn show(&mut self) {
        self.visible = true;
        self.selected = 0;
        self.confirming_disconnect = false;
    }

    /// Hide the modal.
    pub fn hide(&mut self) {
        self.visible = false;
        self.confirming_disconnect = false;
    }

    /// Handle key input. Returns an action to perform.
    pub fn handle_key(&mut self, key: KeyEvent, session_count: usize) -> SessionsModalAction {
        if session_count == 0 {
            // No sessions, close on any key
            self.hide();
            return SessionsModalAction::Close;
        }

        // In confirmation mode
        if self.confirming_disconnect {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let index = self.selected;
                    self.confirming_disconnect = false;
                    return SessionsModalAction::ConfirmDisconnect(index);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirming_disconnect = false;
                }
                _ => {}
            }
            return SessionsModalAction::None;
        }

        // Normal mode
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.hide();
                return SessionsModalAction::Close;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected < session_count.saturating_sub(1) {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                return SessionsModalAction::SwitchTo(self.selected);
            }
            KeyCode::Char('d') => {
                self.confirming_disconnect = true;
            }
            _ => {}
        }

        SessionsModalAction::None
    }
}

/// Sessions modal widget.
pub struct SessionsModal<'a> {
    state: &'a SessionsModalState,
    sessions: &'a [SessionEntry],
    active_index: Option<usize>,
}

impl<'a> SessionsModal<'a> {
    pub fn new(
        state: &'a SessionsModalState,
        sessions: &'a [SessionEntry],
        active_index: Option<usize>,
    ) -> Self {
        Self {
            state,
            sessions,
            active_index,
        }
    }
}

impl Widget for SessionsModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate overlay size (centered, reasonable size)
        let width = 60.min(area.width.saturating_sub(4));
        // Height: border + sessions + blank + hint + border
        let content_height = self.sessions.len() as u16 + 2; // sessions + hint lines
        let height = (content_height + 2).min(area.height.saturating_sub(4)).max(8);

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let overlay_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(overlay_area, buf);

        // Block with border
        let block = Block::default()
            .title(" Sessions ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let inner = block.inner(overlay_area);
        block.render(overlay_area, buf);

        // Build session lines
        let mut lines: Vec<Line> = Vec::new();

        for (idx, entry) in self.sessions.iter().enumerate() {
            let is_selected = idx == self.state.selected;
            let is_active = Some(idx) == self.active_index;

            let line = build_session_line(entry, is_selected, is_active, self.state.confirming_disconnect);
            lines.push(line);
        }

        // Add empty line before hints
        lines.push(Line::from(""));

        // Add hint line
        let hint_line = if self.state.confirming_disconnect {
            Line::from(vec![
                Span::raw("Disconnect? "),
                Span::styled("[y]", Theme::keybind()),
                Span::raw("es / "),
                Span::styled("[n]", Theme::keybind()),
                Span::raw("o"),
            ])
        } else {
            Line::from(vec![
                Span::styled("[Enter]", Theme::keybind()),
                Span::raw(" Switch  "),
                Span::styled("[d]", Theme::keybind()),
                Span::raw(" Disconnect  "),
                Span::styled("[Esc]", Theme::keybind()),
                Span::raw(" Close"),
            ])
        };
        lines.push(hint_line);

        Paragraph::new(lines).render(inner, buf);
    }
}

/// Build a line for a single session entry.
fn build_session_line(
    entry: &SessionEntry,
    is_selected: bool,
    is_active: bool,
    confirming_disconnect: bool,
) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();

    // Selection/active marker
    let marker = if is_active && is_selected {
        "* > "
    } else if is_active {
        "*   "
    } else if is_selected {
        "  > "
    } else {
        "    "
    };

    let marker_style = if is_selected && confirming_disconnect {
        Theme::error()
    } else if is_selected {
        Theme::highlight()
    } else if is_active {
        Theme::success()
    } else {
        Theme::muted()
    };

    spans.push(Span::styled(marker.to_string(), marker_style));

    // Session content based on state
    match &entry.state {
        SessionState::PreConnect(_) => {
            let text = "New Connection";
            let style = if is_selected {
                Theme::highlight()
            } else {
                Theme::muted()
            };
            spans.push(Span::styled(text.to_string(), style));
        }
        SessionState::Connected(state) => {
            // Port name
            let port_name = state.handle.port_name();
            let name_style = if is_selected {
                Theme::highlight()
            } else {
                Theme::default()
            };
            spans.push(Span::styled(port_name.to_string(), name_style));

            // Stats - show chunk count and buffer size
            let buffer = state.handle.buffer();
            let chunk_count = buffer.len();
            let buffer_size = buffer.size();
            let stats_str = format!(
                "  {} chunks  {}",
                chunk_count,
                format_bytes(buffer_size),
            );
            let stats_style = Theme::muted();
            spans.push(Span::styled(stats_str, stats_style));

            // Duration if we have session start time
            if let Some(start) = state.traffic.session_start
                && let Ok(elapsed) = start.elapsed()
            {
                let duration_str = format!("  {}", format_duration(elapsed.as_secs()));
                spans.push(Span::styled(duration_str, stats_style));
            }
        }
    }

    Line::from(spans)
}

/// Format bytes nicely (e.g., 1024 -> "1.0 KB").
fn format_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration nicely (seconds -> "HH:MM:SS").
fn format_duration(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
