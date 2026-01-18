//! Help overlay widget.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget},
};
use serial_core::ui::config::{
    ConfigNav, FieldDef, FieldKind, FieldValue, Section, always_enabled, always_valid,
    always_visible,
};

use crate::{
    keybind::{KeyContext, Keybind, all_keybinds},
    settings::GlobalSettings,
    theme::Theme,
    widget::ConfigPanel,
};

/// Re-export GlobalSettings as AppSettings for backward compatibility within this module.
pub type AppSettings = GlobalSettings;

/// Tab in the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    Shortcuts,
    Settings,
    Commands,
}

impl HelpTab {
    pub const ALL: [HelpTab; 3] = [HelpTab::Shortcuts, HelpTab::Settings, HelpTab::Commands];

    pub fn title(self) -> &'static str {
        match self {
            HelpTab::Shortcuts => "Shortcuts",
            HelpTab::Settings => "Settings",
            HelpTab::Commands => "Commands",
        }
    }

    pub fn next(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::Settings,
            HelpTab::Settings => HelpTab::Commands,
            HelpTab::Commands => HelpTab::Shortcuts,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::Commands,
            HelpTab::Settings => HelpTab::Shortcuts,
            HelpTab::Commands => HelpTab::Settings,
        }
    }
}

// Settings panel definitions
const AUTO_SAVE_FORMAT_OPTIONS: &[&str] = &["Raw Binary", "Encoded Text"];
const AUTO_SAVE_ENCODING_OPTIONS: &[&str] = &["UTF-8", "ASCII", "Hex", "Binary"];
const PATTERN_MODE_OPTIONS: &[&str] = &["Normal", "Regex"];
const BUFFER_SIZE_OPTIONS: &[&str] = &["1 MB", "5 MB", "10 MB", "50 MB", "100 MB", "Unlimited"];
const MAX_SESSIONS_OPTIONS: &[&str] = &["5", "10", "20", "50", "100"];
const FILE_SAVE_SCOPE_OPTIONS: &[&str] = &["Existing Only", "New Only", "Existing + Continue"];

/// Max sessions values corresponding to MAX_SESSIONS_OPTIONS
const MAX_SESSIONS_VALUES: &[usize] = &[5, 10, 20, 50, 100];

