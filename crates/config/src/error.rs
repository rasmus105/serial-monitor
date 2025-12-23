//! Error types for the configuration system.

use thiserror::Error;

/// Errors that can occur when working with configuration values.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ConfigError {
    /// A value has the wrong type for its field.
    #[error("Field '{field}': expected {expected}, got {got}")]
    TypeMismatch {
        /// Field name
        field: String,
        /// Expected type name
        expected: &'static str,
        /// Actual type name
        got: &'static str,
    },

    /// A numeric value is outside the allowed range.
    #[error("Field '{field}': value {value} out of range [{min}, {max}]")]
    OutOfRange {
        /// Field name
        field: String,
        /// The invalid value (as string for display)
        value: String,
        /// Minimum allowed (as string, or "unbounded")
        min: String,
        /// Maximum allowed (as string, or "unbounded")
        max: String,
    },

    /// An enum variant index is invalid.
    #[error("Field '{field}': invalid variant index {index} (max: {max})")]
    InvalidVariant {
        /// Field name
        field: String,
        /// The invalid index
        index: usize,
        /// Maximum valid index
        max: usize,
    },

    /// A list has an invalid length.
    #[error("Field '{field}': list length {len} out of range [{min_display}, {max_display}]")]
    ListLengthOutOfRange {
        /// Field name
        field: String,
        /// Actual length
        len: usize,
        /// Minimum allowed length
        min: Option<usize>,
        /// Maximum allowed length
        max: Option<usize>,
        /// Min as string for display
        min_display: String,
        /// Max as string for display
        max_display: String,
    },

    /// A custom validation error.
    #[error("Field '{field}': {message}")]
    ValidationFailed {
        /// Field name
        field: String,
        /// Error message
        message: String,
    },

    /// Wrong number of values provided.
    #[error("Expected {expected} values, got {got}")]
    WrongValueCount {
        /// Expected number of values
        expected: usize,
        /// Actual number of values
        got: usize,
    },
}

impl ConfigError {
    /// Create a type mismatch error.
    pub fn type_mismatch(field: impl Into<String>, expected: &'static str, got: &'static str) -> Self {
        Self::TypeMismatch {
            field: field.into(),
            expected,
            got,
        }
    }

    /// Create an out of range error for integers.
    pub fn int_out_of_range(field: impl Into<String>, value: i64, min: Option<i64>, max: Option<i64>) -> Self {
        Self::OutOfRange {
            field: field.into(),
            value: value.to_string(),
            min: min.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
            max: max.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
        }
    }

    /// Create an out of range error for unsigned integers.
    pub fn uint_out_of_range(field: impl Into<String>, value: u64, min: Option<u64>, max: Option<u64>) -> Self {
        Self::OutOfRange {
            field: field.into(),
            value: value.to_string(),
            min: min.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
            max: max.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
        }
    }

    /// Create an out of range error for floats.
    pub fn float_out_of_range(field: impl Into<String>, value: f64, min: Option<f64>, max: Option<f64>) -> Self {
        Self::OutOfRange {
            field: field.into(),
            value: value.to_string(),
            min: min.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
            max: max.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
        }
    }

    /// Create an invalid variant error.
    pub fn invalid_variant(field: impl Into<String>, index: usize, max: usize) -> Self {
        Self::InvalidVariant {
            field: field.into(),
            index,
            max,
        }
    }

    /// Create a list length out of range error.
    pub fn list_length_out_of_range(
        field: impl Into<String>,
        len: usize,
        min: Option<usize>,
        max: Option<usize>,
    ) -> Self {
        Self::ListLengthOutOfRange {
            field: field.into(),
            len,
            min,
            max,
            min_display: min.map(|v| v.to_string()).unwrap_or_else(|| "0".to_string()),
            max_display: max.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string()),
        }
    }

    /// Create a validation failed error.
    pub fn validation_failed(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ValidationFailed {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Create a wrong value count error.
    pub fn wrong_value_count(expected: usize, got: usize) -> Self {
        Self::WrongValueCount { expected, got }
    }
}
