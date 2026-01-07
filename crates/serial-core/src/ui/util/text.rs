//! Framework-agnostic text editing utilities.
//!
//! This module provides:
//! - `TextBuffer`: A text buffer type that handles all the logic of text editing
//!   (cursor movement, insertion, deletion, word operations) without any UI
//!   framework dependencies. Frontends wrap this with their own scroll/rendering logic.
//! - `slice_by_display_width`: A function to slice strings by display column positions,
//!   handling multi-byte UTF-8 and wide characters correctly.

use unicode_width::UnicodeWidthChar;

/// A text buffer with cursor support.
///
/// Handles UTF-8 text editing operations including:
/// - Character insertion/deletion
/// - Cursor movement (char, word, line boundaries)
/// - Word-based operations (delete word, etc.)
///
/// # Example
///
/// ```
/// use serial_core::ui::text::TextBuffer;
///
/// let mut buf = TextBuffer::default();
/// buf.insert_char('H');
/// buf.insert_char('i');
/// assert_eq!(buf.content(), "Hi");
/// assert_eq!(buf.cursor(), 2);
///
/// buf.move_start();
/// assert_eq!(buf.cursor(), 0);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBuffer {
    content: String,
    /// Cursor position as byte index into content
    cursor: usize,
}

impl TextBuffer {
    /// Create a new empty text buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a text buffer with initial content.
    /// Cursor is placed at the end.
    pub fn with_content(content: impl Into<String>) -> Self {
        let content = content.into();
        let cursor = content.len();
        Self { content, cursor }
    }

