//! Text wrapping utilities for the traffic view
//!
//! This module provides pre-computed text wrapping that maps physical screen rows
//! to logical data chunks. This is necessary because ratatui's built-in Wrap
//! doesn't integrate well with manual scrolling - it operates on logical lines
//! while scrolling needs to work on physical rows.

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// A physical row that will be displayed on screen.
/// Each physical row maps back to a logical chunk index.
#[derive(Debug, Clone)]
pub struct PhysicalRow<'a> {
    /// The ratatui Line to render
    pub line: Line<'a>,
    /// Index of the logical chunk this row belongs to
    pub chunk_index: usize,
    /// Whether this is the first row of the chunk (shows prefix)
    pub is_first_row: bool,
}

/// Wraps a styled line into multiple physical rows that fit within the given width.
///
/// # Arguments
/// * `prefix` - The prefix text (e.g., "RX: " or "TX: ")
/// * `prefix_style` - Style for the prefix
/// * `content` - The main content text
/// * `content_style` - Style for the content
/// * `chunk_index` - The logical chunk index for mapping
/// * `width` - Maximum width in characters
///
/// # Returns
/// A vector of PhysicalRows, one for each screen row needed
pub fn wrap_line<'a>(
    prefix: &'a str,
    prefix_style: Style,
    content: &'a str,
    content_style: Style,
    chunk_index: usize,
    width: usize,
) -> Vec<PhysicalRow<'a>> {
    if width == 0 {
        return vec![];
    }

    let mut rows = Vec::new();
    let prefix_width = UnicodeWidthStr::width(prefix);

    // First row includes the prefix
    let first_row_content_width = width.saturating_sub(prefix_width);
    // Subsequent rows are indented by prefix_width, so they also have reduced width
    let subsequent_row_content_width = width.saturating_sub(prefix_width);

    if first_row_content_width == 0 {
        // Edge case: width is smaller than prefix
        // Just show truncated prefix
        rows.push(PhysicalRow {
            line: Line::from(Span::styled(prefix, prefix_style)),
            chunk_index,
            is_first_row: true,
        });
        return rows;
    }

    // Split content into chunks that fit the available width
    let content_parts = wrap_text(content, first_row_content_width, subsequent_row_content_width);

    if content_parts.is_empty() {
        // Empty content, just show prefix
        rows.push(PhysicalRow {
            line: Line::from(Span::styled(prefix, prefix_style)),
            chunk_index,
            is_first_row: true,
        });
        return rows;
    }

    // First row: prefix + first part of content
    rows.push(PhysicalRow {
        line: Line::from(vec![
            Span::styled(prefix, prefix_style),
            Span::styled(content_parts[0], content_style),
        ]),
        chunk_index,
        is_first_row: true,
    });

    // Subsequent rows: just content (indented to align with first row's content)
    let indent = " ".repeat(prefix_width);
    for part in content_parts.into_iter().skip(1) {
        rows.push(PhysicalRow {
            line: Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled(part, content_style),
            ]),
            chunk_index,
            is_first_row: false,
        });
    }

    rows
}

/// Wraps text into parts that fit within specified widths.
///
/// # Arguments
/// * `text` - The text to wrap
/// * `first_width` - Width available for the first part
/// * `subsequent_width` - Width available for subsequent parts
///
/// # Returns
/// A vector of string slices, each fitting within its respective width
fn wrap_text(text: &str, first_width: usize, subsequent_width: usize) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }

    let mut parts = Vec::new();
    let mut remaining = text;
    let mut is_first = true;

    while !remaining.is_empty() {
        let available_width = if is_first { first_width } else { subsequent_width };
        is_first = false;

        if available_width == 0 {
            break;
        }

        let (part, rest) = split_at_width(remaining, available_width);
        if part.is_empty() {
            // Can't make progress, take at least one char to avoid infinite loop
            let first_char_end = remaining
                .char_indices()
                .nth(1)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());
            parts.push(&remaining[..first_char_end]);
            remaining = &remaining[first_char_end..];
        } else {
            parts.push(part);
            remaining = rest;
        }
    }

    parts
}

/// Splits text at approximately the given display width.
///
/// Returns (part_that_fits, remainder).
/// Uses unicode width to handle multi-byte and wide characters correctly.
fn split_at_width(text: &str, max_width: usize) -> (&str, &str) {
    let mut current_width = 0;
    let mut last_valid_boundary = 0;

    for (byte_idx, ch) in text.char_indices() {
        let char_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);

        if current_width + char_width > max_width {
            // This character would exceed the width
            return (&text[..last_valid_boundary], &text[last_valid_boundary..]);
        }

        current_width += char_width;
        last_valid_boundary = byte_idx + ch.len_utf8();
    }

    // Entire string fits
    (text, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_at_width_simple() {
        let (part, rest) = split_at_width("hello world", 5);
        assert_eq!(part, "hello");
        assert_eq!(rest, " world");
    }

    #[test]
    fn test_split_at_width_fits() {
        let (part, rest) = split_at_width("hi", 10);
        assert_eq!(part, "hi");
        assert_eq!(rest, "");
    }

    #[test]
    fn test_split_at_width_unicode() {
        // Japanese characters are typically 2 cells wide
        let (part, rest) = split_at_width("日本語", 4);
        assert_eq!(part, "日本");
        assert_eq!(rest, "語");
    }

    #[test]
    fn test_wrap_text_basic() {
        let parts = wrap_text("hello world test", 5, 6);
        assert_eq!(parts, vec!["hello", " world", " test"]);
    }

    #[test]
    fn test_wrap_text_empty() {
        let parts = wrap_text("", 10, 10);
        assert!(parts.is_empty());
    }

    #[test]
    fn test_wrap_line_single_row() {
        let rows = wrap_line("RX: ", Style::default(), "short", Style::default(), 0, 80);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_first_row);
        assert_eq!(rows[0].chunk_index, 0);
    }

    #[test]
    fn test_wrap_line_multiple_rows() {
        // "RX: " is 4 chars, content needs to wrap
        let content = "this is a very long line that will need to wrap";
        let rows = wrap_line("RX: ", Style::default(), content, Style::default(), 5, 20);
        assert!(rows.len() > 1);
        assert!(rows[0].is_first_row);
        assert!(!rows[1].is_first_row);
        assert_eq!(rows[0].chunk_index, 5);
        assert_eq!(rows[1].chunk_index, 5);
    }
}
