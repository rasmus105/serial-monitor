//! Pre-connection view: port selection and configuration.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use serial_core::{
    DataBits, SerialConfig, list_ports,
    ui::{
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
        serial_config::{
            COMMON_BAUD_RATES, DATA_BITS_VARIANTS, FLOW_CONTROL_VARIANTS, PARITY_VARIANTS,
            STOP_BITS_VARIANTS,
        },
    },
};

use crate::{
    app::{Focus, PreConnectAction},
    theme::Theme,
    widget::{
        ConfigPanel, PortList, TextInput, Toast,
        port_list::PortListState,
        text_input::TextInputState,
    },
};

/// Pre-connection view state.
pub struct PreConnectView {
    /// Port list state.
    pub port_list: PortListState,
    /// Serial configuration.
    pub config: PreConnectConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
}

/// Configuration state for pre-connection.
#[derive(Debug, Clone)]
pub struct PreConnectConfig {
    pub baud_rate_index: usize,
    pub data_bits_index: usize,
    pub parity_index: usize,
    pub stop_bits_index: usize,
    pub flow_control_index: usize,
}

impl Default for PreConnectConfig {
    fn default() -> Self {
        Self {
            // Default to 115200
            baud_rate_index: COMMON_BAUD_RATES
                .iter()
                .position(|&r| r == 115200)
                .unwrap_or(8),
            // Default to 8 data bits
            data_bits_index: DATA_BITS_VARIANTS
                .iter()
                .position(|&d| d == DataBits::Eight)
                .unwrap_or(3),
            // Default to no parity
            parity_index: 0,
            // Default to 1 stop bit
            stop_bits_index: 0,
            // Default to no flow control
            flow_control_index: 0,
        }
    }
}

impl PreConnectConfig {
    pub fn to_serial_config(&self) -> SerialConfig {
        SerialConfig {
            baud_rate: COMMON_BAUD_RATES[self.baud_rate_index],
            data_bits: DATA_BITS_VARIANTS[self.data_bits_index],
            parity: PARITY_VARIANTS[self.parity_index],
            stop_bits: STOP_BITS_VARIANTS[self.stop_bits_index],
            flow_control: FLOW_CONTROL_VARIANTS[self.flow_control_index],
        }
    }
}

// Config panel field definitions
const BAUD_RATE_OPTIONS: &[&str] = &[
    "300", "1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "230400", "460800",
    "921600",
];
const DATA_BITS_OPTIONS: &[&str] = &["5", "6", "7", "8"];
const PARITY_OPTIONS: &[&str] = &["None", "Odd", "Even"];
const STOP_BITS_OPTIONS: &[&str] = &["1", "2"];
const FLOW_CONTROL_OPTIONS: &[&str] = &["None", "Software (XON/XOFF)", "Hardware (RTS/CTS)"];

