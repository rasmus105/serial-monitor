//! Help overlay widget.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget},
};

use crate::{
    keybind::{KeyContext, Keybind, all_keybinds},
    theme::Theme,
};

/// Tab in the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    Shortcuts,
    Options,
    About,
}

impl HelpTab {
    pub const ALL: [HelpTab; 3] = [HelpTab::Shortcuts, HelpTab::Options, HelpTab::About];

    pub fn title(self) -> &'static str {
        match self {
            HelpTab::Shortcuts => "Shortcuts",
            HelpTab::Options => "Options",
            HelpTab::About => "About",
        }
    }

    pub fn next(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::Options,
            HelpTab::Options => HelpTab::About,
            HelpTab::About => HelpTab::Shortcuts,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            HelpTab::Shortcuts => HelpTab::About,
            HelpTab::Options => HelpTab::Shortcuts,
            HelpTab::About => HelpTab::Options,
        }
    }
}

/// State for the help overlay.
#[derive(Debug, Default)]
pub struct HelpOverlayState {
    pub visible: bool,
    pub tab: HelpTab,
    pub scroll: usize,
}

impl HelpOverlayState {
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll = 0;
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn next_tab(&mut self) {
        self.tab = self.tab.next();
        self.scroll = 0;
    }

    pub fn prev_tab(&mut self) {
        self.tab = self.tab.prev();
        self.scroll = 0;
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
            HelpTab::Options => render_options(content_area, buf),
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

fn render_options(area: Rect, buf: &mut Buffer) {
    let lines = vec![
        Line::from(vec![Span::styled("Pattern Mode", Theme::title())]),
        Line::from(""),
        Line::from("  Normal: Literal text matching"),
        Line::from("  Regex:  Regular expression matching"),
        Line::from(""),
        Line::from(vec![Span::styled("Auto-Save", Theme::title())]),
        Line::from(""),
        Line::from("  Auto-save writes data to a temporary file"),
        Line::from("  for crash recovery. Configure location and"),
        Line::from("  format in the config panel."),
        Line::from(""),
        Line::from(vec![Span::styled("Encoding", Theme::title())]),
        Line::from(""),
        Line::from("  UTF-8:   Unicode text"),
        Line::from("  ASCII:   7-bit ASCII (invalid bytes shown as dots)"),
        Line::from("  Hex:     Hexadecimal byte values"),
        Line::from("  Binary:  Binary byte values"),
    ];

    Paragraph::new(lines).render(area, buf);
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
