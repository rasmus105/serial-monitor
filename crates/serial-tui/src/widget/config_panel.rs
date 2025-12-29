//! Config panel widget using serial-core's declarative config system.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use serial_core::ui::config::{ConfigPanelNav, FieldKind, FieldValue, Section};

use crate::theme::Theme;

/// Config panel widget.
pub struct ConfigPanel<'a, T: 'static> {
    sections: &'a [Section<T>],
    data: &'a T,
    nav: &'a ConfigPanelNav,
    focused: bool,
    block: Option<Block<'a>>,
    /// Whether to show config as read-only (grayed out).
    read_only: bool,
}

impl<'a, T: 'static> ConfigPanel<'a, T> {
    pub fn new(sections: &'a [Section<T>], data: &'a T, nav: &'a ConfigPanelNav) -> Self {
        Self {
            sections,
            data,
            nav,
            focused: false,
            block: None,
            read_only: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Get info needed to render dropdown overlay after the main panel.
    /// Returns (field_y, options, selected_index) if dropdown is open.
    fn get_dropdown_info(&self, inner: Rect) -> Option<(u16, &'static [&'static str], usize)> {
        if !self.nav.dropdown_open || !self.focused {
            return None;
        }

        let mut y = inner.y;
        let mut field_index = 0;

        for section in self.sections {
            if y >= inner.y + inner.height {
                break;
            }

            // Account for section header
            if section.header.is_some() {
                y += 2; // header + separator
            }

            for field in section.fields {
                if y >= inner.y + inner.height {
                    break;
                }

                if !(field.visible)(self.data) {
                    continue;
                }

                if self.nav.selected == field_index {
                    if let FieldKind::Select { options } = &field.kind {
                        return Some((y, options, self.nav.dropdown_index));
                    }
                }

                y += 1;
                field_index += 1;
            }

            y += 1; // spacing between sections
        }

        None
    }
}

impl<T: 'static> Widget for ConfigPanel<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < 4 || inner.height < 1 {
            return;
        }

        // Get dropdown info before iterating (needed for overlay)
        let dropdown_info = self.get_dropdown_info(inner);

        let mut y = inner.y;
        let mut field_index = 0;

        for section in self.sections {
            if y >= inner.y + inner.height {
                break;
            }

            // Render section header if present
            if let Some(header) = &section.header {
                let header_style = if self.read_only {
                    Theme::muted()
                } else {
                    Theme::title()
                };

                let line = Line::from(vec![Span::styled(header.to_string(), header_style)]);
                Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
                y += 1;

                // Separator line
                if y < inner.y + inner.height {
                    let sep = "─".repeat(inner.width as usize);
                    Paragraph::new(sep)
                        .style(Theme::muted())
                        .render(Rect::new(inner.x, y, inner.width, 1), buf);
                    y += 1;
                }
            }

            // Render fields
            for field in section.fields {
                if y >= inner.y + inner.height {
                    break;
                }

                if !(field.visible)(self.data) {
                    continue;
                }

                let is_selected = self.focused && self.nav.selected == field_index;
                let is_dropdown_open = is_selected && self.nav.dropdown_open;
                let value = (field.get)(self.data);

                let (label_style, value_style) = if self.read_only {
                    (Theme::muted(), Theme::muted())
                } else if is_selected {
                    (
                        Style::default().add_modifier(Modifier::BOLD),
                        Theme::highlight(),
                    )
                } else {
                    (Theme::default(), Theme::default())
                };

                // Format field based on kind
                let value_str = match (&field.kind, &value) {
                    (FieldKind::Toggle, FieldValue::Bool(b)) => {
                        if *b { "[x]" } else { "[ ]" }.to_string()
                    }
                    (FieldKind::Select { options }, FieldValue::OptionIndex(i)) => {
                        let option = options.get(*i).unwrap_or(&"?");
                        if is_dropdown_open {
                            // Show dropdown indicator when open
                            format!("[{}] ▼", option)
                        } else {
                            format!("[{}]", option)
                        }
                    }
                    (FieldKind::TextInput { .. }, FieldValue::String(s)) => s.to_string(),
                    (FieldKind::NumericInput { .. }, FieldValue::Usize(n)) => n.to_string(),
                    (FieldKind::NumericInput { .. }, FieldValue::Isize(n)) => n.to_string(),
                    (FieldKind::NumericInput { .. }, FieldValue::Float(f)) => format!("{:.2}", f),
                    _ => "?".to_string(),
                };

                // Calculate layout: label on left, value on right
                let label = &field.label;
                let available = inner.width as usize;
                let value_width = value_str.len().min(available / 2);
                let label_width = available.saturating_sub(value_width + 1);

                let label_display: String = if label.len() > label_width {
                    format!("{}...", &label[..label_width.saturating_sub(3)])
                } else {
                    (*label).to_string()
                };

                let padding = available.saturating_sub(label_display.len() + value_str.len());

                let line = Line::from(vec![
                    Span::styled(label_display, label_style),
                    Span::raw(" ".repeat(padding)),
                    Span::styled(value_str, value_style),
                ]);

                if is_selected {
                    // Highlight the entire row
                    let highlight_area = Rect::new(inner.x, y, inner.width, 1);
                    for x in highlight_area.x..highlight_area.x + highlight_area.width {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_bg(Theme::SELECTION);
                        }
                    }
                }

                Paragraph::new(line)
                    .wrap(Wrap { trim: false })
                    .render(Rect::new(inner.x, y, inner.width, 1), buf);

                y += 1;
                field_index += 1;
            }

            // Add spacing between sections
            y += 1;
        }

        // Render dropdown overlay if open
        if let Some((field_y, options, selected_idx)) = dropdown_info {
            render_dropdown_overlay(buf, inner, field_y, options, selected_idx);
        }
    }
}

