//! Unit conversion utilities for UI inputs
//!
//! These types help users specify sizes and durations in human-friendly units
//! (e.g., "10 KiB" or "500 ms") which are then converted to raw values.

use std::time::Duration;

use strum::{Display, EnumIter, IntoStaticStr, VariantArray};

/// Size unit for configuring buffer sizes, chunk sizes, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, VariantArray, IntoStaticStr)]
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

    /// Display name for this unit
    pub fn display_name(self) -> &'static str {
        self.into()
    }

    /// Create from index into VARIANTS
    pub fn from_index(index: usize) -> Self {
        Self::VARIANTS.get(index).copied().unwrap_or_default()
    }
}

/// Time unit for configuring delays, timeouts, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, VariantArray, IntoStaticStr)]
pub enum TimeUnit {
    /// Milliseconds
    #[default]
    #[strum(to_string = "ms")]
    Milliseconds,

    /// Seconds
    #[strum(to_string = "s")]
    Seconds,

    /// Minutes
    #[strum(to_string = "min")]
    Minutes,

    /// Hours
    #[strum(to_string = "h")]
    Hours,
}

impl TimeUnit {
    /// Convert a value in this unit to Duration
    pub fn to_duration(self, value: u64) -> Duration {
        match self {
            TimeUnit::Milliseconds => Duration::from_millis(value),
            TimeUnit::Seconds => Duration::from_secs(value),
            TimeUnit::Minutes => Duration::from_secs(value.saturating_mul(60)),
            TimeUnit::Hours => Duration::from_secs(value.saturating_mul(3600)),
        }
    }

    /// Display name for this unit
    pub fn display_name(self) -> &'static str {
        self.into()
    }

    /// Create from index into VARIANTS
    pub fn from_index(index: usize) -> Self {
        Self::VARIANTS.get(index).copied().unwrap_or_default()
    }
}
