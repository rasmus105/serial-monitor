//! Crash logging utilities for panic recovery.
//!
//! This module provides frontend-agnostic crash logging that any frontend can use
//! to capture panic information and save it to disk.
//!
//! # Usage
//!
//! Frontends should set a panic hook early in their initialization:
//!
//! ```ignore
//! std::panic::set_hook(Box::new(|panic_info| {
//!     // Restore terminal/UI state first (frontend-specific)
//!     
//!     let crash_info = CrashInfo::from_panic(panic_info)
//!         .with_context(format!("App state: {:?}", app_state));
//!     
//!     match write_crash_log(&crash_info) {
//!         Ok(path) => eprintln!("Crash log saved: {}", path.display()),
//!         Err(e) => eprintln!("Failed to save crash log: {}", e),
//!     }
//! }));
//! ```

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::time::SystemTime;

/// Information captured during a panic for crash logging.
pub struct CrashInfo {
    pub timestamp: SystemTime,
    pub panic_message: String,
    pub panic_location: Option<String>,
    pub backtrace: Backtrace,
    pub thread_name: Option<String>,
    pub additional_context: Option<String>,
}

impl CrashInfo {
    /// Create crash info from a panic hook's PanicHookInfo.
    pub fn from_panic(info: &PanicHookInfo<'_>) -> Self {
        let panic_message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };

        let panic_location = info.location().map(|loc| {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        });

        let thread_name = std::thread::current().name().map(String::from);

        Self {
            timestamp: SystemTime::now(),
            panic_message,
            panic_location,
            backtrace: Backtrace::force_capture(),
            thread_name,
            additional_context: None,
        }
    }

    /// Add optional context from the frontend (e.g., app state, current view).
    pub fn with_context(mut self, context: String) -> Self {
        self.additional_context = Some(context);
        self
    }
}

/// Returns the default directory for crash logs.
///
/// Uses `~/.cache/serial-monitor/` on Linux, with `/tmp/serial-monitor/` as fallback.
pub fn crash_log_directory() -> PathBuf {
    dirs::cache_dir()
        .map(|p| p.join("serial-monitor"))
        .unwrap_or_else(|| PathBuf::from("/tmp/serial-monitor"))
}

/// Write a crash log to disk.
///
/// Creates a file named `crash-{timestamp}.log` in the crash log directory.
/// Returns the path to the created file on success.
pub fn write_crash_log(info: &CrashInfo) -> std::io::Result<PathBuf> {
    let dir = crash_log_directory();
    std::fs::create_dir_all(&dir)?;

    let timestamp = format_timestamp_for_filename(info.timestamp);
    let filename = format!("crash-{}.log", timestamp);
    let path = dir.join(&filename);

    let content = format_crash_log(info);
    std::fs::write(&path, content)?;

    Ok(path)
}

/// Format SystemTime as ISO8601-like string for filenames (no colons).
/// Format: 2025-12-28T14-32-15
fn format_timestamp_for_filename(time: SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Convert to UTC components manually
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Calculate year, month, day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Format SystemTime as ISO8601 string for display.
/// Format: 2025-12-28T14:32:15Z
fn format_timestamp_for_display(time: SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Simplified algorithm - handles leap years
    let mut remaining_days = days as i64;
    let mut year = 1970i64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let days_in_months: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days_in_month in days_in_months {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }

    let day = remaining_days as u64 + 1;
    (year as u64, month, day)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn format_crash_log(info: &CrashInfo) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "CRASH at {}\n",
        format_timestamp_for_display(info.timestamp)
    ));

    // Thread
    if let Some(ref name) = info.thread_name {
        output.push_str(&format!("Thread: {}\n", name));
    } else {
        output.push_str("Thread: <unnamed>\n");
    }

    // Location
    if let Some(ref loc) = info.panic_location {
        output.push_str(&format!("Location: {}\n", loc));
    }

    // Message
    output.push_str(&format!("Message: {}\n", info.panic_message));

    // Backtrace
    output.push_str("\n--- Backtrace ---\n");
    output.push_str(&info.backtrace.to_string());

    // Additional context (if provided)
    if let Some(ref ctx) = info.additional_context {
        output.push_str("\n--- Context ---\n");
        output.push_str(ctx);
        output.push('\n');
    }

    output
}
