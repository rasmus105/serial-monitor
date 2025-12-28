//! Encoding utilities for displaying serial data
//!
//! Provides conversion from raw bytes to various display formats.
//! The core stores raw bytes; encoding is applied on-demand for display.

use strum::{Display, VariantArray};

/// Available display encodings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, VariantArray)]
pub enum Encoding {
    /// Hexadecimal representation (e.g., "DE AD BE EF")
    #[strum(serialize = "HEX")]
    Hex,
    /// UTF-8 text (invalid sequences replaced with U+FFFD)
    #[default]
    #[strum(serialize = "UTF-8")]
    Utf8,
    /// ASCII text (non-printable shown as dots or escape sequences)
    #[strum(serialize = "ASCII")]
    Ascii,
    /// Binary representation (e.g., "11011110 10101101")
    #[strum(serialize = "BIN")]
    Binary,
}

impl Encoding {
    /// Get the next encoding in the cycle
    pub fn cycle_next(self) -> Self {
        match self {
            Encoding::Hex => Encoding::Utf8,
            Encoding::Utf8 => Encoding::Ascii,
            Encoding::Ascii => Encoding::Binary,
            Encoding::Binary => Encoding::Hex,
        }
    }

    /// Get all available encodings
    pub fn all() -> &'static [Encoding] {
        Self::VARIANTS
    }
}

/// Encode raw bytes to a string representation
pub fn encode(data: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Hex => encode_hex(data),
        Encoding::Utf8 => encode_utf8(data),
        Encoding::Ascii => encode_ascii(data),
        Encoding::Binary => encode_binary(data),
    }
}

/// Encode bytes as hexadecimal
///
/// Format: "DE AD BE EF" (uppercase, space-separated)
pub fn encode_hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Encode bytes as UTF-8
///
/// Invalid UTF-8 sequences are replaced with the replacement character (U+FFFD).
/// Control characters are replaced with visible escape sequences to prevent
/// them from affecting terminal rendering.
pub fn encode_utf8(data: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(data);
    // Replace control characters with visible representations
    // to prevent them from corrupting terminal output
    let mut result = String::with_capacity(lossy.len());
    for ch in lossy.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            // Other control characters (0x00-0x1F except the above, and 0x7F)
            c if c.is_control() => {
                result.push_str(&format!("\\x{:02X}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Encode bytes as ASCII
///
/// - Printable ASCII (0x20-0x7E) shown as-is
/// - Common control characters shown as escape sequences (\n, \r, \t)
/// - Other bytes shown as hex escape (\xNN)
pub fn encode_ascii(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len() * 2);

    for &byte in data {
        match byte {
            // Printable ASCII
            0x20..=0x7E => result.push(byte as char),
            // Common control characters
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            b'\0' => result.push_str("\\0"),
            // Everything else as hex escape
            _ => result.push_str(&format!("\\x{:02X}", byte)),
        }
    }

    result
}

/// Encode bytes as binary
///
/// Format: "11011110 10101101" (8-bit groups, space-separated)
pub fn encode_binary(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:08b}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_hex() {
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE, 0xEF]), "DE AD BE EF");
        assert_eq!(encode_hex(&[0x00, 0xFF]), "00 FF");
        assert_eq!(encode_hex(&[]), "");
    }

    #[test]
    fn test_encode_utf8() {
        assert_eq!(encode_utf8(b"Hello"), "Hello");
        assert_eq!(encode_utf8(&[0xC3, 0xA9]), "é"); // UTF-8 é
        // Invalid UTF-8 should use replacement character
        assert_eq!(encode_utf8(&[0xFF, 0xFE]), "\u{FFFD}\u{FFFD}");
        // Control characters should be escaped
        assert_eq!(encode_utf8(b"Line1\nLine2"), "Line1\\nLine2");
        assert_eq!(encode_utf8(b"\r\n\t"), "\\r\\n\\t");
    }

    #[test]
    fn test_encode_ascii() {
        assert_eq!(encode_ascii(b"Hello"), "Hello");
        assert_eq!(encode_ascii(b"Line1\nLine2"), "Line1\\nLine2");
        assert_eq!(encode_ascii(&[0x00, 0x01, 0x7F]), "\\0\\x01\\x7F");
        assert_eq!(encode_ascii(b"\r\n\t"), "\\r\\n\\t");
    }

    #[test]
    fn test_encode_binary() {
        assert_eq!(encode_binary(&[0xFF]), "11111111");
        assert_eq!(encode_binary(&[0x00, 0xFF]), "00000000 11111111");
        assert_eq!(encode_binary(&[0xAA]), "10101010");
    }
}
