//! Confirmation overlay widget.
//!
//! A simple modal dialog that asks the user to confirm an action.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::theme::Theme;

/// State for a confirmation dialog.
#[derive(Debug, Clone, Default)]
pub struct ConfirmState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// The prompt message to display.
    pub message: String,
}

impl ConfirmState {
    /// Show the confirmation dialog with the given message.
    pub fn show(&mut self, message: impl Into<String>) {
        self.visible = true;
        self.message = message.into();
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }
}

/// Confirmation overlay widget.
pub struct ConfirmOverlay<'a> {
    state: &'a ConfirmState,
}

impl<'a> ConfirmOverlay<'a> {
    pub fn new(state: &'a ConfirmState) -> Self {
        Self { state }
    }
}

impl Widget for ConfirmOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate overlay size (compact, centered)
        let message_len = self.state.message.len() as u16;
        // Width: message + padding, plus room for the hint line
        let hint = "[y]es  [n]o";
        let content_width = message_len.max(hint.len() as u16);
        let width = (content_width + 6)
            .min(area.width.saturating_sub(4))
            .max(20);
        let height = 5; // border + message + blank + hint + border

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let overlay_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(overlay_area, buf);

        // Block with border
        let block = Block::default()
            .title(" Confirm ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let inner = block.inner(overlay_area);
        block.render(overlay_area, buf);

        // Content: message and hint
        let lines = vec![
            Line::from(self.state.message.as_str()),
            Line::from(""),
            Line::from(vec![
                Span::styled("[y]", Theme::keybind()),
                Span::raw("es  "),
                Span::styled("[n]", Theme::keybind()),
                Span::raw("o"),
            ]),
        ];

        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .render(inner, buf);
    }
}
