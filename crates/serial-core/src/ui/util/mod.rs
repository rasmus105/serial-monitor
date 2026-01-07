//! Generic UI utilities that are framework-agnostic and reusable.
//!
//! These utilities have no knowledge of serial monitor domain concepts
//! and could theoretically be used in any application.

pub mod config;
pub mod text;

pub use config::{ConfigKeyResult, ConfigNav, EditMode, FieldDef, FieldValue, Section};
pub use text::TextBuffer;
