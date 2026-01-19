//! UI utilities for serial monitor frontends
//!
//! This module provides framework-agnostic utilities that help frontends
//! display and configure data consistently without duplicating logic.
//!
//! ## Structure
//!
//! - `util/` - Generic, reusable utilities (text editing, config panels)
//! - Top-level modules - Serial monitor specific display helpers

pub mod descriptions;
pub mod encoding;
pub mod serial_config;
mod timestamp;
mod units;
pub mod util;

// Re-export util submodules at ui level for ergonomic imports
pub use util::config;
pub use util::text;

// Re-export commonly used types for convenience
pub use util::{
    ConfigKeyResult,
    ConfigNav,
    EditMode,
    FieldDef,
    FieldValue,
    Section,
    TextBuffer,
    // Formatting utilities
    format_bytes,
    format_duration,
    format_rate,
    // Escape sequence parsing
    parse_escape_sequences,
    // Text display utilities
    slice_by_display_width,
};

pub use timestamp::TimestampFormat;
pub use units::{SizeUnit, TimeUnit};
