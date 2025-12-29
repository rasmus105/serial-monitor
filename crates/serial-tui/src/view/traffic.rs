//! Traffic view: main data display with search and send functionality.

use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};
use serial_core::{
    Direction as DataDirection, SerialConfig, SessionHandle,
    buffer::PatternMode,
    ui::{
        TimestampFormat,
        config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible},
        encoding::{ENCODING_DISPLAY_NAMES, ENCODING_VARIANTS},
    },
};

use crate::{
    app::{Focus, TrafficAction},
    theme::Theme,
    widget::{ConfigPanel, TextInput, text_input::TextInputState},
};

/// Traffic view state.
pub struct TrafficView {
    /// Current scroll position (visible chunk index).
    pub scroll: usize,
    /// Search input state.
    pub search_input: TextInputState,
    /// Whether search input is focused.
    pub search_focused: bool,
    /// Send input state.
    pub send_input: TextInputState,
    /// Whether send input is focused.
    pub send_focused: bool,
    /// Traffic config.
    pub config: TrafficConfig,
    /// Config panel navigation.
    pub config_nav: ConfigPanelNav,
    /// Session start time for relative timestamps.
    pub session_start: Option<SystemTime>,
    /// Last known visible height (for scroll bounds calculation).
    last_visible_height: usize,
}

/// Traffic view configuration.
#[derive(Debug, Clone)]
pub struct TrafficConfig {
    pub encoding_index: usize,
    pub show_tx: bool,
    pub show_rx: bool,
    pub show_timestamps: bool,
    pub timestamp_format_index: usize,
    pub auto_scroll: bool,
    pub pattern_mode_index: usize,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            encoding_index: 0, // UTF-8
            show_tx: true,
            show_rx: true,
            show_timestamps: true,
            timestamp_format_index: 0, // Relative
            auto_scroll: true,
            pattern_mode_index: 0, // Normal
        }
    }
}

impl TrafficConfig {
    /// Get the timestamp format from the index.
    pub fn timestamp_format(&self) -> TimestampFormat {
        match self.timestamp_format_index {
            0 => TimestampFormat::Relative,
            1 => TimestampFormat::AbsoluteMillis,
            2 => TimestampFormat::Absolute,
            _ => TimestampFormat::Relative,
        }
    }
}

// Config panel definitions
const ENCODING_OPTIONS: &[&str] = ENCODING_DISPLAY_NAMES;
const PATTERN_MODE_OPTIONS: &[&str] = &["Normal", "Regex"];
const TIMESTAMP_FORMAT_OPTIONS: &[&str] = &["Relative", "HH:MM:SS.mmm", "HH:MM:SS"];

