//! Text input widget with cursor support.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

/// A text input field.
#[derive(Debug, Default, Clone)]
pub struct TextInputState {
    /// Current input content
    pub content: String,
    /// Cursor position (byte index)
    cursor: usize,
    /// Horizontal scroll offset
    scroll: usize,
    /// Placeholder text
    pub placeholder: String,
}

impl TextInputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self.cursor = self.content.len();
        self
    }

    /// Handle a key event. Returns true if the event was handled.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) => {
                self.insert_char(c);
                true
            }
            KeyCode::Backspace => {
                self.delete_char_before();
                true
            }
            KeyCode::Delete => {
                self.delete_char_after();
                true
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.move_word_left();
                } else {
                    self.move_left();
                }
                true
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.move_word_right();
                } else {
                    self.move_right();
                }
                true
            }
            KeyCode::Home => {
                self.move_start();
                true
            }
            KeyCode::End => {
                self.move_end();
                true
            }
            _ => false,
        }
    }

    /// Clear the input.
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Take the content and clear the input.
    pub fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.content);
        self.cursor = 0;
        self.scroll = 0;
        content
    }

    fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.prev_char_boundary();
            self.content.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    fn delete_char_after(&mut self) {
        if self.cursor < self.content.len() {
            let next = self.next_char_boundary();
            self.content.drain(self.cursor..next);
        }
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_char_boundary();
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            self.cursor = self.next_char_boundary();
        }
    }

    fn move_start(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = self.content.len();
    }

    fn move_word_left(&mut self) {
        // Skip whitespace, then skip word characters
        while self.cursor > 0 {
            let prev = self.prev_char_boundary();
            let c = self.content[prev..self.cursor].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor = prev;
        }
        while self.cursor > 0 {
            let prev = self.prev_char_boundary();
            let c = self.content[prev..self.cursor].chars().next().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor = prev;
        }
    }

    fn move_word_right(&mut self) {
        // Skip word characters, then skip whitespace
        while self.cursor < self.content.len() {
            let c = self.content[self.cursor..].chars().next().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor = self.next_char_boundary();
        }
        while self.cursor < self.content.len() {
            let c = self.content[self.cursor..].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor = self.next_char_boundary();
        }
    }

    fn prev_char_boundary(&self) -> usize {
        let mut idx = self.cursor.saturating_sub(1);
        while idx > 0 && !self.content.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn next_char_boundary(&self) -> usize {
        let mut idx = self.cursor + 1;
        while idx < self.content.len() && !self.content.is_char_boundary(idx) {
            idx += 1;
        }
        idx.min(self.content.len())
    }

    /// Update scroll to ensure cursor is visible.
    fn update_scroll(&mut self, width: usize) {
        let cursor_pos = self.content[..self.cursor].width();
        if cursor_pos < self.scroll {
            self.scroll = cursor_pos;
        } else if cursor_pos >= self.scroll + width {
            self.scroll = cursor_pos.saturating_sub(width) + 1;
        }
    }
}

/// Text input widget.
pub struct TextInput<'a> {
    state: &'a mut TextInputState,
    focused: bool,
    block: Option<Block<'a>>,
    style: Style,
}

impl<'a> TextInput<'a> {
    pub fn new(state: &'a mut TextInputState) -> Self {
        Self {
            state,
            focused: false,
            block: None,
            style: Style::default(),
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

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl Widget for TextInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = &self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Update scroll
        self.state.update_scroll(inner.width as usize - 1);

        let display_text = if self.state.content.is_empty() {
            &self.state.placeholder
        } else {
            &self.state.content
        };

        let style = if self.state.content.is_empty() {
            Theme::muted()
        } else {
            self.style
        };

        // Calculate visible portion
        let text_width = display_text.width();
        let visible_start = self.state.scroll.min(text_width);

        // Get visible text by character width
        let mut char_start = 0;
        let mut width_count = 0;
        for (i, c) in display_text.char_indices() {
            if width_count >= visible_start {
                char_start = i;
                break;
            }
            width_count += c.width().unwrap_or(0);
        }

        let visible_text: String = display_text[char_start..]
            .chars()
            .take_while(|c| {
                width_count += c.width().unwrap_or(0);
                width_count <= visible_start + inner.width as usize
            })
            .collect();

        Paragraph::new(visible_text)
            .style(style)
            .render(inner, buf);

        // Draw cursor if focused and not showing placeholder
        if self.focused && !self.state.content.is_empty() {
            let cursor_x =
                self.state.content[..self.state.cursor].width() - self.state.scroll;
            if cursor_x < inner.width as usize {
                let x = inner.x + cursor_x as u16;
                let y = inner.y;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Style::default().bg(Theme::HIGHLIGHT));
                }
            }
        } else if self.focused {
            // Show cursor at start for empty input
            if let Some(cell) = buf.cell_mut((inner.x, inner.y)) {
                cell.set_style(Style::default().bg(Theme::HIGHLIGHT));
            }
        }
    }
}
