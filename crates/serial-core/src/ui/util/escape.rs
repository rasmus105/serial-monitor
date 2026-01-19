//! C-style escape sequence parsing for user input.
//!
//! Converts strings with escape sequences (like `\r\n`, `\t`, `\xHH`) into raw bytes.
//! This is the inverse operation of the encoding module which converts bytes to display strings.

/// Parse C-style escape sequences in a string and return the resulting bytes.
///
/// Supports the following escape sequences:
/// - `\\` → backslash (0x5C)
/// - `\r` → carriage return (0x0D)
/// - `\n` → line feed (0x0A)
/// - `\t` → tab (0x09)
/// - `\0` → null (0x00)
/// - `\xHH` → arbitrary byte in hex (e.g., `\x1B` for ESC)
///
/// Invalid escapes (like `\q`) are treated as literal characters (backslash + 'q').
/// Incomplete hex escapes (like `\x` or `\x1`) are also treated literally.
///
/// # Examples
///
/// ```
/// use serial_core::ui::util::escape::parse_escape_sequences;
///
/// // Common escapes
/// assert_eq!(parse_escape_sequences(r"Hello\r\n"), b"Hello\r\n");
/// assert_eq!(parse_escape_sequences(r"Tab:\tEnd"), b"Tab:\tEnd");
///
/// // Hex escapes
/// assert_eq!(parse_escape_sequences(r"\x00\xFF"), vec![0x00, 0xFF]);
///
/// // Escaped backslash
/// assert_eq!(parse_escape_sequences(r"C:\\path"), b"C:\\path");
///
/// // Invalid escapes are literal
/// assert_eq!(parse_escape_sequences(r"\q"), b"\\q");
/// ```
pub fn parse_escape_sequences(input: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('\\') => {
                    chars.next();
                    result.push(b'\\');
                }
                Some('r') => {
                    chars.next();
                    result.push(b'\r');
                }
                Some('n') => {
                    chars.next();
                    result.push(b'\n');
                }
                Some('t') => {
                    chars.next();
                    result.push(b'\t');
                }
                Some('0') => {
                    chars.next();
                    result.push(b'\0');
                }
                Some('x') => {
                    chars.next(); // consume 'x'

                    // Try to read two hex digits
                    let hex1 = chars.peek().and_then(|c| c.to_digit(16));
                    if let Some(h1) = hex1 {
                        chars.next();
                        let hex2 = chars.peek().and_then(|c| c.to_digit(16));
                        if let Some(h2) = hex2 {
                            chars.next();
                            result.push((h1 * 16 + h2) as u8);
                        } else {
                            // Only one hex digit - treat as literal \xH
                            result.extend_from_slice(b"\\x");
                            // Re-encode the hex digit we consumed
                            result.push(char::from_digit(h1, 16).unwrap() as u8);
                        }
                    } else {
                        // No hex digits after \x - treat as literal
                        result.extend_from_slice(b"\\x");
                    }
                }
                Some(&next_char) => {
                    // Unknown escape - treat as literal backslash + char
                    result.push(b'\\');
                    // Don't consume the next char, let it be processed normally
                    // Actually we need to handle multi-byte chars properly
                    for b in next_char.to_string().as_bytes() {
                        result.push(*b);
                    }
                    chars.next();
                }
                None => {
                    // Trailing backslash - treat as literal
                    result.push(b'\\');
                }
            }
        } else {
            // Regular character - encode as UTF-8
            for b in c.to_string().as_bytes() {
                result.push(*b);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_escapes() {
        assert_eq!(parse_escape_sequences("Hello World"), b"Hello World");
        assert_eq!(parse_escape_sequences(""), b"");
        assert_eq!(parse_escape_sequences("AT+COMMAND"), b"AT+COMMAND");
    }

    #[test]
    fn test_common_escapes() {
        assert_eq!(parse_escape_sequences(r"\r"), b"\r");
        assert_eq!(parse_escape_sequences(r"\n"), b"\n");
        assert_eq!(parse_escape_sequences(r"\t"), b"\t");
        assert_eq!(parse_escape_sequences(r"\0"), b"\0");
        assert_eq!(parse_escape_sequences(r"\\"), b"\\");
    }

    #[test]
    fn test_crlf() {
        assert_eq!(parse_escape_sequences(r"\r\n"), b"\r\n");
        assert_eq!(parse_escape_sequences(r"AT+CMD\r\n"), b"AT+CMD\r\n");
    }

    #[test]
    fn test_hex_escapes() {
        assert_eq!(parse_escape_sequences(r"\x00"), vec![0x00]);
        assert_eq!(parse_escape_sequences(r"\xFF"), vec![0xFF]);
        assert_eq!(parse_escape_sequences(r"\xff"), vec![0xFF]);
        assert_eq!(parse_escape_sequences(r"\x1B"), vec![0x1B]); // ESC
        assert_eq!(parse_escape_sequences(r"\xDE\xAD"), vec![0xDE, 0xAD]);
    }

    #[test]
    fn test_mixed() {
        assert_eq!(parse_escape_sequences(r"Hello\r\nWorld"), b"Hello\r\nWorld");
        assert_eq!(
            parse_escape_sequences(r"\x48\x69"), // "Hi" in hex
            b"Hi"
        );
        assert_eq!(
            parse_escape_sequences(r"Tab:\tNewline:\n"),
            b"Tab:\tNewline:\n"
        );
    }

    #[test]
    fn test_invalid_escapes_literal() {
        // Unknown escape sequences are treated literally
        assert_eq!(parse_escape_sequences(r"\q"), b"\\q");
        assert_eq!(parse_escape_sequences(r"\a"), b"\\a");
        assert_eq!(parse_escape_sequences(r"Hello\qWorld"), b"Hello\\qWorld");
    }

    #[test]
    fn test_incomplete_hex_literal() {
        // \x with no hex digits
        assert_eq!(parse_escape_sequences(r"\x"), b"\\x");
        assert_eq!(parse_escape_sequences(r"\xG"), b"\\xG");
        // \x with only one hex digit
        assert_eq!(parse_escape_sequences(r"\x1"), b"\\x1");
        assert_eq!(parse_escape_sequences(r"\x1G"), b"\\x1G");
    }

    #[test]
    fn test_trailing_backslash() {
        assert_eq!(parse_escape_sequences(r"Hello\"), b"Hello\\");
    }

    #[test]
    fn test_escaped_backslash_before_escape() {
        // Double backslash should produce single backslash, not escape the next char
        assert_eq!(parse_escape_sequences(r"\\n"), b"\\n");
        assert_eq!(parse_escape_sequences(r"\\r\\n"), b"\\r\\n");
    }

    #[test]
    fn test_unicode_passthrough() {
        // Unicode characters should pass through unchanged (as UTF-8 bytes)
        assert_eq!(parse_escape_sequences("Hello 🌍"), "Hello 🌍".as_bytes());
        assert_eq!(parse_escape_sequences("日本語"), "日本語".as_bytes());
    }

    #[test]
    fn test_realistic_at_commands() {
        assert_eq!(parse_escape_sequences(r"AT\r\n"), b"AT\r\n");
        assert_eq!(parse_escape_sequences(r"AT+CGMI\r\n"), b"AT+CGMI\r\n");
        // Test with quotes in the command (using regular string with escapes)
        assert_eq!(
            parse_escape_sequences("AT+CMGS=\"+1234567890\"\\r\\n"),
            b"AT+CMGS=\"+1234567890\"\r\n"
        );
    }
}
