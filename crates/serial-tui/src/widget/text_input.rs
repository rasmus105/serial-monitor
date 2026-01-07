//! Text input widget with cursor support.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Paragraph, Widget},
};
use serial_core::ui::TextBuffer;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

/// A text input field.
#[derive(Debug, Default, Clone)]
pub struct TextInputState {
    /// Text buffer handling content and cursor
    buffer: TextBuffer,
    /// Horizontal scroll offset (display width units)
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
        self.buffer.set_content(content);
        self
    }

    /// Get the current content.
    pub fn content(&self) -> &str {
        self.buffer.content()
    }

    /// Handle a key event. Returns true if the event was handled.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Handle Ctrl+<key> sequences
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let handled = match key.code {
                KeyCode::Char('a') => {
                    self.buffer.move_start();
                    true
                }
                KeyCode::Char('e') => {
                    self.buffer.move_end();
                    true
                }
                KeyCode::Char('b') => {
                    self.buffer.move_left();
                    true
                }
                KeyCode::Char('f') => {
                    self.buffer.move_right();
                    true
                }
                KeyCode::Char('h') => {
                    self.buffer.delete_char_before();
                    true
                }
                KeyCode::Char('w') => {
                    self.buffer.delete_word_before();
                    true
                }
                KeyCode::Char('u') => {
                    self.buffer.delete_to_start();
                    true
                }
                KeyCode::Char('k') => {
                    self.buffer.delete_to_end();
                    true
                }
                KeyCode::Left => {
                    self.buffer.move_word_left();
                    true
                }
                KeyCode::Right => {
                    self.buffer.move_word_right();
                    true
                }
                _ => false,
            };
            if handled {
                self.clear_completion();
            }
            return handled;
        }

        // Handle Alt+<key> sequences
        if key.modifiers.contains(KeyModifiers::ALT) {
            let handled = match key.code {
                KeyCode::Char('b') => {
                    self.buffer.move_word_left();
                    true
                }
                KeyCode::Char('f') => {
                    self.buffer.move_word_right();
                    true
                }
                _ => false,
            };
            if handled {
                self.clear_completion();
            }
            return handled;
        }

        let handled = match key.code {
            KeyCode::Char(c) => {
                self.buffer.insert_char(c);
                true
            }
            KeyCode::Backspace => {
                self.buffer.delete_char_before();
                true
            }
            KeyCode::Delete => {
                self.buffer.delete_char_after();
                true
            }
            KeyCode::Left => {
                self.buffer.move_left();
                true
            }
            KeyCode::Right => {
                self.buffer.move_right();
                true
            }
            KeyCode::Home => {
                self.buffer.move_start();
                true
            }
            KeyCode::End => {
                self.buffer.move_end();
                true
            }
            _ => false,
        };
        if handled {
            self.clear_completion();
        }
        handled
    }

    /// Clear the input.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.scroll = 0;
        self.completion = None;
    }

    /// Set the content and move cursor to end.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.buffer.set_content(content);
        self.scroll = 0;
        self.completion = None;
    }

    /// Take the content and clear the input.
    pub fn take(&mut self) -> String {
        self.scroll = 0;
        self.completion = None;
        self.buffer.take()
    }

    /// Get the cursor position (display width from start).
    pub fn cursor_display_pos(&self) -> usize {
        self.buffer.content()[..self.buffer.cursor()].width()
    }

    /// Update scroll to ensure cursor is visible.
    fn update_scroll(&mut self, width: usize) {
        let cursor_pos = self.cursor_display_pos();
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
                self.buffer.set_content(state.matches[state.index].clone());
                return true;
            }
            return false;
        }

        // Start new completion
        let matches = find_path_completions(self.buffer.content());
        if matches.is_empty() {
            return false;
        }

        if matches.len() == 1 {
            // Single match - complete it directly
            let completed = &matches[0];
            self.buffer.set_content(completed.clone());
            // Store state so subsequent tabs can cycle (if it's a directory)
            self.completion = Some(PathCompletionState {
                matches,
                index: 0,
            });
            return true;
        }

        // Multiple matches - complete to common prefix first
        let common = longest_common_prefix(&matches);
        if common.len() > self.buffer.len() {
            // We can extend the input
            self.buffer.set_content(common);
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
                self.buffer.set_content(state.matches[0].clone());
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

        let content = self.state.buffer.content();
        let display_text = if content.is_empty() {
            &self.state.placeholder
        } else {
            content
        };

        let style = if content.is_empty() {
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
        if self.focused && !content.is_empty() {
            let cursor_x = self.state.cursor_display_pos() - self.state.scroll;
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
