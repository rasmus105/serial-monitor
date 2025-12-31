//! Encoding utilities for converting raw bytes to display strings
//!
//! Provides multiple encoding modes with configurable formatting options
//! for hexadecimal and binary representations.

use std::fmt::Write;

use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumMessage};

/// Formatting options for hexadecimal display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexFormat {
    /// Bytes per group (0 = no grouping)
    pub group_size: u8,
    /// Separator between groups
    pub separator: char,
    /// Use uppercase hex digits (A-F vs a-f)
    pub uppercase: bool,
}

impl Default for HexFormat {
    fn default() -> Self {
        Self {
            group_size: 1,
            separator: ' ',
            uppercase: true,
        }
    }
}

/// Formatting options for binary display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryFormat {
    /// Bits per group (0 = no grouping, typically 8 for byte grouping)
    pub group_size: u8,
    /// Separator between groups
    pub separator: char,
}

impl Default for BinaryFormat {
    fn default() -> Self {
        Self {
            group_size: 8,
            separator: ' ',
        }
    }
}

/// Encoding mode for displaying raw bytes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Display, EnumMessage, AsRefStr, Serialize, Deserialize)]
pub enum Encoding {
    /// UTF-8 with replacement character for invalid sequences
    #[default]
    #[strum(serialize = "UTF-8", message = "Display as UTF-8 text")]
    Utf8,

    /// ASCII with escape sequences for non-printable characters
    #[strum(
        serialize = "ASCII",
        message = "Display as ASCII (non-printable as escapes)"
    )]
    Ascii,

    /// Hexadecimal with configurable formatting
    #[strum(serialize = "Hex", message = "Display as hexadecimal bytes")]
    Hex(HexFormat),

    /// Binary with configurable formatting
    #[strum(serialize = "Binary", message = "Display as binary")]
    Binary(BinaryFormat),
}

/// Encode raw bytes to a display string using the specified encoding
pub fn encode(data: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => encode_utf8(data),
        Encoding::Ascii => encode_ascii(data),
        Encoding::Hex(format) => encode_hex(data, format),
        Encoding::Binary(format) => encode_binary(data, format),
    }
}

/// Encode bytes as UTF-8, replacing invalid sequences with replacement character.
///
/// Control characters are shown as escape sequences (e.g., `\n`, `\r`, `\t`)
/// to prevent them from affecting terminal rendering.
pub fn encode_utf8(data: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(data);

    let mut result = String::with_capacity(lossy.len());
    for c in lossy.chars() {
        match c {
            // Allow printable ASCII and all non-ASCII Unicode (multi-byte UTF-8)
            ' '..='~' => result.push(c),
            // Common control characters as named escapes
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            // Other C0 control characters (0x01-0x1F except \t, \n, \r)
            '\x01'..='\x08' | '\x0B'..='\x0C' | '\x0E'..='\x1F' => {
                let _ = write!(result, "\\x{:02X}", c as u8);
            }
            // DEL character
            '\x7F' => result.push_str("\\x7F"),
            // All other Unicode characters (emojis, CJK, etc.) - pass through
            _ => result.push(c),
        }
    }
    result
}

/// Encode bytes as ASCII, replacing non-printable characters with escape sequences
///
/// Printable ASCII range: 32-126 (space through tilde)
/// Common control characters use named escapes (`\n`, `\r`, `\t`, `\0`).
/// Everything else (0-31, 127, 128-255) becomes hex escapes like `\xFF`.
pub fn encode_ascii(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }

    // Estimate capacity - most bytes are likely printable (1 char each)
    // Escapes are 2-4 chars, so this is a reasonable starting point
    let mut result = String::with_capacity(data.len());

    for &b in data {
        match b {
            // Printable ASCII
            0x20..=0x7E => result.push(b as char),
            // Common control characters as named escapes
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            b'\0' => result.push_str("\\0"),
            // Everything else as hex escape
            _ => {
                let _ = write!(result, "\\x{:02X}", b);
            }
        }
    }

    result
}

