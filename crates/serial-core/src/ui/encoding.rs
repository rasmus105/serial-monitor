//! Display utilities for encoding types
//!
//! Since `Encoding` has variants with inner data (`Hex(HexFormat)`, `Binary(BinaryFormat)`),
//! strum's `VariantArray` can't be derived directly. This module provides const arrays
//! with default-formatted variants for use in UI dropdowns.

use crate::{BinaryFormat, Encoding, HexFormat};

/// All `Encoding` variants with default formats, suitable for dropdowns
pub const ENCODING_VARIANTS: &[Encoding] = &[
    Encoding::Utf8,
    Encoding::Ascii,
    Encoding::Hex(HexFormat {
        group_size: 1,
        separator: ' ',
        uppercase: true,
    }),
    Encoding::Binary(BinaryFormat {
        group_size: 8,
        separator: ' ',
    }),
];

/// Display names for encoding variants (matches ENCODING_VARIANTS order)
pub const ENCODING_DISPLAY_NAMES: &[&str] = &["UTF-8", "ASCII", "Hex", "Binary"];

/// Get the index of an encoding in ENCODING_VARIANTS (by discriminant, ignoring format)
pub fn encoding_index(encoding: Encoding) -> usize {
    match encoding {
        Encoding::Utf8 => 0,
        Encoding::Ascii => 1,
        Encoding::Hex(_) => 2,
        Encoding::Binary(_) => 3,
    }
}

/// Get display name for an encoding
pub fn encoding_display(encoding: Encoding) -> &'static str {
    ENCODING_DISPLAY_NAMES[encoding_index(encoding)]
}
