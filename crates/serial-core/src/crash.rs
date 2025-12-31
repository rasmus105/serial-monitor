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
use std::fmt::{self, Display};
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};

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

        let panic_location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

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

impl Display for CrashInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Header
        let dt: DateTime<Utc> = self.timestamp.into();
        writeln!(f, "CRASH at {}", dt.format("%Y-%m-%dT%H:%M:%SZ"))?;

        // Thread
        let thread_display = self.thread_name.as_deref().unwrap_or("<unnamed>");
        writeln!(f, "Thread: {}", thread_display)?;

        // Location
        if let Some(ref loc) = self.panic_location {
            writeln!(f, "Location: {}", loc)?;
        }

        // Message
        writeln!(f, "Message: {}", self.panic_message)?;

        // Backtrace
        writeln!(f, "\n--- Backtrace ---")?;
        write!(f, "{}", self.backtrace)?;

        // Additional context (if provided)
        if let Some(ref ctx) = self.additional_context {
            writeln!(f, "\n--- Context ---")?;
            writeln!(f, "{}", ctx)?;
        }

        Ok(())
    }
}

/// Returns the default directory for crash logs.
///
/// Uses the platform-appropriate cache directory (e.g., `~/.cache/serial-monitor/` on Linux,
/// `~/Library/Caches/serial-monitor/` on macOS, `C:\Users\<User>\AppData\Local\serial-monitor\`
/// on Windows), with the system temp directory as fallback.
pub fn crash_log_directory() -> PathBuf {
    dirs::cache_dir()
        .map(|p| p.join("serial-monitor"))
        .unwrap_or_else(|| std::env::temp_dir().join("serial-monitor"))
}

/// Write a crash log to disk.
///
/// Creates a file named `crash-{timestamp}.log` in the crash log directory.
/// Returns the path to the created file on success.
pub fn write_crash_log(info: &CrashInfo) -> std::io::Result<PathBuf> {
    let dir = crash_log_directory();
    std::fs::create_dir_all(&dir)?;

    let dt: DateTime<Utc> = info.timestamp.into();
    let timestamp = dt.format("%Y-%m-%dT%H-%M-%S");
    let filename = format!("crash-{}.log", timestamp);
    let path = dir.join(&filename);

    std::fs::write(&path, info.to_string())?;

    Ok(path)
}