/// Encode bytes as hexadecimal with the specified format
///
/// Examples with default format (group_size=1, separator=' ', uppercase=true):
/// - `[0x48, 0x65, 0x6C]` → `"48 65 6C"`
///
/// With group_size=2:
/// - `[0x48, 0x65, 0x6C, 0x6C]` → `"4865 6C6C"`
///
/// With group_size=0 (no grouping):
/// - `[0x48, 0x65, 0x6C]` → `"48656C"`
pub fn encode_hex(data: &[u8], format: HexFormat) -> String {
    if data.is_empty() {
        return String::new();
    }

    // Calculate exact capacity needed
    let capacity = if format.group_size == 0 {
        // 2 hex chars per byte, no separators
        data.len() * 2
    } else {
        // 2 hex chars per byte + separators between groups
        let group_size = format.group_size as usize;
        let num_groups = data.len().div_ceil(group_size);
        let num_separators = num_groups.saturating_sub(1);
        data.len() * 2 + num_separators
    };

    let mut result = String::with_capacity(capacity);

    if format.group_size == 0 {
        // No grouping - just write all hex pairs
        for &b in data {
            if format.uppercase {
                let _ = write!(result, "{:02X}", b);
            } else {
                let _ = write!(result, "{:02x}", b);
            }
        }
    } else {
        // Group bytes with separators
        let group_size = format.group_size as usize;
        for (i, &b) in data.iter().enumerate() {
            // Add separator before starting a new group (except the first)
            if i > 0 && i % group_size == 0 {
                result.push(format.separator);
            }
            if format.uppercase {
                let _ = write!(result, "{:02X}", b);
            } else {
                let _ = write!(result, "{:02x}", b);
            }
        }
    }

    result
}

