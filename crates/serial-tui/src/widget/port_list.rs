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
#[derive(Debug, Default, Clone)]
pub struct PortListState {
    pub ports: Vec<PortInfo>,
    pub list_state: ListState,
    /// Search/filter pattern (highlights matching ports).
    pub search_pattern: String,
    /// Indices of ports matching the search pattern.
    pub matching_indices: Vec<usize>,
    /// Current match index for n/N navigation.
    pub current_match: usize,
}

impl PortListState {
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
        // Re-apply search if active
        if !self.search_pattern.is_empty() {
            self.update_matches();
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

    /// Set search pattern and update matches.
    pub fn set_search(&mut self, pattern: &str) {
        self.search_pattern = pattern.to_lowercase();
        self.update_matches();
    }

    /// Clear search pattern.
    pub fn clear_search(&mut self) {
        self.search_pattern.clear();
        self.matching_indices.clear();
        self.current_match = 0;
    }

    /// Update matching indices based on current search pattern.
    fn update_matches(&mut self) {
        self.matching_indices.clear();
        if self.search_pattern.is_empty() {
            return;
        }

        let pattern = &self.search_pattern;
        for (i, port) in self.ports.iter().enumerate() {
            // Match against port name, product, manufacturer
            let name_match = port.name.to_lowercase().contains(pattern);
            let product_match = port
                .product
                .as_ref()
                .is_some_and(|p| p.to_lowercase().contains(pattern));
            let manufacturer_match = port
                .manufacturer
                .as_ref()
                .is_some_and(|m| m.to_lowercase().contains(pattern));

            if name_match || product_match || manufacturer_match {
                self.matching_indices.push(i);
            }
        }

        // Reset current match index
        self.current_match = 0;

        // Jump to first match if there is one
        if let Some(&first_match) = self.matching_indices.first() {
            self.list_state.select(Some(first_match));
        }
    }

    /// Go to next matching port.
    pub fn goto_next_match(&mut self) {
        if self.matching_indices.is_empty() {
            return;
        }
        self.current_match = (self.current_match + 1) % self.matching_indices.len();
        if let Some(&idx) = self.matching_indices.get(self.current_match) {
            self.list_state.select(Some(idx));
        }
    }

    /// Go to previous matching port.
    pub fn goto_prev_match(&mut self) {
        if self.matching_indices.is_empty() {
            return;
        }
        self.current_match = if self.current_match == 0 {
            self.matching_indices.len() - 1
        } else {
            self.current_match - 1
        };
        if let Some(&idx) = self.matching_indices.get(self.current_match) {
            self.list_state.select(Some(idx));
        }
    }

    /// Check if a port index is a search match.
    pub fn is_match(&self, index: usize) -> bool {
        self.matching_indices.contains(&index)
    }

    /// Check if search is active.
    pub fn has_search(&self) -> bool {
        !self.search_pattern.is_empty()
    }

    /// Get search status message.
    pub fn search_status(&self) -> String {
        if self.search_pattern.is_empty() {
            String::new()
        } else if self.matching_indices.is_empty() {
            "No matches".to_string()
        } else {
            format!(
                "{}/{} matches",
                self.current_match + 1,
                self.matching_indices.len()
            )
        }
    }
}

/// Port list widget.
#[derive(Default)]
pub struct PortList<'a> {
    block: Option<Block<'a>>,
    focused: bool,
}

impl<'a> PortList<'a> {
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}


impl StatefulWidget for PortList<'_> {
    type State = PortListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let has_search = state.has_search();

        let items: Vec<ListItem> = state
            .ports
            .iter()
            .enumerate()
            .map(|(i, port)| {
                let is_match = has_search && state.is_match(i);

                // Style port name based on match status
                let name_style = if is_match {
                    Theme::search_match()
                } else {
                    Theme::highlight()
                };

                let mut lines = vec![Line::from(vec![
                    Span::styled(&port.name, name_style),
                ])];

                // Add details on second line if available
                let mut details = Vec::new();
                if let Some(ref product) = port.product {
                    details.push(product.clone());
                }
                if let Some(ref manufacturer) = port.manufacturer {
                    details.push(format!("({})", manufacturer));
                }
                if let Some(vid) = port.vid
                    && let Some(pid) = port.pid
                {
                    details.push(format!("[{:04x}:{:04x}]", vid, pid));
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
