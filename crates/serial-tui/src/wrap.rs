//! Text wrapping utilities for the traffic view
//!
//! This module provides pre-computed text wrapping that maps physical screen rows
//! to logical data chunks. This is necessary because ratatui's built-in Wrap
//! doesn't integrate well with manual scrolling - it operates on logical lines
//! while scrolling needs to work on physical rows.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// Configuration for the gutter (line numbers, timestamps, etc.)
#[derive(Debug, Clone)]
pub struct GutterConfig {
    /// Line number to display (1-indexed), or None to hide
    pub line_number: Option<usize>,
    /// Width reserved for line numbers (for alignment)
    pub line_number_width: usize,
    /// Formatted timestamp string, or None to hide
    pub timestamp: Option<String>,
    /// Style for the gutter (typically muted + bold)
    pub style: Style,
}

impl GutterConfig {
    /// Create a gutter with no line numbers or timestamps
    pub fn empty() -> Self {
        Self {
            line_number: None,
            line_number_width: 0,
            timestamp: None,
            style: Style::default(),
        }
    }

    /// Calculate the total width of the gutter
    pub fn width(&self) -> usize {
        let mut w = 0;
        if self.line_number.is_some() {
            w += self.line_number_width + 1; // +1 for separator space
        }
        if let Some(ref ts) = self.timestamp {
            w += UnicodeWidthStr::width(ts.as_str()) + 1; // +1 for separator space
        }
        w
    }

    /// Build the gutter spans for the first row of a chunk
    pub fn build_first_row_spans(&self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        if let Some(line_num) = self.line_number {
            let formatted = format!("{:>width$} ", line_num, width = self.line_number_width);
            spans.push(Span::styled(formatted, self.style));
        }

        if let Some(ref ts) = self.timestamp {
            let formatted = format!("{} ", ts);
            spans.push(Span::styled(formatted, self.style));
        }

        spans
    }

    /// Build the gutter spans for continuation rows (just whitespace for alignment)
    pub fn build_continuation_spans(&self) -> Vec<Span<'static>> {
        let width = self.width();
        if width == 0 {
            return vec![];
        }
        vec![Span::raw(" ".repeat(width))]
    }
}

/// A physical row that will be displayed on screen.
/// Each physical row maps back to a logical chunk index.
#[derive(Debug, Clone)]
pub struct PhysicalRow<'a> {
    /// The ratatui Line to render
    pub line: Line<'a>,
    /// Index of the logical chunk this row belongs to
    pub chunk_index: usize,
    /// Whether this is the first row of the chunk (shows gutter)
    pub is_first_row: bool,
}

/// Wraps a styled line into multiple physical rows that fit within the given width.
///
/// # Arguments
/// * `gutter` - Configuration for line numbers/timestamps gutter
/// * `content` - The main content text
/// * `content_style` - Style for the content
/// * `chunk_index` - The logical chunk index for mapping
/// * `width` - Maximum width in characters
///
/// # Returns
/// A vector of PhysicalRows, one for each screen row needed
pub fn wrap_line<'a>(
    gutter: &GutterConfig,
    content: &'a str,
    content_style: Style,
    chunk_index: usize,
    width: usize,
) -> Vec<PhysicalRow<'a>> {
    if width == 0 {
        return vec![];
    }

    let mut rows = Vec::new();
    let gutter_width = gutter.width();

    // Content width is what remains after the gutter
    let content_width = width.saturating_sub(gutter_width);

    if content_width == 0 {
        // Edge case: width is smaller than gutter
        // Just show truncated gutter
        let mut spans = gutter.build_first_row_spans();
        if spans.is_empty() {
            spans.push(Span::raw(""));
        }
        rows.push(PhysicalRow {
            line: Line::from(spans),
            chunk_index,
            is_first_row: true,
        });
        return rows;
    }

    // Split content into parts that fit the available width
    let content_parts = wrap_text(content, content_width);

    if content_parts.is_empty() {
        // Empty content, just show gutter
        let spans = gutter.build_first_row_spans();
        rows.push(PhysicalRow {
            line: Line::from(spans),
            chunk_index,
            is_first_row: true,
        });
        return rows;
    }

    // First row: gutter + first part of content
    let mut first_spans = gutter.build_first_row_spans();
    first_spans.push(Span::styled(content_parts[0], content_style));
    rows.push(PhysicalRow {
        line: Line::from(first_spans),
        chunk_index,
        is_first_row: true,
    });

    // Subsequent rows: indent (to align with first row's content) + wrapped content
    for part in content_parts.into_iter().skip(1) {
        let mut cont_spans = gutter.build_continuation_spans();
        cont_spans.push(Span::styled(part, content_style));
        rows.push(PhysicalRow {
            line: Line::from(cont_spans),
            chunk_index,
            is_first_row: false,
        });
    }

    rows
}

