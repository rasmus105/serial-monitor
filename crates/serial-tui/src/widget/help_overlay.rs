//! Help overlay widget.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget},
};
use serial_core::ui::config::{ConfigPanelNav, FieldDef, FieldKind, FieldValue, Section, always_valid, always_visible};

use crate::{
    keybind::{KeyContext, Keybind, all_keybinds},
    theme::Theme,
    widget::ConfigPanel,
};

/// Tab in the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    Shortcuts,
    Settings,
    About,
}

impl HelpTab {
    pub const ALL: [HelpTab; 3] = [HelpTab::Shortcuts, HelpTab::Settings, HelpTab::About];

    pub fn title(self) -> &'static str {
        match self {
            HelpTab::Shortcuts => "Shortcuts",
            HelpTab::Settings => "Settings",
            HelpTab::About => "About",
        }
    }

    pub fn next(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::Settings,
            HelpTab::Settings => HelpTab::About,
            HelpTab::About => HelpTab::Shortcuts,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::About,
            HelpTab::Settings => HelpTab::Shortcuts,
            HelpTab::About => HelpTab::Settings,
        }
    }
}

/// Global application settings.
#[derive(Debug, Clone)]
pub struct AppSettings {
    /// Default timestamp format index.
    pub timestamp_format_index: usize,
    /// Whether to show tips on startup.
    pub show_tips: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            timestamp_format_index: 0, // Relative
            show_tips: true,
        }
    }
}

// Settings panel definitions
const TIMESTAMP_FORMAT_OPTIONS: &[&str] = &["Relative", "HH:MM:SS.mmm", "HH:MM:SS"];

static SETTINGS_SECTIONS: &[Section<AppSettings>] = &[
    Section {
        header: Some("Display"),
        fields: &[
            FieldDef {
                id: "timestamp_format",
                label: "Timestamp Format",
                kind: FieldKind::Select {
                    options: TIMESTAMP_FORMAT_OPTIONS,
                },
                get: |c| FieldValue::OptionIndex(c.timestamp_format_index),
                set: |c, v| {
                    if let FieldValue::OptionIndex(i) = v {
                        c.timestamp_format_index = i;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
    },
    Section {
        header: Some("Behavior"),
        fields: &[
            FieldDef {
                id: "show_tips",
                label: "Show Tips",
                kind: FieldKind::Toggle,
                get: |c| FieldValue::Bool(c.show_tips),
                set: |c, v| {
                    if let FieldValue::Bool(b) = v {
                        c.show_tips = b;
                    }
                },
                visible: always_visible,
                validate: always_valid,
            },
        ],
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
    pub settings_nav: ConfigPanelNav,
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
                if self.tab != HelpTab::Settings || !self.settings_nav.dropdown_open {
                    self.next_tab();
                }
            }
            KeyCode::BackTab | KeyCode::Char('h') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Don't switch tab if in dropdown
                if self.tab != HelpTab::Settings || !self.settings_nav.dropdown_open {
                    self.prev_tab();
                }
            }
            _ => {}
        }

        // Tab-specific handling
        match self.tab {
            HelpTab::Shortcuts | HelpTab::About => {
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
        if self.settings_nav.dropdown_open {
            return self.handle_settings_dropdown_key(key);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.settings_nav.next_field(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_nav.prev_field(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('h') | KeyCode::Left if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(field) = self.settings_nav.current_field(SETTINGS_SECTIONS, &self.settings) {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self.settings_nav.toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    } else if field.kind.is_select() {
                        self.settings_nav.dropdown_prev(SETTINGS_SECTIONS, &self.settings);
                        let _ = self.settings_nav.apply_dropdown_selection(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(field) = self.settings_nav.current_field(SETTINGS_SECTIONS, &self.settings) {
                    if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self.settings_nav.toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    } else if field.kind.is_select() {
                        self.settings_nav.dropdown_next(SETTINGS_SECTIONS, &self.settings);
                        let _ = self.settings_nav.apply_dropdown_selection(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(field) = self.settings_nav.current_field(SETTINGS_SECTIONS, &self.settings) {
                    if field.kind.is_select() {
                        self.settings_nav.open_dropdown(SETTINGS_SECTIONS, &self.settings);
                    } else if matches!(field.kind, FieldKind::Toggle) {
                        let _ = self.settings_nav.toggle_current(SETTINGS_SECTIONS, &mut self.settings);
                    }
                }
            }
            _ => {}
        }
        self.settings_nav.sync_dropdown_index(SETTINGS_SECTIONS, &self.settings);
        false
    }

    fn handle_settings_dropdown_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.settings_nav.dropdown_next(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_nav.dropdown_prev(SETTINGS_SECTIONS, &self.settings);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self.settings_nav.apply_dropdown_selection(SETTINGS_SECTIONS, &mut self.settings);
                self.settings_nav.close_dropdown();
                return true; // Need redraw for dropdown close
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.settings_nav.close_dropdown();
                self.settings_nav.sync_dropdown_index(SETTINGS_SECTIONS, &self.settings);
                return true; // Need redraw for dropdown close
            }
            _ => {}
        }
        false
    }

    /// Get the selected timestamp format.
    pub fn timestamp_format(&self) -> serial_core::ui::TimestampFormat {
        use serial_core::ui::TimestampFormat;
        match self.settings.timestamp_format_index {
            0 => TimestampFormat::Relative,
            1 => TimestampFormat::AbsoluteMillis,
            2 => TimestampFormat::Absolute,
            _ => TimestampFormat::Relative,
        }
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
        let selected = HelpTab::ALL.iter().position(|t| *t == self.state.tab).unwrap_or(0);

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
            HelpTab::Settings => render_settings(content_area, buf, &self.state.settings, &self.state.settings_nav),
            HelpTab::About => render_about(content_area, buf),
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
        (KeyContext::Graph, "Graph View"),
        (KeyContext::FileSender, "File Sender"),
        (KeyContext::Input, "Input Mode"),
    ];

    let mut lines: Vec<Line> = Vec::new();

    for (context, name) in contexts {
        let context_keybinds: Vec<&Keybind> = keybinds.iter().filter(|k| k.context == context).collect();

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

fn render_settings(area: Rect, buf: &mut Buffer, settings: &AppSettings, nav: &ConfigPanelNav) {
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
    Paragraph::new(help_lines).render(
        Rect::new(area.x, area.y, area.width, help_height),
        buf,
    );

    // Settings panel below
    let panel_area = Rect::new(area.x, area.y + help_height, area.width, area.height.saturating_sub(help_height));

    ConfigPanel::new(SETTINGS_SECTIONS, settings, nav)
        .focused(true)
        .render(panel_area, buf);
}

fn render_about(area: Rect, buf: &mut Buffer) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Serial Monitor",
            Theme::title(),
        )]),
        Line::from(""),
        Line::from("A terminal-based serial port monitor with"),
        Line::from("vim-like keybindings."),
        Line::from(""),
        Line::from(vec![Span::styled("Features:", Theme::highlight())]),
        Line::from("  - Real-time traffic monitoring"),
        Line::from("  - Graphing of parsed data"),
        Line::from("  - File sending with progress"),
        Line::from("  - Multiple encoding views"),
        Line::from("  - Search and filtering"),
        Line::from("  - Auto-save for crash recovery"),
        Line::from(""),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("?", Theme::keybind()),
            Span::raw(" again or "),
            Span::styled("Esc", Theme::keybind()),
            Span::raw(" to close"),
        ]),
    ];

    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .render(area, buf);
}
