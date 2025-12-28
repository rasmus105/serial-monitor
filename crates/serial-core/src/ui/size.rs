//! Size unit utilities for buffer and length configuration

use strum::{Display, EnumIter, VariantArray};

/// Size unit for configuring buffer sizes, line lengths, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, VariantArray)]
pub enum SizeUnit {
    /// Bytes
    #[strum(to_string = "B")]
    Bytes,

    /// Kibibytes (1024 bytes)
    #[default]
    #[strum(to_string = "KiB")]
    KiB,

    /// Mebibytes (1024² bytes)
    #[strum(to_string = "MiB")]
    MiB,
}

impl SizeUnit {
    /// Convert a value in this unit to bytes
    pub fn to_bytes(self, value: usize) -> usize {
        match self {
            SizeUnit::Bytes => value,
            SizeUnit::KiB => value.saturating_mul(1024),
            SizeUnit::MiB => value.saturating_mul(1024 * 1024),
        }
    }

    /// Convert bytes to a value in this unit
    pub fn from_bytes(self, bytes: usize) -> f64 {
        match self {
            SizeUnit::Bytes => bytes as f64,
            SizeUnit::KiB => bytes as f64 / 1024.0,
            SizeUnit::MiB => bytes as f64 / (1024.0 * 1024.0),
        }
    }
}