/// Render a dropdown overlay below the selected field.
fn render_dropdown_overlay(
    buf: &mut Buffer,
    panel_area: Rect,
    field_y: u16,
    options: &[&str],
    selected_idx: usize,
) {
    // Calculate dropdown dimensions
    let max_option_len = options.iter().map(|s| s.len()).max().unwrap_or(10);
    let dropdown_width = (max_option_len + 4).min(panel_area.width as usize) as u16;
    let dropdown_height = (options.len() + 2).min(10) as u16; // +2 for border

    // Position: below the field, right-aligned to panel
    let dropdown_x = panel_area.x + panel_area.width - dropdown_width;
    let dropdown_y = field_y + 1;

    // Ensure it fits in the terminal
    let dropdown_area = Rect::new(
        dropdown_x,
        dropdown_y,
        dropdown_width,
        dropdown_height.min(panel_area.y + panel_area.height - dropdown_y),
    );

    if dropdown_area.height < 3 {
        return; // Not enough space
    }

    // Clear and draw border
    Clear.render(dropdown_area, buf);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused());

    let inner = block.inner(dropdown_area);
    block.render(dropdown_area, buf);

    // Render options
    let visible_options = (inner.height as usize).min(options.len());
    let scroll_offset = if selected_idx >= visible_options {
        selected_idx - visible_options + 1
    } else {
        0
    };

    for (i, option) in options.iter().enumerate().skip(scroll_offset).take(visible_options) {
        let y = inner.y + (i - scroll_offset) as u16;
        let is_selected = i == selected_idx;

        // Highlight selected option
        if is_selected {
            for x in inner.x..inner.x + inner.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(Theme::PRIMARY);
                    cell.set_fg(Theme::BG);
                }
            }
        }

        let prefix = if is_selected { ">" } else { " " };
        let line = Line::from(format!("{} {}", prefix, option));

        let style = if is_selected {
            Style::default().fg(Theme::BG).add_modifier(Modifier::BOLD)
        } else {
            Theme::default()
        };

        Paragraph::new(line)
            .style(style)
            .render(Rect::new(inner.x, y, inner.width, 1), buf);
    }
}
