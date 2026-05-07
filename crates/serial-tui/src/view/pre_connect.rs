//! Pre-connection view: port selection.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use serial_core::{PortInfo, list_ports};

use crate::{
    app::{Focus, PreConnectAction},
    keybind::PRECONNECT_HINTS,
    theme::Theme,
    widget::{
        PortList, TextInput, Toast, port_list::PortListState, text_input::TextInputState,
        util::build_help_line,
    },
};

/// Pre-connection view state.
pub struct PreConnectView {
    /// Port list state.
    pub port_list: PortListState,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
    /// Last visible height for port list (for half-page scroll).
    last_visible_height: usize,
    /// Last time ports were auto-refreshed.
    last_port_refresh: Instant,
}

impl PreConnectView {
    pub fn new() -> Self {
        Self {
            port_list: PortListState::default(),
            search_input: TextInputState::default().with_placeholder("Search ports..."),
            search_focused: false,
            last_visible_height: 20, // Reasonable default
            last_port_refresh: Instant::now(),
        }
    }

    pub fn refresh_ports(&mut self) {
        match list_ports() {
            Ok(ports) => {
                self.port_list.set_ports(ports);
            }
            Err(_) => {
                self.port_list.set_ports(vec![]);
            }
        }
    }

    /// Auto-refreshes ports every 500ms.
    /// Returns a toast if ports changed.
    pub fn tick_auto_refresh(&mut self) -> Option<Toast> {
        use std::time::Duration;
        const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

        if self.last_port_refresh.elapsed() < REFRESH_INTERVAL {
            return None;
        }

        self.last_port_refresh = Instant::now();

        let old_ports: Vec<PortInfo> = self.port_list.ports.clone();

        let new_ports = match list_ports() {
            Ok(ports) => ports,
            Err(_) => vec![],
        };

        // Only update state when the list actually changed so search
        // position and selection are not reset on every tick.
        if new_ports == old_ports {
            return None;
        }

        self.port_list.set_ports(new_ports);

        let old_names: Vec<String> = old_ports.iter().map(|p| p.name.clone()).collect();
        let new_names: Vec<String> = self
            .port_list
            .ports
            .iter()
            .map(|p| p.name.clone())
            .collect();

        let added: Vec<_> = new_names
            .iter()
            .filter(|p| !old_names.contains(p))
            .collect();
        let removed: Vec<_> = old_names
            .iter()
            .filter(|p| !new_names.contains(p))
            .collect();

        if !added.is_empty() {
            return Some(Toast::info(format!(
                "Port connected: {}",
                added
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if !removed.is_empty() {
            return Some(Toast::info(format!(
                "Port disconnected: {}",
                removed
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        None
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        _config_area: Option<Rect>,
        buf: &mut Buffer,
        focus: Focus,
    ) {
        // Main area: port list + optional search/directory input bar
        let show_search_bar = self.search_focused || self.port_list.has_search();
        let main_chunks = if show_search_bar {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(3)])
                .split(main_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5)])
                .split(main_area)
        };

        // Track visible height for half-page scrolling (subtract borders)
        self.last_visible_height = main_chunks[0].height.saturating_sub(2) as usize;

        // Port list
        let port_title = if self.port_list.has_search() {
            let status = self.port_list.search_status();
            format!(" Available Ports [{}] ", status)
        } else {
            " Available Ports ".to_string()
        };

        let port_block = Block::default()
            .title(port_title)
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main && !self.search_focused {
                Theme::border_disconnected()
            } else {
                Theme::border()
            });

        PortList::default()
            .block(port_block)
            .focused(focus == Focus::Main && !self.search_focused)
            .render(main_chunks[0], buf, &mut self.port_list);

        // Search bar if active
        if self.search_focused || self.port_list.has_search() {
            let search_block = Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(if self.search_focused {
                    Theme::border_disconnected()
                } else {
                    Theme::border()
                });

            TextInput::new(&mut self.search_input)
                .block(search_block)
                .focused(self.search_focused)
                .render(main_chunks[1], buf);
        }

        // Help text at bottom of port list
        if main_chunks[0].height > 2 {
            let help_y = main_chunks[0].y + main_chunks[0].height - 2;
            let help_line = build_help_line(PRECONNECT_HINTS, Theme::keybind_disconnected());
            Paragraph::new(help_line).style(Theme::muted()).render(
                Rect::new(main_chunks[0].x + 2, help_y, main_chunks[0].width - 4, 1),
                buf,
            );
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus) -> Option<PreConnectAction> {
        // Handle search input mode
        if self.search_focused {
            match key.code {
                KeyCode::Enter => {
                    // Apply search and exit search mode
                    let pattern = self.search_input.content().to_string();
                    self.port_list.set_search(&pattern);
                    self.search_focused = false;
                }
                KeyCode::Esc => {
                    // Clear search and exit
                    self.search_focused = false;
                    self.search_input.clear();
                    self.port_list.clear_search();
                }
                _ => {
                    self.search_input.handle_key(key);
                    // Live search as user types
                    self.port_list.set_search(self.search_input.content());
                }
            }
            return None;
        }

        match focus {
            Focus::Main => self.handle_main_key(key),
            Focus::Config => None,
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        // Half-page scroll amount based on visible height
        let half_page = self.last_visible_height / 2;

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.port_list.select_next();
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.port_list.select_prev();
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down
                for _ in 0..half_page {
                    self.port_list.select_next();
                }
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up
                for _ in 0..half_page {
                    self.port_list.select_prev();
                }
            }
            KeyCode::Char('/') => {
                self.search_focused = true;
            }
            KeyCode::Char('n') => {
                // Next search match
                self.port_list.goto_next_match();
            }
            KeyCode::Char('N') => {
                // Previous search match
                self.port_list.goto_prev_match();
            }
            KeyCode::Enter => {
                if let Some(port) = self.port_list.selected_name() {
                    return Some(PreConnectAction::Connect {
                        port: port.to_string(),
                    });
                }
            }
            _ => {}
        }
        None
    }
}

impl Default for PreConnectView {
    fn default() -> Self {
        Self::new()
    }
}
