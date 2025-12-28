//! Port list widget for displaying available serial ports.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, StatefulWidget},
};
use serial_core::PortInfo;

use crate::theme::Theme;

/// State for the port list.
#[derive(Debug, Default)]
pub struct PortListState {
    pub ports: Vec<PortInfo>,
    pub list_state: ListState,
}

impl PortListState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_ports(&mut self, ports: Vec<PortInfo>) {
        self.ports = ports;
        // Reset selection if out of bounds
        if let Some(selected) = self.list_state.selected() {
            if selected >= self.ports.len() {
                self.list_state.select(if self.ports.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
        } else if !self.ports.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn selected(&self) -> Option<&PortInfo> {
        self.list_state.selected().and_then(|i| self.ports.get(i))
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.selected().map(|p| p.name.as_str())
    }

    pub fn select_next(&mut self) {
        if self.ports.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.ports.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn select_prev(&mut self) {
        if self.ports.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.ports.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
}

/// Port list widget.
pub struct PortList<'a> {
    block: Option<Block<'a>>,
    focused: bool,
}

impl<'a> PortList<'a> {
    pub fn new() -> Self {
        Self {
            block: None,
            focused: false,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Default for PortList<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for PortList<'_> {
    type State = PortListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let items: Vec<ListItem> = state
            .ports
            .iter()
            .map(|port| {
                let mut lines = vec![Line::from(vec![
                    Span::styled(&port.name, Theme::highlight()),
                ])];

                // Add details on second line if available
                let mut details = Vec::new();
                if let Some(ref product) = port.product {
                    details.push(product.clone());
                }
                if let Some(ref manufacturer) = port.manufacturer {
                    details.push(format!("({})", manufacturer));
                }
                if let Some(vid) = port.vid {
                    if let Some(pid) = port.pid {
                        details.push(format!("[{:04x}:{:04x}]", vid, pid));
                    }
                }

                if !details.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {}", details.join(" ")),
                        Theme::muted(),
                    )]));
                }

                ListItem::new(lines)
            })
            .collect();

        let highlight_style = if self.focused {
            Style::default()
                .bg(Theme::SELECTION)
                .add_modifier(Modifier::BOLD)
        } else {
            Theme::selected()
        };

        let mut list = List::new(items)
            .highlight_style(highlight_style)
            .highlight_symbol("> ");

        if let Some(block) = self.block {
            list = list.block(block);
        }

        StatefulWidget::render(list, area, buf, &mut state.list_state);
    }
}