    /// Get the current content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the cursor position (byte index).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get the length in bytes.
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Set the content and move cursor to end.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.content.len();
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
    }

    /// Take the content, leaving the buffer empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.content)
    }

    // =========================================================================
    // Insertion
    // =========================================================================

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at the cursor position.
    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    // =========================================================================
    // Deletion
    // =========================================================================

    /// Delete the character before the cursor (Backspace).
    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.prev_char_boundary();
            self.content.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// Delete the character after the cursor (Delete).
    pub fn delete_char_after(&mut self) {
        if self.cursor < self.content.len() {
            let next = self.next_char_boundary();
            self.content.drain(self.cursor..next);
        }
    }

    /// Delete from cursor to start of line (Ctrl+U).
    pub fn delete_to_start(&mut self) {
        if self.cursor > 0 {
            self.content.drain(..self.cursor);
            self.cursor = 0;
        }
    }

    /// Delete from cursor to end of line (Ctrl+K).
    pub fn delete_to_end(&mut self) {
        if self.cursor < self.content.len() {
            self.content.truncate(self.cursor);
        }
    }

    /// Delete the word before the cursor (Ctrl+W).
    pub fn delete_word_before(&mut self) {
        let end = self.cursor;
        self.move_word_left();
        if self.cursor < end {
            self.content.drain(self.cursor..end);
        }
    }

    // =========================================================================
    // Cursor movement
    // =========================================================================

    /// Move cursor one character left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_char_boundary();
        }
    }

    /// Move cursor one character right.
    pub fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            self.cursor = self.next_char_boundary();
        }
    }

    /// Move cursor to start (Home / Ctrl+A).
    pub fn move_start(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end (End / Ctrl+E).
    pub fn move_end(&mut self) {
        self.cursor = self.content.len();
    }

    /// Move cursor one word left (Ctrl+Left / Alt+B).
    pub fn move_word_left(&mut self) {
        // Skip whitespace backwards
        while self.cursor > 0 {
            let prev = self.prev_char_boundary();
            let c = self.content[prev..self.cursor].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor = prev;
        }
        // Skip word characters backwards
        while self.cursor > 0 {
            let prev = self.prev_char_boundary();
            let c = self.content[prev..self.cursor].chars().next().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor = prev;
        }
    }

    /// Move cursor one word right (Ctrl+Right / Alt+F).
    pub fn move_word_right(&mut self) {
        // Skip word characters forwards
        while self.cursor < self.content.len() {
            let c = self.content[self.cursor..].chars().next().unwrap();
            if c.is_whitespace() {
                break;
            }
            self.cursor = self.next_char_boundary();
        }
        // Skip whitespace forwards
        while self.cursor < self.content.len() {
            let c = self.content[self.cursor..].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.cursor = self.next_char_boundary();
        }
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    /// Find the previous character boundary before cursor.
    fn prev_char_boundary(&self) -> usize {
        let mut idx = self.cursor.saturating_sub(1);
        while idx > 0 && !self.content.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    /// Find the next character boundary after cursor.
    fn next_char_boundary(&self) -> usize {
        let mut idx = self.cursor + 1;
        while idx < self.content.len() && !self.content.is_char_boundary(idx) {
            idx += 1;
        }
        idx.min(self.content.len())
    }
}

impl From<String> for TextBuffer {
    fn from(content: String) -> Self {
        Self::with_content(content)
    }
}

impl From<&str> for TextBuffer {
    fn from(content: &str) -> Self {
        Self::with_content(content)
    }
}

/// Slice a string by display width positions, returning byte indices.
///
/// Given a start and end display column, returns the byte range that covers
/// those columns. Handles multi-byte UTF-8 characters and wide characters correctly.
///
/// Returns `(byte_start, byte_end)` where the slice `&s[byte_start..byte_end]`
/// contains the characters that fall within the display range.
///
/// # Example
///
/// ```
/// use serial_core::ui::text::slice_by_display_width;
///
/// let s = "hello世界";
/// // 'h','e','l','l','o' each have width 1, '世','界' each have width 2
/// // Total display width: 5 + 4 = 9
///
/// let (start, end) = slice_by_display_width(s, 0, 5);
/// assert_eq!(&s[start..end], "hello");
///
/// let (start, end) = slice_by_display_width(s, 5, 9);
/// assert_eq!(&s[start..end], "世界");
/// ```
pub fn slice_by_display_width(s: &str, display_start: usize, display_end: usize) -> (usize, usize) {
    let mut current_width = 0;
    let mut byte_start = None;
    let mut byte_end = s.len();

    for (byte_idx, ch) in s.char_indices() {
        let char_width = ch.width().unwrap_or(0);

        // Found the start position
        if byte_start.is_none() && current_width + char_width > display_start {
            byte_start = Some(byte_idx);
        }

        // Found the end position
        if current_width >= display_end {
            byte_end = byte_idx;
            break;
        }

        current_width += char_width;
    }

    (byte_start.unwrap_or(s.len()), byte_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_editing() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        buf.insert_char('c');
        assert_eq!(buf.content(), "abc");
        assert_eq!(buf.cursor(), 3);

        buf.delete_char_before();
        assert_eq!(buf.content(), "ab");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut buf = TextBuffer::with_content("hello world");
        assert_eq!(buf.cursor(), 11);

        buf.move_start();
        assert_eq!(buf.cursor(), 0);

        buf.move_end();
        assert_eq!(buf.cursor(), 11);

        buf.move_left();
        assert_eq!(buf.cursor(), 10);
    }

    #[test]
    fn test_word_movement() {
        let mut buf = TextBuffer::with_content("hello world");
        buf.move_start();
        
        buf.move_word_right();
        assert_eq!(buf.cursor(), 6); // After "hello "
        
        buf.move_word_left();
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn test_delete_word() {
        let mut buf = TextBuffer::with_content("hello world");
        buf.delete_word_before();
        assert_eq!(buf.content(), "hello ");
    }

    #[test]
    fn test_utf8_handling() {
        let mut buf = TextBuffer::new();
        buf.insert_char('日');
        buf.insert_char('本');
        assert_eq!(buf.content(), "日本");
        assert_eq!(buf.cursor(), 6); // Each char is 3 bytes

        buf.move_left();
        assert_eq!(buf.cursor(), 3);
        
        buf.delete_char_before();
        assert_eq!(buf.content(), "本");
    }
}
