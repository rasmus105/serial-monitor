//! Generic UI utilities that are framework-agnostic and reusable.
//!
//! These utilities have no knowledge of serial monitor domain concepts
//! and could theoretically be used in any application.

pub mod config;
mod format;
pub mod text;

pub use config::{ConfigKeyResult, ConfigNav, EditMode, FieldDef, FieldValue, Section};
pub use format::{format_bytes, format_duration, format_rate};
pub use text::{TextBuffer, slice_by_display_width};
