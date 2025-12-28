//! Encoding utilities for converting raw bytes to display strings
//!
//! Provides multiple encoding modes with configurable formatting options
//! for hexadecimal and binary representations.

use strum::{AsRefStr, Display, EnumMessage};

/// Formatting options for hexadecimal display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Display, EnumMessage, AsRefStr)]
pub enum Encoding {
    /// UTF-8 with replacement character for invalid sequences
    #[default]
    #[strum(serialize = "UTF-8", message = "Display as UTF-8 text")]
    Utf8,

    /// ASCII with dots for non-printable characters
    #[strum(
        serialize = "ASCII",
        message = "Display as ASCII (non-printable as dots)"
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

/// Encode bytes as UTF-8, replacing invalid sequences with replacement character
pub fn encode_utf8(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

/// Encode bytes as ASCII, replacing non-printable characters with dots
///
/// Printable ASCII range: 32-126 (space through tilde)
/// Everything else (0-31, 127, 128-255) becomes '.'
pub fn encode_ascii(data: &[u8]) -> String {
    let mut result = String::new();
    for b in data {
        match b {
            // Printable ASCII
            0x20..=0x7E => result.push(*b as char),
            // Common control characters
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            b'\0' => result.push_str("\\0"),
            // Everything else as hex escape
            _ => result.push_str(&format!("\\x{:02X}", b)),
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

    let hex_chars: Vec<String> = data
        .iter()
        .map(|&b| {
            if format.uppercase {
                format!("{:02X}", b)
            } else {
                format!("{:02x}", b)
            }
        })
        .collect();

    if format.group_size == 0 {
        // No grouping - concatenate all hex pairs
        hex_chars.join("")
    } else {
        // Group bytes and join with separator
        hex_chars
            .chunks(format.group_size as usize)
            .map(|chunk| chunk.join(""))
            .collect::<Vec<_>>()
            .join(&format.separator.to_string())
    }
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

    // Convert all bytes to binary string (8 bits each)
    let all_bits: String = data.iter().map(|&b| format!("{:08b}", b)).collect();

    if format.group_size == 0 {
        // No grouping
        all_bits
    } else {
        // Group bits and join with separator
        all_bits
            .as_bytes()
            .chunks(format.group_size as usize)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join(&format.separator.to_string())
    }
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
    fn ascii_printable() {
        assert_eq!(encode_ascii(b"Hello World!"), "Hello World!");
        // Boundary: space (32) and tilde (126)
        assert_eq!(encode_ascii(&[32, 126]), " ~");
    }

    #[test]
    fn ascii_non_printable_replaced() {
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
