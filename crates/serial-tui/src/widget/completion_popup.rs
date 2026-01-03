//! Completion popup widget.
//!
//! Displays a list of completion options above or below an input field,
//! similar to neovim's completion menu.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget},
};

use crate::theme::Theme;

/// Maximum number of visible options in the popup.
const MAX_VISIBLE_OPTIONS: usize = 6;

/// What kind of completions are being shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionKind {
    /// Completing a command name (e.g., "help", "connect").
    #[default]
    Command,
    /// Completing an argument to a command.
    Argument,
}

/// Direction to render the popup relative to the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupDirection {
    /// Render above the input (default, for command bars at bottom).
    #[default]
    Above,
    /// Render below the input (for inputs at top of a region).
    Below,
}

/// State for the completion popup.
#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    /// Whether the popup is visible.
    pub visible: bool,
    /// List of completion options.
    pub options: Vec<String>,
    /// Currently selected index.
    pub selected: usize,
    /// Scroll offset for when there are more options than visible.
    scroll_offset: usize,
    /// What kind of completions are being shown.
    pub kind: CompletionKind,
}

impl CompletionState {
    /// Show the completion popup with the given options.
    ///
    /// The popup becomes visible only if there are options to display.
    pub fn show(&mut self, options: Vec<String>, kind: CompletionKind) {
        if options.is_empty() {
            self.hide();
            return;
        }
        self.visible = true;
        self.options = options;
        self.selected = 0;
        self.scroll_offset = 0;
        self.kind = kind;
    }

    /// Hide the completion popup and clear state.
    pub fn hide(&mut self) {
        self.visible = false;
        self.options.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.kind = CompletionKind::Command;
    }

    /// Move to the next completion option (Tab).
    pub fn next(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.options.len();
        self.ensure_visible();
    }

    /// Move to the previous completion option (Shift+Tab).
    pub fn prev(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.options.len() - 1
        } else {
            self.selected - 1
        };
        self.ensure_visible();
    }

    /// Get the currently selected completion value.
    pub fn selected_value(&self) -> Option<&str> {
        self.options.get(self.selected).map(|s| s.as_str())
    }

    /// Ensure the selected item is visible within the scroll window.
    fn ensure_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + MAX_VISIBLE_OPTIONS {
            self.scroll_offset = self.selected - MAX_VISIBLE_OPTIONS + 1;
        }
    }
}

/// Widget for rendering the completion popup.
///
/// This widget renders a popup above or below an input area, showing
/// available completions with the selected one highlighted.
pub struct CompletionPopup<'a> {
    state: &'a CompletionState,
    /// Y position of the input (popup renders above or below this).
    input_y: u16,
    /// Height of the input area (used for below positioning).
    input_height: u16,
    /// X position where the popup should start.
    input_x: u16,
    /// Whether to use disconnected (yellow) theming.
    disconnected: bool,
    /// Direction to render the popup.
    direction: PopupDirection,
}

impl<'a> CompletionPopup<'a> {
    /// Create a new completion popup.
    ///
    /// - `state`: The completion state to render
    /// - `input_y`: Y coordinate of the input line
    /// - `input_x`: X coordinate where the popup should align
    pub fn new(state: &'a CompletionState, input_y: u16, input_x: u16) -> Self {
        Self {
            state,
            input_y,
            input_height: 1,
            input_x,
            disconnected: false,
            direction: PopupDirection::Above,
        }
    }

    /// Use disconnected theming (yellow instead of cyan).
    pub fn disconnected(mut self, disconnected: bool) -> Self {
        self.disconnected = disconnected;
        self
    }

    /// Set the direction to render the popup.
    pub fn direction(mut self, direction: PopupDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set the height of the input area (for below positioning).
    pub fn input_height(mut self, height: u16) -> Self {
        self.input_height = height;
        self
    }
}

impl Widget for CompletionPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || self.state.options.is_empty() {
            return;
        }

        // Calculate popup dimensions
        let visible_count = self.state.options.len().min(MAX_VISIBLE_OPTIONS);
        let hint_line = 1; // For navigation hint
        let popup_height = visible_count as u16 + hint_line;

        // Find the longest option for width calculation
        let max_option_len = self
            .state
            .options
            .iter()
            .map(|s| s.len())
            .max()
            .unwrap_or(0);

        // Width: longest option + padding + scroll indicator space
        let hint_text = "[Tab]/[S-Tab]";
        let content_width = max_option_len.max(hint_text.len());
        let popup_width = (content_width + 4) as u16; // 2 chars padding each side

        // Position popup based on direction
        let popup_x = self.input_x.min(area.width.saturating_sub(popup_width));
        let popup_y = match self.direction {
            PopupDirection::Above => self.input_y.saturating_sub(popup_height),
            PopupDirection::Below => self.input_y + self.input_height,
        };

        // Ensure popup fits within screen
        match self.direction {
            PopupDirection::Above => {
                if popup_y < area.y || popup_height > self.input_y.saturating_sub(area.y) {
                    return; // Not enough space above input
                }
            }
            PopupDirection::Below => {
                let available_below = area.height.saturating_sub(popup_y.saturating_sub(area.y));
                if popup_height > available_below {
                    return; // Not enough space below input
                }
            }
        }

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear background
        Clear.render(popup_area, buf);

        // Draw border/background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Theme::base());
                }
            }
        }

        // Render options
        let visible_options: Vec<&String> = self
            .state
            .options
            .iter()
            .skip(self.state.scroll_offset)
            .take(MAX_VISIBLE_OPTIONS)
            .collect();

        for (i, option) in visible_options.iter().enumerate() {
            let y = popup_area.y + i as u16;
            let is_selected = self.state.scroll_offset + i == self.state.selected;

            // Build the line with padding
            let display_text = format!(" {} ", option);
            let style = if is_selected {
                Theme::selected()
            } else {
                Theme::base()
            };

            // Check if we need scroll indicators
            let has_more_above = self.state.scroll_offset > 0;
            let has_more_below =
                self.state.scroll_offset + MAX_VISIBLE_OPTIONS < self.state.options.len();

            let mut spans = vec![Span::styled(display_text, style)];

            // Add scroll indicator on the right edge
            if i == 0 && has_more_above {
                spans.push(Span::styled(" ↑", Theme::muted()));
            } else if i == visible_options.len() - 1 && has_more_below {
                spans.push(Span::styled(" ↓", Theme::muted()));
            }

            let line = Line::from(spans);
            let line_area = Rect::new(popup_area.x, y, popup_area.width, 1);

            // Fill background for selected item
            if is_selected {
                for x in popup_area.x..popup_area.x + popup_area.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(Theme::selected());
                    }
                }
            }

            Paragraph::new(line).render(line_area, buf);
        }

        // Render navigation hint at the bottom
        let hint_y = popup_area.y + visible_count as u16;
        let keybind_style = if self.disconnected {
            Theme::keybind_disconnected()
        } else {
            Theme::keybind()
        };
        let hint_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("[Tab]", keybind_style),
            Span::styled("/", Theme::muted()),
            Span::styled("[S-Tab]", keybind_style),
        ]);
        let hint_area = Rect::new(popup_area.x, hint_y, popup_area.width, 1);

        // Muted background for hint line
        for x in popup_area.x..popup_area.x + popup_area.width {
            if let Some(cell) = buf.cell_mut((x, hint_y)) {
                cell.set_style(Theme::muted());
            }
        }

        Paragraph::new(hint_line).render(hint_area, buf);
    }
}
