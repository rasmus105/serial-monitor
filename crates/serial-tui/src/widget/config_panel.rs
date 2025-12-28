//! Config panel widget using serial-core's declarative config system.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
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
}

impl<T: 'static> Widget for ConfigPanel<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < 4 || inner.height < 1 {
            return;
        }

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
                Paragraph::new(line).render(
                    Rect::new(inner.x, y, inner.width, 1),
                    buf,
                );
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
                        format!("< {} >", options.get(*i).unwrap_or(&"?"))
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
    }
}
