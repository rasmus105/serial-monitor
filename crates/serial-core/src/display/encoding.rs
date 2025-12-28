//! Encoding utilities for converting raw bytes to display strings
//!
//! Provides multiple encoding modes with configurable formatting options
//! for hexadecimal and binary representations.

use strum::{AsRefStr, Display, EnumMessage, VariantArray};

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
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Display, EnumMessage, AsRefStr, VariantArray,
)]
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

impl Encoding {
    /// Create UTF-8 encoding
    pub fn utf8() -> Self {
        Self::Utf8
    }

    /// Create ASCII encoding
    pub fn ascii() -> Self {
        Self::Ascii
    }

    /// Create Hex encoding with default format
    pub fn hex() -> Self {
        Self::Hex(HexFormat::default())
    }

    /// Create Hex encoding with custom format
    pub fn hex_with_format(format: HexFormat) -> Self {
        Self::Hex(format)
    }

    /// Create Binary encoding with default format
    pub fn binary() -> Self {
        Self::Binary(BinaryFormat::default())
    }

    /// Create Binary encoding with custom format
    pub fn binary_with_format(format: BinaryFormat) -> Self {
        Self::Binary(format)
    }

    /// Get display name for this encoding
    pub fn name(&self) -> &str {
        self.as_ref()
    }

    /// Get description for this encoding
    pub fn description(&self) -> &'static str {
        self.get_message().expect("All variants have messages")
    }

    /// Get all base encoding variants (without format variations)
    pub fn all_variants() -> &'static [Encoding] {
        Self::VARIANTS
    }
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
pub fn encode_ascii(data: &[u8]) -> String {
    todo!("Implement ASCII encoding")
}

/// Encode bytes as hexadecimal with the specified format
pub fn encode_hex(data: &[u8], format: HexFormat) -> String {
    todo!("Implement hex encoding with format")
}

/// Encode bytes as binary with the specified format
pub fn encode_binary(data: &[u8], format: BinaryFormat) -> String {
    todo!("Implement binary encoding with format")
}

#[cfg(test)]
mod tests {
    // TODO: Add encoding tests once implementation is complete
}