/// Truncates a styled line to fit within the given width, showing ellipsis if truncated.
///
/// # Arguments
/// * `gutter` - Configuration for line numbers/timestamps gutter
/// * `content` - The main content text
/// * `content_style` - Style for the content
/// * `chunk_index` - The logical chunk index for mapping
/// * `width` - Maximum width in characters
///
/// # Returns
/// A vector containing a single PhysicalRow (truncated lines are always one row)
pub fn truncate_line<'a>(
    gutter: &GutterConfig,
    content: &'a str,
    content_style: Style,
    chunk_index: usize,
    width: usize,
) -> Vec<PhysicalRow<'a>> {
    if width == 0 {
        return vec![];
    }

    let gutter_width = gutter.width();
    let content_width = width.saturating_sub(gutter_width);

    if content_width == 0 {
        // Edge case: width is smaller than gutter
        let mut spans = gutter.build_first_row_spans();
        if spans.is_empty() {
            spans.push(Span::raw(""));
        }
        return vec![PhysicalRow {
            line: Line::from(spans),
            chunk_index,
            is_first_row: true,
        }];
    }

    let mut spans = gutter.build_first_row_spans();

    // Check if content fits
    let content_display_width = UnicodeWidthStr::width(content);

    if content_display_width <= content_width {
        // Content fits, no truncation needed
        spans.push(Span::styled(content, content_style));
    } else {
        // Need to truncate - reserve space for ellipsis
        let ellipsis = "…";
        let ellipsis_width = 1;
        let available_for_content = content_width.saturating_sub(ellipsis_width);

        if available_for_content == 0 {
            // Only room for ellipsis
            spans.push(Span::styled(ellipsis, content_style));
        } else {
            // Truncate content and add ellipsis
            let (truncated, _) = split_at_width(content, available_for_content);
            spans.push(Span::styled(truncated, content_style));
            spans.push(Span::styled(
                ellipsis,
                content_style.add_modifier(Modifier::DIM),
            ));
        }
    }

    vec![PhysicalRow {
        line: Line::from(spans),
        chunk_index,
        is_first_row: true,
    }]
}

/// Wraps text into parts that fit within the specified width.
///
/// # Arguments
/// * `text` - The text to wrap
/// * `width` - Width available for each part
///
/// # Returns
/// A vector of string slices, each fitting within the width
fn wrap_text(text: &str, width: usize) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }

    let mut parts = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if width == 0 {
            break;
        }

        let (part, rest) = split_at_width(remaining, width);
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
        let parts = wrap_text("hello world test", 6);
        assert_eq!(parts, vec!["hello ", "world ", "test"]);
    }

    #[test]
    fn test_wrap_text_empty() {
        let parts = wrap_text("", 10);
        assert!(parts.is_empty());
    }

    #[test]
    fn test_wrap_line_single_row() {
        let gutter = GutterConfig::empty();
        let rows = wrap_line(&gutter, "short", Style::default(), 0, 80);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_first_row);
        assert_eq!(rows[0].chunk_index, 0);
    }

    #[test]
    fn test_wrap_line_multiple_rows() {
        let gutter = GutterConfig::empty();
        let content = "this is a very long line that will need to wrap";
        let rows = wrap_line(&gutter, content, Style::default(), 5, 20);
        assert!(rows.len() > 1);
        assert!(rows[0].is_first_row);
        assert!(!rows[1].is_first_row);
        assert_eq!(rows[0].chunk_index, 5);
        assert_eq!(rows[1].chunk_index, 5);
    }

    #[test]
    fn test_gutter_with_line_numbers() {
        use ratatui::style::{Color, Modifier};

        let gutter = GutterConfig {
            line_number: Some(42),
            line_number_width: 4,
            timestamp: None,
            style: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        };

        assert_eq!(gutter.width(), 5); // "  42 "
        let rows = wrap_line(&gutter, "test content", Style::default(), 0, 80);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_gutter_with_timestamp() {
        use ratatui::style::{Color, Modifier};

        let gutter = GutterConfig {
            line_number: None,
            line_number_width: 0,
            timestamp: Some("+1.234s".to_string()),
            style: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        };

        assert_eq!(gutter.width(), 8); // "+1.234s "
    }

    #[test]
    fn test_gutter_with_both() {
        use ratatui::style::{Color, Modifier};

        let gutter = GutterConfig {
            line_number: Some(1),
            line_number_width: 4,
            timestamp: Some("+0.000s".to_string()),
            style: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        };

        // "   1 " (5) + "+0.000s " (8) = 13
        assert_eq!(gutter.width(), 13);
    }
}