static SETTINGS_SECTIONS: &[Section<AppSettings>] = &[
    Section {
        header: Some("Auto-Save (Crash Recovery)"),
        fields: &[
            FieldDef {
                id: "auto_save_enabled",
                label: "Enabled",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_enabled),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_enabled = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_max_sessions",
                label: "Max Sessions",
                kind: FieldKind::Select {
                    options: MAX_SESSIONS_OPTIONS,
                },
                get: |c| {
                    let idx = MAX_SESSIONS_VALUES
                        .iter()
                        .position(|&v| v == c.auto_save_max_sessions)
                        .unwrap_or(1);
                    FieldValue::OptionIndex(idx)
                },
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.auto_save_max_sessions =
                            MAX_SESSIONS_VALUES.get(i).copied().unwrap_or(10);
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled,
                parent_id: Some("auto_save_enabled"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_rx",
                label: "Save RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_rx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_rx = b;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled,
                parent_id: Some("auto_save_enabled"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_tx",
                label: "Save TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_tx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_tx = b;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled,
                parent_id: Some("auto_save_enabled"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_format",
                label: "Format",
                kind: FieldKind::Select {
                    options: AUTO_SAVE_FORMAT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.auto_save_format_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.auto_save_format_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled,
                parent_id: Some("auto_save_enabled"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_encoding",
                label: "Encoding",
                kind: FieldKind::Select {
                    options: AUTO_SAVE_ENCODING_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.auto_save_encoding_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.auto_save_encoding_index = i;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled && c.auto_save_format_index == 1,
                parent_id: Some("auto_save_format"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_timestamps",
                label: "Timestamps",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_timestamps),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_timestamps = b;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled && c.auto_save_format_index == 1,
                parent_id: Some("auto_save_format"),
                validate: always_valid,
            },
            FieldDef {
                id: "auto_save_direction",
                label: "Direction Markers",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.auto_save_direction),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.auto_save_direction = b;
                    }
                },
                visible: always_visible,
                enabled: |c| c.auto_save_enabled && c.auto_save_format_index == 1,
                parent_id: Some("auto_save_format"),
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("File Saving (User)"),
        fields: &[
            FieldDef {
                id: "file_save_scope",
                label: "Save Scope",
                kind: FieldKind::Select {
                    options: FILE_SAVE_SCOPE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.file_save_scope_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.file_save_scope_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_rx",
                label: "Save RX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.file_save_rx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.file_save_rx = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_tx",
                label: "Save TX",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.file_save_tx),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.file_save_tx = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_timestamps",
                label: "Timestamps",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.file_save_timestamps),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.file_save_timestamps = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "file_save_direction",
                label: "Direction Markers",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.file_save_direction),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.file_save_direction = b;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Pattern Matching"),
        fields: &[
            FieldDef {
                id: "search_mode",
                label: "Search Mode",
                kind: FieldKind::Select {
                    options: PATTERN_MODE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.search_mode_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.search_mode_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
            FieldDef {
                id: "filter_mode",
                label: "Filter Mode",
                kind: FieldKind::Select {
                    options: PATTERN_MODE_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.filter_mode_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.filter_mode_index = i;
                    }
                },
                visible: always_visible,
                enabled: always_enabled,
                parent_id: None,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Buffer"),
        fields: &[FieldDef {
            id: "buffer_size",
            label: "Buffer Size",
            kind: FieldKind::Select {
                options: BUFFER_SIZE_OPTIONS,
            },
            get: |c| FieldValue::OptionIndex(c.buffer_size_index),
            set: |c, v| {
                if let FieldValue::OptionIndex(i) = v {
                    c.buffer_size_index = i;
                }
            },
            visible: always_visible,
            enabled: always_enabled,
            parent_id: None,
            validate: always_valid,
        }],
    },
    Section {
        header: Some("System"),
        fields: &[FieldDef {
            id: "keep_awake",
            label: "Keep Awake",
            kind: FieldKind::Toggle,
            get: |c| FieldValue::Bool(c.keep_awake),
            set: |c, v| {
                if let FieldValue::Bool(b) = v {
                    c.keep_awake = b;
                }
            },
            visible: always_visible,
            enabled: always_enabled,
            parent_id: None,
            validate: always_valid,
        }],
    },
];

/// State for the help overlay.
#[derive(Debug, Default)]
pub struct HelpOverlayState {
    pub visible: bool,
    pub tab: HelpTab,
    pub scroll: usize,
    /// Global app settings.
    pub settings: AppSettings,
    /// Config panel navigation for settings tab.
    pub settings_nav: ConfigNav,
}

impl HelpOverlayState {
    /// Toggle visibility. Returns true if a full redraw is needed (overlay just closed).
    pub fn toggle(&mut self) -> bool {
        let was_visible = self.visible;
        self.visible = !self.visible;
        if self.visible {
            self.scroll = 0;
        }
        // Need redraw when closing (was visible, now hidden)
        was_visible && !self.visible
    }

    /// Hide the overlay. Returns true if a full redraw is needed.
    pub fn hide(&mut self) -> bool {
        let was_visible = self.visible;
        self.visible = false;
        was_visible
    }

    pub fn next_tab(&mut self) {
        self.tab = self.tab.next();
        self.scroll = 0;
    }

    pub fn prev_tab(&mut self) {
        self.tab = self.tab.prev();
        self.scroll = 0;
    }

    /// Handle key input. Returns true if a full redraw is needed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        const HALF_PAGE: usize = 15;

        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                return self.hide();
            }
            KeyCode::Tab | KeyCode::Char('l') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Don't switch tab if in dropdown
                if self.tab != HelpTab::Settings || !self.settings_nav.edit_mode.is_dropdown() {
                    self.next_tab();
                }
            }
            KeyCode::BackTab | KeyCode::Char('h')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Don't switch tab if in dropdown
                if self.tab != HelpTab::Settings || !self.settings_nav.edit_mode.is_dropdown() {
                    self.prev_tab();
                }
            }
            _ => {}
        }

        // Tab-specific handling
        match self.tab {
            HelpTab::Shortcuts | HelpTab::Commands => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.scroll = self.scroll.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.scroll = self.scroll.saturating_sub(1);
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll = self.scroll.saturating_add(HALF_PAGE);
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll = self.scroll.saturating_sub(HALF_PAGE);
                    }
                    KeyCode::Char('g') => {
                        self.scroll = 0;
                    }
                    KeyCode::Char('G') => {
                        self.scroll = 1000; // Will be clamped
                    }
                    _ => {}
                }
            }
            HelpTab::Settings => {
                return self.handle_settings_key(key);
            }
        }
        false
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        // Handle dropdown mode
        if self.settings_nav.edit_mode.is_dropdown() {
            return self.handle_settings_dropdown_key(key);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.settings_nav
                    .next_field(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_nav
                    .prev_field(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('h') | KeyCode::Left
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                if let Some(field) = self
                    .settings_nav
                    .current_field(SETTINGS_SECTIONS, &self.settings)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .settings_nav
                            .toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    } else if field.kind.is_select() {
                        self.settings_nav
                            .dropdown_prev(SETTINGS_SECTIONS, &self.settings);
                        let _ = self
                            .settings_nav
                            .apply_dropdown(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                if let Some(field) = self
                    .settings_nav
                    .current_field(SETTINGS_SECTIONS, &self.settings)
                {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .settings_nav
                            .toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    } else if field.kind.is_select() {
                        self.settings_nav
                            .dropdown_next(SETTINGS_SECTIONS, &self.settings);
                        let _ = self
                            .settings_nav
                            .apply_dropdown(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(field) = self
                    .settings_nav
                    .current_field(SETTINGS_SECTIONS, &self.settings)
                {
                    if field.kind.is_select() {
                        self.settings_nav
                            .open_dropdown(SETTINGS_SECTIONS, &self.settings);
                    } else if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self
                            .settings_nav
                            .toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            _ => {}
        }
        false
    }

    fn handle_settings_dropdown_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.settings_nav
                    .dropdown_next(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_nav
                    .dropdown_prev(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self
                    .settings_nav
                    .apply_dropdown(SETTINGS_SECTIONS, &mut self.settings);
                self.settings_nav.close_dropdown();
                return true; // Need redraw for dropdown close
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.settings_nav.close_dropdown();
                return true; // Need redraw for dropdown close
            }
            _ => {}
        }
        false
    }
}

/// Help overlay widget.
pub struct HelpOverlay<'a> {
    state: &'a HelpOverlayState,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(state: &'a HelpOverlayState) -> Self {
        Self { state }
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate overlay area (centered, 80% of screen or max 80x40)
        let width = (area.width * 80 / 100).min(80);
        let height = (area.height * 80 / 100).min(40);
        let x = area.x + (area.width - width) / 2;
        let y = area.y + (area.height - height) / 2;
        let overlay_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(overlay_area, buf);

        // Outer block
        let block = Block::default()
            .title(" Help ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let inner = block.inner(overlay_area);
        block.render(overlay_area, buf);

        if inner.height < 4 {
            return;
        }

        // Tabs
        let tabs_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let titles: Vec<&str> = HelpTab::ALL.iter().map(|t| t.title()).collect();
        let selected = HelpTab::ALL
            .iter()
            .position(|t| *t == self.state.tab)
            .unwrap_or(0);

        Tabs::new(titles)
            .select(selected)
            .style(Theme::tab_inactive())
            .highlight_style(Theme::tab_active())
            .divider("|")
            .render(tabs_area, buf);

        // Content area
        let content_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);

        match self.state.tab {
            HelpTab::Shortcuts => render_shortcuts(content_area, buf, self.state.scroll),
            HelpTab::Settings => render_settings(
                content_area,
                buf,
                &self.state.settings,
                &self.state.settings_nav,
            ),
            HelpTab::Commands => render_commands(content_area, buf, self.state.scroll),
        }
    }
}

fn render_shortcuts(area: Rect, buf: &mut Buffer, scroll: usize) {
    let keybinds = all_keybinds();

    // Group by context
    let contexts = [
        (KeyContext::Global, "Global"),
        (KeyContext::Navigation, "Navigation"),
        (KeyContext::PreConnect, "Pre-Connection"),
        (KeyContext::Connected, "Connected"),
        (KeyContext::Traffic, "Traffic View"),
        (KeyContext::Visual, "Visual Mode"),
        (KeyContext::Graph, "Graph View"),
        (KeyContext::FileSender, "File Sender"),
        (KeyContext::Input, "Input Mode"),
    ];

    let mut lines: Vec<Line> = Vec::new();

    for (context, name) in contexts {
        let context_keybinds: Vec<&Keybind> =
            keybinds.iter().filter(|k| k.context == context).collect();

        if context_keybinds.is_empty() {
            continue;
        }

        // Section header
        lines.push(Line::from(vec![Span::styled(name, Theme::title())]));

        for kb in context_keybinds {
            lines.push(Line::from(vec![
                Span::styled(format!("{:>12}", kb.key_display()), Theme::keybind()),
                Span::raw("  "),
                Span::styled(kb.description, Theme::keybind_desc()),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Apply scroll
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    Paragraph::new(visible_lines).render(area, buf);
}

fn render_settings(area: Rect, buf: &mut Buffer, settings: &AppSettings, nav: &ConfigNav) {
    // Instructions at top
    let help_lines = vec![
        Line::from(vec![
            Span::styled("j/k", Theme::keybind()),
            Span::raw(" Navigate  "),
            Span::styled("h/l", Theme::keybind()),
            Span::raw(" Change value  "),
            Span::styled("Enter", Theme::keybind()),
            Span::raw(" Select"),
        ]),
        Line::from(""),
    ];

    let help_height = help_lines.len() as u16;
    Paragraph::new(help_lines).render(Rect::new(area.x, area.y, area.width, help_height), buf);

    // Settings panel below
    let panel_area = Rect::new(
        area.x,
        area.y + help_height,
        area.width,
        area.height.saturating_sub(help_height),
    );

    ConfigPanel::new(SETTINGS_SECTIONS, settings, nav)
        .focused(true)
        .render(panel_area, buf);
}

fn render_commands(area: Rect, buf: &mut Buffer, scroll: usize) {
    let mut lines: Vec<Line> = vec![
        // Usage section
        Line::from(vec![Span::styled("Command Mode", Theme::title())]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled(":", Theme::keybind()),
            Span::raw(" to enter command mode. Use "),
            Span::styled("Tab", Theme::keybind()),
            Span::raw(" for completion."),
        ]),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("Enter", Theme::keybind()),
            Span::raw(" to execute, "),
            Span::styled("Esc", Theme::keybind()),
            Span::raw(" to cancel."),
        ]),
        Line::from(""),
        // Commands section
        Line::from(vec![Span::styled("Available Commands", Theme::title())]),
        Line::from(""),
    ];

    // Command list: (command, alias, description)
    let commands = [
        (
            ":connect <port>",
            ":c",
            "Open connection config modal for the specified port",
        ),
        (":disconnect", ":d", "Disconnect from the current session"),
        (":save <path>", ":w", "Save buffer contents to a file"),
        (":quit", ":q", "Quit the application"),
        (":help", ":h", "Open this help panel"),
        (":sessions", ":s", "Open the sessions manager modal"),
        (":settings", ":set", "Open the settings panel"),
    ];

    for (cmd, alias, desc) in commands {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<18}", cmd), Theme::keybind()),
            Span::styled(format!("{:<6}", alias), Theme::muted()),
            Span::raw(desc),
        ]));
    }

    // Apply scroll
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    Paragraph::new(visible_lines).render(area, buf);
}
