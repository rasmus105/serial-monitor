//! UI utilities for serial monitor frontends
//!
//! This module provides framework-agnostic utilities that help frontends
//! display and configure data consistently without duplicating logic.
//!
//! These are presentation helpers that build on top of core types - they
//! don't affect core functionality, only how data is displayed to users.

mod timestamp;

pub use timestamp::TimestampFormat;