static TRAFFIC_CONFIG_SECTIONS: &[Section<TrafficConfig>] = &[
    Section {
        header: Some("Display"),
        fields: &[
            FieldDef {
                id: "encoding",
                label: "Encoding",
                kind: FieldKind::Select {
                    options: ENCODING_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.encoding_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.encoding_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "show_timestamps",
                label: "Timestamps",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_timestamps),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_timestamps = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "timestamp_format",
                label: "Time Format",
                kind: FieldKind::Select {
                    options: TIMESTAMP_FORMAT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.timestamp_format_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.timestamp_format_index = i;
                    }
                },
                visible: |c| c.show_timestamps,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_scroll",
                label: "Auto Scroll",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_scroll),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_scroll = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Filter"),
        fields: &[
            FieldDef {
                id: "show_tx",
                label: "Show TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_tx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_tx = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "show_rx",
                label: "Show RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_rx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_rx = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
            FieldDef {
                id: "pattern_mode",
                label: "Pattern Mode",
                kind: FieldKind::Select {
                    options: PATTERN_MODE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.pattern_mode_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.pattern_mode_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
];

// Connection info section (read-only) - common baud rates for display
#[allow(dead_code)]
const BAUD_RATE_DISPLAY: &[&str] = &[
    "300", "1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "230400", "460800",
    "921600",
];

impl TrafficView {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            search_input: TextInputState::new().with_placeholder("Search pattern..."),
            search_focused: false,
            send_input: TextInputState::new().with_placeholder("Data to send..."),
            send_focused: false,
            config: TrafficConfig::default(),
            config_nav: ConfigPanelNav::new(),
            session_start: None,
            last_visible_height: 20, // Conservative default
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.search_focused || self.send_focused
    }

    /// Sync config changes to the session buffer
    pub fn sync_config_to_buffer(&self, handle: &SessionHandle) {
        let mut buffer = handle.buffer_mut();
        
        // Sync encoding
        let encoding = ENCODING_VARIANTS[self.config.encoding_index];
        buffer.set_encoding(encoding);
        
        // Sync show_tx/show_rx
        buffer.set_show_tx(self.config.show_tx);
        buffer.set_show_rx(self.config.show_rx);
    }

    pub fn draw(
        &mut self,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        handle: &SessionHandle,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        // Main area layout: traffic + optional search/send bar
        let show_input_bar = self.search_focused || self.send_focused;
        let main_chunks = if show_input_bar {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(main_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3)])
                .split(main_area)
        };

        // Draw traffic
        self.draw_traffic(main_chunks[0], buf, handle, focus);

        // Draw input bar if active
        if show_input_bar {
            self.draw_input_bar(main_chunks[1], buf);
        }

        // Draw config panel
        if let Some(config_area) = config_area {
            self.draw_config(config_area, buf, serial_config, focus);
        }
    }

    fn draw_traffic(&mut self, area: Rect, buf: &mut Buffer, handle: &SessionHandle, focus: Focus) {
        let buffer = handle.buffer();

        let inner_height = area.height.saturating_sub(2) as usize; // Account for borders
        let total = buffer.len();
        
        // Update last visible height for key handler scroll bounds
        if inner_height > 0 {
            self.last_visible_height = inner_height;
        }
        
        // Calculate scroll position with proper bounds
        let max_scroll = total.saturating_sub(self.last_visible_height);
        let scroll = if self.config.auto_scroll && total > 0 {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        
        // Clamp stored scroll to valid range
        self.scroll = scroll;

        let block = Block::default()
            .title(format!(
                " Traffic [{}/{}] ",
                scroll + 1,
                total.max(1)
            ))
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Main && !self.is_input_mode() {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let visible_height = inner.height as usize;

        // For relative timestamps, calculate the max width needed for alignment
        // by looking at the last visible chunk's timestamp
        let timestamp_width = if self.config.show_timestamps {
            match self.config.timestamp_format() {
                TimestampFormat::Relative => {
                    // Find the maximum timestamp width in visible chunks
                    let session_start = self.session_start.unwrap_or_else(SystemTime::now);
                    buffer
                        .chunks()
                        .skip(scroll)
                        .take(visible_height)
                        .map(|chunk| {
                            let elapsed = chunk.timestamp.duration_since(session_start).unwrap_or_default();
                            let secs = elapsed.as_secs_f64();
                            // "+{secs:.3}s" - calculate width
                            format!("+{:.3}s", secs).len()
                        })
                        .max()
                        .unwrap_or(7) // Default "+0.000s" = 7 chars
                }
                TimestampFormat::AbsoluteMillis => 12, // "12:34:56.789" = 12 chars
                TimestampFormat::Absolute => 8,       // "12:34:56" = 8 chars
            }
        } else {
            0
        };

        // Render chunks
        let mut y = inner.y;
        for (_i, chunk) in buffer.chunks().skip(scroll).take(visible_height).enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            // Direction indicator
            let (dir_char, dir_style) = match chunk.direction {
                DataDirection::Tx => ("TX", Theme::tx()),
                DataDirection::Rx => ("RX", Theme::rx()),
            };

            // Build line
            let mut spans = vec![
                Span::styled(format!("{} ", dir_char), dir_style),
            ];

            // Timestamp - format according to config with alignment
            if self.config.show_timestamps {
                let session_start = self.session_start.unwrap_or(chunk.timestamp);
                let formatted = self.config.timestamp_format().format(chunk.timestamp, session_start);
                // Right-align timestamp within the calculated width
                let padded = format!("{:>width$} ", formatted, width = timestamp_width);
                spans.push(Span::styled(padded, Theme::muted()));
            }

            // Content (with search highlighting if matches exist)
            let content = &chunk.encoded;
            // Calculate prefix width: "TX " or "RX " = 3 chars, timestamp + space
            let prefix_width = 3 + if self.config.show_timestamps { timestamp_width + 1 } else { 0 };
            let max_content_len = (inner.width as usize).saturating_sub(prefix_width);
            let display_content: String = if content.len() > max_content_len {
                format!("{}...", &content[..max_content_len.saturating_sub(3)])
            } else {
                content.to_string()
            };

            spans.push(Span::raw(display_content));

            let line = Line::from(spans);
            Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);

            y += 1;
        }

        // Help text
        if total == 0 {
            let help = "No data yet. Waiting for traffic...";
            Paragraph::new(help)
                .style(Theme::muted())
                .render(Rect::new(inner.x + 1, inner.y, inner.width - 2, 1), buf);
        }

        // Scrollbar - render on the border itself (not inside content area)
        if total > visible_height {
            // ScrollbarState expects:
            // - content_length: the number of scrollable positions (max_scroll + 1)
            // - position: current scroll position (0 to max_scroll)
            let max_scroll = total.saturating_sub(visible_height);
            let mut scrollbar_state = ScrollbarState::new(max_scroll + 1).position(scroll);
            // Position scrollbar on the right border, between the corners
            let scrollbar_area = Rect::new(
                area.x + area.width - 1,  // Right border column
                area.y + 1,               // Start below top border
                1,
                area.height.saturating_sub(2), // Exclude top and bottom borders
            );
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)  // No arrow at top
                .end_symbol(None)    // No arrow at bottom
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }

    fn draw_input_bar(&self, area: Rect, buf: &mut Buffer) {
        let (title, input_state) = if self.search_focused {
            ("Search", &self.search_input)
        } else {
            ("Send", &self.send_input)
        };

        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let mut state = input_state.clone();
        TextInput::new(&mut state)
            .block(block)
            .focused(true)
            .render(area, buf);
    }

    fn draw_config(
        &self,
        area: Rect,
        buf: &mut Buffer,
        serial_config: &SerialConfig,
        focus: Focus,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(5)])
            .split(area);

        // Connection info (read-only)
        let conn_block = Block::default()
            .title(" Connection ")
            .borders(Borders::ALL)
            .border_style(Theme::border());

        let conn_inner = conn_block.inner(chunks[0]);
        conn_block.render(chunks[0], buf);

        let conn_lines = vec![
            Line::from(vec![
                Span::styled("Baud:  ", Theme::muted()),
                Span::raw(serial_config.baud_rate.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Data:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.data_bits)),
            ]),
            Line::from(vec![
                Span::styled("Parity:", Theme::muted()),
                Span::raw(format!(" {:?}", serial_config.parity)),
            ]),
            Line::from(vec![
                Span::styled("Stop:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.stop_bits)),
            ]),
            Line::from(vec![
                Span::styled("Flow:  ", Theme::muted()),
                Span::raw(format!("{:?}", serial_config.flow_control)),
            ]),
        ];

        for (i, line) in conn_lines.into_iter().enumerate() {
            if i >= conn_inner.height as usize {
                break;
            }
            Paragraph::new(line).render(
                Rect::new(conn_inner.x, conn_inner.y + i as u16, conn_inner.width, 1),
                buf,
            );
        }

        // Traffic config
        let config_block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(if focus == Focus::Config {
                Theme::border_focused()
            } else {
                Theme::border()
            });

        ConfigPanel::new(TRAFFIC_CONFIG_SECTIONS, &self.config, &self.config_nav)
            .block(config_block)
            .focused(focus == Focus::Config)
            .render(chunks[1], buf);
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        focus: Focus,
        handle: &SessionHandle,
    ) -> Option<TrafficAction> {
        // Handle input modes
        if self.search_focused {
            return self.handle_search_key(key, handle);
        }
        if self.send_focused {
            return self.handle_send_key(key);
        }

        match focus {
            Focus::Main => self.handle_main_key(key, handle),
            Focus::Config => self.handle_config_key(key, handle),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        // Half-page scroll amount based on actual visible height
        let half_page = self.last_visible_height / 2;

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        let buffer = handle.buffer();
        let total = buffer.len();
        // Use the last known visible height for accurate scroll bounds
        let max_scroll = total.saturating_sub(self.last_visible_height);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.scroll = self.scroll.saturating_add(1).min(max_scroll);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.scroll = self.scroll.saturating_sub(1);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('d') if has_ctrl => {
                // Half-page down
                self.scroll = self.scroll.saturating_add(half_page).min(max_scroll);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('u') if has_ctrl => {
                // Half-page up
                self.scroll = self.scroll.saturating_sub(half_page);
                self.config.auto_scroll = false;
            }
            KeyCode::Char('g') => {
                self.scroll = 0;
                self.config.auto_scroll = false;
            }
            KeyCode::Char('G') => {
                self.scroll = max_scroll;
                self.config.auto_scroll = true;
            }
            KeyCode::Char('/') => {
                self.search_focused = true;
            }
            KeyCode::Char('s') => {
                self.send_focused = true;
            }
            KeyCode::Char('n') => {
                // Next search match
                drop(buffer); // Release borrow before getting mutable reference
                if let Some(chunk_idx) = handle.buffer_mut().goto_next_match() {
                    self.scroll = chunk_idx;
                    self.config.auto_scroll = false;
                }
            }
            KeyCode::Char('N') => {
                // Previous search match
                drop(buffer); // Release borrow before getting mutable reference
                if let Some(chunk_idx) = handle.buffer_mut().goto_prev_match() {
                    self.scroll = chunk_idx;
                    self.config.auto_scroll = false;
                }
            }
            _ => {}
        }
        None
    }

    fn handle_config_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        // Handle dropdown mode separately
        if self.config_nav.is_dropdown_open() {
            return self.handle_dropdown_key(key, handle);
        }

        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.config_nav
                    .next_field(TRAFFIC_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.config_nav
                    .prev_field(TRAFFIC_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(field) = self
                    .config_nav
                    .current_field(TRAFFIC_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                        self.sync_config_to_buffer(handle);
                        return Some(TrafficAction::RequestClear);
                    } else if field.kind.is_select() {
                        self.config_nav
                            .dropdown_prev(TRAFFIC_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                        self.sync_config_to_buffer(handle);
                        return Some(TrafficAction::RequestClear);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(field) = self
                    .config_nav
                    .current_field(TRAFFIC_CONFIG_SECTIONS, &self.config)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                        self.sync_config_to_buffer(handle);
                        return Some(TrafficAction::RequestClear);
                    } else if field.kind.is_select() {
                        self.config_nav
                            .dropdown_next(TRAFFIC_CONFIG_SECTIONS, &self.config);
                        let _ = self
                            .config_nav
                            .apply_dropdown_selection(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                        self.sync_config_to_buffer(handle);
                        return Some(TrafficAction::RequestClear);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(field) = self
                    .config_nav
                    .current_field(TRAFFIC_CONFIG_SECTIONS, &self.config)
                {
                    if field.kind.is_select() {
                        self.config_nav
                            .open_dropdown(TRAFFIC_CONFIG_SECTIONS, &self.config);
                    } else if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .config_nav
                            .toggle_current(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                        self.sync_config_to_buffer(handle);
                        return Some(TrafficAction::RequestClear);
                    }
                }
            }
            _ => {}
        }
        self.config_nav
            .sync_dropdown_index(TRAFFIC_CONFIG_SECTIONS, &self.config);
        None
    }

    fn handle_dropdown_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        // Ignore j/k with CTRL modifier (let it be consumed without action)
        let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
                self.config_nav
                    .dropdown_next(TRAFFIC_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
                self.config_nav
                    .dropdown_prev(TRAFFIC_CONFIG_SECTIONS, &self.config);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self
                    .config_nav
                    .apply_dropdown_selection(TRAFFIC_CONFIG_SECTIONS, &mut self.config);
                self.config_nav.close_dropdown();
                self.sync_config_to_buffer(handle);
                return Some(TrafficAction::RequestClear);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.config_nav.close_dropdown();
                self.config_nav
                    .sync_dropdown_index(TRAFFIC_CONFIG_SECTIONS, &self.config);
                return Some(TrafficAction::RequestClear);
            }
            _ => {}
        }
        None
    }

    fn handle_search_key(&mut self, key: KeyEvent, handle: &SessionHandle) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                let pattern = self.search_input.content.clone();
                if !pattern.is_empty() {
                    // Set search pattern on buffer
                    let mode = if self.config.pattern_mode_index == 1 {
                        PatternMode::Regex
                    } else {
                        PatternMode::Normal
                    };
                    if let Err(e) = handle.buffer_mut().set_search_pattern(&pattern, mode) {
                        return Some(TrafficAction::Toast(crate::widget::Toast::error(e)));
                    }
                } else {
                    // Clear search if empty
                    handle.buffer_mut().clear_search();
                }
                self.search_focused = false;
            }
            KeyCode::Esc => {
                self.search_focused = false;
                self.search_input.clear();
                handle.buffer_mut().clear_search();
            }
            _ => {
                self.search_input.handle_key(key);
            }
        }
        None
    }

    fn handle_send_key(&mut self, key: KeyEvent) -> Option<TrafficAction> {
        match key.code {
            KeyCode::Enter => {
                let data = self.send_input.take();
                if !data.is_empty() {
                    self.send_focused = false;
                    // Add newline for convenience
                    let mut bytes = data.into_bytes();
                    bytes.push(b'\n');
                    return Some(TrafficAction::Send(bytes));
                }
            }
            KeyCode::Esc => {
                self.send_focused = false;
                self.send_input.clear();
            }
            _ => {
                self.send_input.handle_key(key);
            }
        }
        None
    }
}

impl Default for TrafficView {
    fn default() -> Self {
        Self::new()
    }
}