static PRECONNECT_CONFIG_SECTIONS: &[Section<PreConnectConfig>] = &[Section {
    header: Some("Serial Port"),
    fields: &[
        FieldDef {
            id: "baud_rate",
            label: "Baud Rate",
            kind: FieldKind::Select {
                options: BAUD_RATE_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.baud_rate_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.baud_rate_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "data_bits",
            label: "Data Bits",
            kind: FieldKind::Select {
                options: DATA_BITS_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.data_bits_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.data_bits_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "parity",
            label: "Parity",
            kind: FieldKind::Select {
                options: PARITY_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.parity_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.parity_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "stop_bits",
            label: "Stop Bits",
            kind: FieldKind::Select {
                options: STOP_BITS_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.stop_bits_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.stop_bits_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
        FieldDef {
            id: "flow_control",
            label: "Flow Control",
            kind: FieldKind::Select {
                options: FLOW_CONTROL_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.flow_control_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.flow_control_index = i;
                }
            },
            visible: always_visible,
            validate: always_valid,
        },
    ],
}];

impl PreConnectView {
    pub fn new() -> Self {
        Self {
            port_list: PortListState::new(),
            config: PreConnectConfig::default(),
            config_nav: ConfigPanelNav::new(),
            search_input: TextInputState::new().with_placeholder("Search ports..."),
            search_focused: false,
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

    pub fn is_input_mode(&self) -> bool {
        self.search_focused
    }

    pub fn draw(
        &self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        focus: Focus,
    ) {
        // Main area: port list + optional search bar
        let main_chunks = if self.search_focused || self.port_list.has_search() {
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
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let mut port_list_state = PortListState {
            ports: self.port_list.ports.clone(),
            list_state: self.port_list.list_state.clone(),
            search_pattern: self.port_list.search_pattern.clone(),
            matching_indices: self.port_list.matching_indices.clone(),
            current_match: self.port_list.current_match,
        };

        PortList::new()
            .block(port_block)
            .focused(focus == Focus::Main && !self.search_focused)
            .render(main_chunks[0], buf, &mut port_list_state);

        // Search bar if active
        if self.search_focused || self.port_list.has_search() {
            let search_block = Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(if self.search_focused {
                    Theme::border_focused()
                } else {
                    Theme::border()
                });

            let mut search_state = self.search_input.clone();
            TextInput::new(&mut search_state)
                .block(search_block)
                .focused(self.search_focused)
                .render(main_chunks[1], buf);
        }

        // Help text at bottom of port list
        if main_chunks[0].height > 2 {
            let help_y = main_chunks[0].y + main_chunks[0].height - 2;
            let help_line = Line::from(vec![
                Span::styled("Enter", Theme::keybind()),
                Span::raw(" connect  "),
                Span::styled("r", Theme::keybind()),
                Span::raw(" refresh  "),
                Span::styled("/", Theme::keybind()),
                Span::raw(" search  "),
                Span::styled("Ctrl+h/l", Theme::keybind()),
                Span::raw(" panels  "),
                Span::styled("?", Theme::keybind()),
                Span::raw(" help"),
            ]);
            Paragraph::new(help_line)
                .style(Theme::muted())
                .render(Rect::new(main_chunks[0].x + 2, help_y, main_chunks[0].width - 4, 1), buf);
        }

        // Config panel
        if let Some(config_area) = config_area {
            let config_block = Block::default()
                .title(" Configuration ")
                .borders(Borders::ALL)
                .border_style(if focus == Focus::Config {
                    Theme::border_focused()
                } else {
                    Theme::border()
                });

            ConfigPanel::new(PRECONNECT_CONFIG_SECTIONS, &self.config, &self.config_nav)
                .block(config_block)
                .focused(focus == Focus::Config)
                .render(config_area, buf);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, focus: Focus) -> Option<PreConnectAction> {
        // Handle search input mode
        if self.search_focused {
            match key.code {
                KeyCode::Enter => {
                    // Apply search and exit search mode
                    let pattern = self.search_input.content.clone();
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
                    self.port_list.set_search(&self.search_input.content);
                }
            }
            return None;
        }

        match focus {
            Focus::Main => self.handle_main_key(key),
            Focus::Config => self.handle_config_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.port_list.select_next();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.port_list.select_prev();
            }
            KeyCode::Char('r') => {
                self.refresh_ports();
                return Some(PreConnectAction::Toast(Toast::info("Ports refreshed")));
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
                        config: self.config.to_serial_config(),
                    });
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent) -> Option<PreConnectAction> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.config_nav
                    .next_field(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.config_nav
                    .prev_field(PRECONNECT_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.config_nav
                    .dropdown_prev(PRECONNECT_CONFIG_SECTIONS, &self.config);
                let _ = self
                    .config_nav
                    .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.config_nav
                    .dropdown_next(PRECONNECT_CONFIG_SECTIONS, &self.config);
                let _ = self
                    .config_nav
                    .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // For select fields, cycle through options
                if let Some(field) = self
                    .config_nav
                    .current_field(PRECONNECT_CONFIG_SECTIONS, &self.config)
                {
                    if field.kind.is_select() {
                        self.config_nav
                            .dropdown_next(PRECONNECT_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(PRECONNECT_CONFIG_SECTIONS, &mut self.config);
                    }
                }
            }
            _ => {}
        }
        // Sync dropdown index with current selection
        self.config_nav
            .sync_dropdown_index(PRECONNECT_CONFIG_SECTIONS, &self.config);
        None
    }
}

impl Default for PreConnectView {
    fn default() -> Self {
        Self::new()
    }
}
