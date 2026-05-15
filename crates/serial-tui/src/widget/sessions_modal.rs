//! Sessions modal widget for viewing and managing active sessions.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use super::util::{
    format_bytes, format_duration, format_flow_control, format_rate, format_serial_config_compact,
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
    /// User requested disconnect (return session index).
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
                return SessionsModalAction::ConfirmDisconnect(self.selected);
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
        let width = 88.min(area.width.saturating_sub(4));
        // Height: border + session detail lines + blank + hint + border
        let content_height = self
            .sessions
            .iter()
            .map(session_line_count)
            .sum::<u16>()
            .saturating_sub(1)
            + 2;
        let height = (content_height + 2)
            .min(area.height.saturating_sub(4))
            .max(8);

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

            lines.extend(build_session_lines(
                entry,
                is_selected,
                is_active,
                self.state.confirming_disconnect,
            ));

            if idx + 1 < self.sessions.len() {
                lines.push(Line::from(""));
            }
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

/// Build lines for a single session entry.
fn build_session_lines(
    entry: &SessionEntry,
    is_selected: bool,
    is_active: bool,
    confirming_disconnect: bool,
) -> Vec<Line<'static>> {
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

    match &entry.state {
        SessionState::PreConnect(_) => {
            let text = "New Connection";
            let style = if is_selected {
                Theme::highlight()
            } else {
                Theme::muted()
            };
            spans.push(Span::styled(text.to_string(), style));

            vec![
                Line::from(spans),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("State:  ", label_style()),
                    Span::styled("Not connected", Theme::muted()),
                ]),
            ]
        }
        SessionState::Connected(state) => {
            let port_name = state.handle.port_name();
            let name_style = if is_selected {
                Theme::highlight()
            } else if !state.connected {
                Theme::error()
            } else {
                Theme::base()
            };
            spans.push(Span::styled(port_name.to_string(), name_style));

            let statistics = state.handle.statistics();
            let duration = format_duration(statistics.duration().as_secs());
            let config = &state.serial_config;

            vec![
                Line::from(spans),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("Config: ", label_style()),
                    Span::styled(format_serial_config_compact(config), Theme::base()),
                    Span::styled(", Flow: ", Theme::muted()),
                    Span::styled(format_flow_control(config.flow_control), Theme::base()),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("RX:     ", Theme::rx().add_modifier(Modifier::BOLD)),
                    Span::styled(format_bytes(statistics.bytes_rx()), Theme::base()),
                    Span::styled(", ", Theme::muted()),
                    Span::styled(format!("{} pkts", statistics.packets_rx()), Theme::base()),
                    Span::styled(", ", Theme::muted()),
                    Span::styled(
                        format_rate(statistics.avg_bytes_rx_per_sec()),
                        Theme::base(),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("TX:     ", Theme::tx().add_modifier(Modifier::BOLD)),
                    Span::styled(format_bytes(statistics.bytes_tx()), Theme::base()),
                    Span::styled(", ", Theme::muted()),
                    Span::styled(format!("{} pkts", statistics.packets_tx()), Theme::base()),
                    Span::styled(", ", Theme::muted()),
                    Span::styled(
                        format_rate(statistics.avg_bytes_tx_per_sec()),
                        Theme::base(),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("Time:   ", label_style()),
                    Span::styled(duration, Theme::base()),
                ]),
            ]
        }
    }
}

fn session_line_count(entry: &SessionEntry) -> u16 {
    match entry.state {
        SessionState::PreConnect(_) => 3,
        SessionState::Connected(_) => 6,
    }
}

fn label_style() -> ratatui::style::Style {
    Theme::muted().add_modifier(Modifier::BOLD)
}
