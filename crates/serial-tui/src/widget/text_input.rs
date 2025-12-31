//! Text input widget with cursor support.

use std::path::Path;

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
    /// Path completion state (for tab completion).
    completion: Option<PathCompletionState>,
}

/// State for cycling through path completions.
#[derive(Debug, Clone)]
struct PathCompletionState {
    /// Available completions.
    matches: Vec<String>,
    /// Current index in matches.
    index: usize,
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
        // Handle Ctrl+<key> sequences
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('a') => {
                    self.move_start();
                    true
                }
                KeyCode::Char('e') => {
                    self.move_end();
                    true
                }
                KeyCode::Char('b') => {
                    self.move_left();
                    true
                }
                KeyCode::Char('f') => {
                    self.move_right();
                    true
                }
                KeyCode::Char('h') => {
                    self.delete_char_before();
                    true
                }
                KeyCode::Char('w') => {
                    self.delete_word_before();
                    true
                }
                KeyCode::Char('u') => {
                    self.delete_to_start();
                    true
                }
                KeyCode::Char('k') => {
                    self.delete_to_end();
                    true
                }
                KeyCode::Left => {
                    self.move_word_left();
                    true
                }
                KeyCode::Right => {
                    self.move_word_right();
                    true
                }
                _ => false,
            };
        }

        // Handle Alt+<key> sequences
        if key.modifiers.contains(KeyModifiers::ALT) {
            return match key.code {
                KeyCode::Char('b') => {
                    self.move_word_left();
                    true
                }
                KeyCode::Char('f') => {
                    self.move_word_right();
                    true
                }
                _ => false,
            };
        }

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
                self.move_left();
                true
            }
            KeyCode::Right => {
                self.move_right();
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
        self.completion = None;
    }

    /// Set the content and move cursor to end.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.content.len();
        self.scroll = 0;
        self.completion = None;
    }

    /// Take the content and clear the input.
    pub fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.content);
        self.cursor = 0;
        self.scroll = 0;
        self.completion = None;
        content
    }

    /// Get the cursor position (display width from start).
    pub fn cursor_display_pos(&self) -> usize {
        self.content[..self.cursor].width()
    }

    fn insert_char(&mut self, c: char) {
        self.clear_completion();
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            self.clear_completion();
            let prev = self.prev_char_boundary();
            self.content.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    fn delete_char_after(&mut self) {
        if self.cursor < self.content.len() {
            self.clear_completion();
            let next = self.next_char_boundary();
            self.content.drain(self.cursor..next);
        }
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.clear_completion();
            self.cursor = self.prev_char_boundary();
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            self.clear_completion();
            self.cursor = self.next_char_boundary();
        }
    }

    fn move_start(&mut self) {
        self.clear_completion();
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.clear_completion();
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

    fn delete_word_before(&mut self) {
        self.clear_completion();
        let end = self.cursor;
        self.move_word_left();
        if self.cursor < end {
            self.content.drain(self.cursor..end);
        }
    }

    fn delete_to_start(&mut self) {
        if self.cursor > 0 {
            self.clear_completion();
            self.content.drain(..self.cursor);
            self.cursor = 0;
        }
    }

    fn delete_to_end(&mut self) {
        if self.cursor < self.content.len() {
            self.clear_completion();
            self.content.truncate(self.cursor);
        }
    }

    fn move_word_right(&mut self) {
        self.clear_completion();
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

    /// Clear any active completion state.
    /// Should be called when the user types or moves cursor.
    fn clear_completion(&mut self) {
        self.completion = None;
    }

    /// Attempt path completion. Returns true if completion was performed.
    ///
    /// Behavior:
    /// - First Tab: Complete to longest common prefix, or cycle if already completed
    /// - Subsequent Tabs: Cycle through available matches
    /// - Any other input clears completion state
    pub fn complete_path(&mut self) -> bool {
        // If we have active completion state, cycle to next match
        if let Some(ref mut state) = self.completion {
            if !state.matches.is_empty() {
                state.index = (state.index + 1) % state.matches.len();
                self.content = state.matches[state.index].clone();
                self.cursor = self.content.len();
                return true;
            }
            return false;
        }

        // Start new completion
        let matches = find_path_completions(&self.content);
        if matches.is_empty() {
            return false;
        }

        if matches.len() == 1 {
            // Single match - complete it directly
            let completed = &matches[0];
            self.content = completed.clone();
            self.cursor = self.content.len();
            // Store state so subsequent tabs can cycle (if it's a directory)
            self.completion = Some(PathCompletionState {
                matches,
                index: 0,
            });
            return true;
        }

        // Multiple matches - complete to common prefix first
        let common = longest_common_prefix(&matches);
        if common.len() > self.content.len() {
            // We can extend the input
            self.content = common;
            self.cursor = self.content.len();
            self.completion = Some(PathCompletionState {
                matches,
                index: 0,
            });
        } else {
            // Already at common prefix, start cycling
            self.completion = Some(PathCompletionState {
                matches,
                index: 0,
            });
            // Apply first match
            if let Some(ref state) = self.completion {
                self.content = state.matches[0].clone();
                self.cursor = self.content.len();
            }
        }
        true
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

// =============================================================================
// Path completion helpers
// =============================================================================

/// Find all path completions for the given input.
pub fn find_path_completions(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }

    let path = Path::new(input);

    // Expand ~ to home directory
    let expanded = if input.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            if input == "~" {
                home
            } else if let Some(rest) = input.strip_prefix("~/") {
                home.join(rest)
            } else {
                // ~username style - not supported, treat literally
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    // Determine parent directory and prefix to match
    let (parent, prefix) = if expanded.is_dir() && input.ends_with('/') {
        // Input is a directory ending with /, list its contents
        (expanded.clone(), String::new())
    } else if let Some(parent) = expanded.parent() {
        let prefix = expanded
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        (parent.to_path_buf(), prefix)
    } else {
        return Vec::new();
    };

    // Read directory entries
    let entries = match std::fs::read_dir(&parent) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut matches: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let full_path = parent.join(&name);

            // Build the completion string, preserving ~ if used
            let completed = if input.starts_with("~/") {
                let home_prefix = "~/";
                if let Some(home) = dirs::home_dir() {
                    if let Ok(rel) = full_path.strip_prefix(&home) {
                        format!("{}{}", home_prefix, rel.display())
                    } else {
                        full_path.display().to_string()
                    }
                } else {
                    full_path.display().to_string()
                }
            } else if input == "~" {
                if let Some(home) = dirs::home_dir() {
                    if let Ok(rel) = full_path.strip_prefix(&home) {
                        format!("~/{}", rel.display())
                    } else {
                        full_path.display().to_string()
                    }
                } else {
                    full_path.display().to_string()
                }
            } else {
                full_path.display().to_string()
            };

            // Add trailing slash for directories
            if full_path.is_dir() {
                format!("{}/", completed)
            } else {
                completed
            }
        })
        .collect();

    matches.sort();
    matches
}

/// Find the longest common prefix among a set of strings.
fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    if strings.len() == 1 {
        return strings[0].clone();
    }

    let first = &strings[0];
    let mut prefix_len = first.len();

    for s in &strings[1..] {
        prefix_len = first
            .chars()
            .zip(s.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();

        if prefix_len == 0 {
            break;
        }
    }

    first.chars().take(prefix_len).collect()
}