/// Encode bytes as binary with the specified format
///
/// Examples with default format (group_size=8, separator=' '):
/// - `[0x48, 0x65]` → `"01001000 01100101"`
///
/// With group_size=4:
/// - `[0x48]` → `"0100 1000"`
///
/// With group_size=0 (no grouping):
/// - `[0x48, 0x65]` → `"0100100001100101"`
pub fn encode_binary(data: &[u8], format: BinaryFormat) -> String {
    if data.is_empty() {
        return String::new();
    }

    let total_bits = data.len() * 8;

    // Calculate exact capacity needed
    let capacity = if format.group_size == 0 {
        total_bits
    } else {
        let group_size = format.group_size as usize;
        let num_groups = total_bits.div_ceil(group_size);
        let num_separators = num_groups.saturating_sub(1);
        total_bits + num_separators
    };

    let mut result = String::with_capacity(capacity);

    if format.group_size == 0 {
        // No grouping - write all bits directly
        for &b in data {
            let _ = write!(result, "{:08b}", b);
        }
    } else {
        // Write bits with separators at group boundaries
        let group_size = format.group_size as usize;
        let mut bit_count = 0;

        for &b in data {
            for bit_pos in (0..8).rev() {
                // Add separator before starting a new group (except the first)
                if bit_count > 0 && bit_count % group_size == 0 {
                    result.push(format.separator);
                }
                result.push(if (b >> bit_pos) & 1 == 1 { '1' } else { '0' });
                bit_count += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_valid() {
        assert_eq!(encode_utf8(b"Hello"), "Hello");
        assert_eq!(encode_utf8("Héllo".as_bytes()), "Héllo");
    }

    #[test]
    fn utf8_invalid_replaced() {
        // Invalid UTF-8 sequence
        assert_eq!(encode_utf8(&[0xFF, 0xFE]), "��");
        // Mixed valid and invalid
        assert_eq!(encode_utf8(&[b'H', 0xFF, b'i']), "H�i");
    }

    #[test]
    fn utf8_control_chars_escaped() {
        // All control chars shown as escape sequences
        assert_eq!(encode_utf8(b"Hello\r\n"), "Hello\\r\\n");
        // Null byte
        assert_eq!(encode_utf8(b"Hello\0World"), "Hello\\0World");
        // Tab
        assert_eq!(encode_utf8(b"Hello\tWorld"), "Hello\\tWorld");
        // Other control chars
        assert_eq!(encode_utf8(&[b'H', 0x01, b'i']), "H\\x01i");
        // DEL character
        assert_eq!(encode_utf8(&[b'H', 0x7F, b'i']), "H\\x7Fi");
    }

    #[test]
    fn utf8_unicode_preserved() {
        // Emojis and other Unicode should pass through
        assert_eq!(encode_utf8("Hello 🌍".as_bytes()), "Hello 🌍");
        assert_eq!(encode_utf8("日本語".as_bytes()), "日本語");
    }

    #[test]
    fn ascii_printable() {
        assert_eq!(encode_ascii(b"Hello World!"), "Hello World!");
        // Boundary: space (32) and tilde (126)
        assert_eq!(encode_ascii(&[32, 126]), " ~");
    }

    #[test]
    fn ascii_control_chars_escaped() {
        // Control characters - shown as escape sequences
        assert_eq!(encode_ascii(&[0, 31, 127]), "\\0\\x1F\\x7F");
        // High bytes - shown as hex escapes
        assert_eq!(encode_ascii(&[128, 255]), "\\x80\\xFF");
        // Mixed
        assert_eq!(encode_ascii(&[b'H', 0x00, b'i', 0xFF]), "H\\0i\\xFF");
        // Common escapes
        assert_eq!(encode_ascii(&[b'\n', b'\r', b'\t']), "\\n\\r\\t");
    }

    #[test]
    fn hex_default_format() {
        let format = HexFormat::default();
        assert_eq!(encode_hex(b"Hi", format), "48 69");
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE, 0xEF], format), "DE AD BE EF");
    }

    #[test]
    fn hex_lowercase() {
        let format = HexFormat {
            uppercase: false,
            ..Default::default()
        };
        assert_eq!(encode_hex(&[0xDE, 0xAD], format), "de ad");
    }

    #[test]
    fn hex_grouped() {
        let format = HexFormat {
            group_size: 2,
            ..Default::default()
        };
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE, 0xEF], format), "DEAD BEEF");

        // Odd number of bytes
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE], format), "DEAD BE");
    }

    #[test]
    fn hex_no_grouping() {
        let format = HexFormat {
            group_size: 0,
            ..Default::default()
        };
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE, 0xEF], format), "DEADBEEF");
    }

    #[test]
    fn hex_custom_separator() {
        let format = HexFormat {
            separator: ':',
            ..Default::default()
        };
        assert_eq!(encode_hex(&[0xDE, 0xAD, 0xBE], format), "DE:AD:BE");
    }

    #[test]
    fn hex_empty() {
        assert_eq!(encode_hex(&[], HexFormat::default()), "");
    }

    #[test]
    fn binary_default_format() {
        let format = BinaryFormat::default();
        assert_eq!(encode_binary(&[0x48], format), "01001000");
        assert_eq!(encode_binary(&[0x48, 0x65], format), "01001000 01100101");
    }

    #[test]
    fn binary_grouped_4bits() {
        let format = BinaryFormat {
            group_size: 4,
            ..Default::default()
        };
        assert_eq!(encode_binary(&[0x48], format), "0100 1000");
        assert_eq!(encode_binary(&[0xF0], format), "1111 0000");
    }

    #[test]
    fn binary_no_grouping() {
        let format = BinaryFormat {
            group_size: 0,
            ..Default::default()
        };
        assert_eq!(encode_binary(&[0x48, 0x65], format), "0100100001100101");
    }

    #[test]
    fn binary_custom_separator() {
        let format = BinaryFormat {
            separator: '_',
            ..Default::default()
        };
        assert_eq!(encode_binary(&[0xFF, 0x00], format), "11111111_00000000");
    }

    #[test]
    fn binary_empty() {
        assert_eq!(encode_binary(&[], BinaryFormat::default()), "");
    }

    #[test]
    fn encode_dispatcher() {
        // Verify the main encode() function dispatches correctly
        assert_eq!(encode(b"Hi", Encoding::Utf8), "Hi");
        assert_eq!(encode(b"Hi", Encoding::Ascii), "Hi");
        assert_eq!(encode(b"Hi", Encoding::Hex(HexFormat::default())), "48 69");
        assert_eq!(
            encode(&[0x48], Encoding::Binary(BinaryFormat::default())),
            "01001000"
        );
    }
}
