//! Timestamp formatting utilities

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use strum::{Display, EnumIter, VariantArray};

/// Format for displaying timestamps in traffic views
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, VariantArray)]
pub enum TimestampFormat {
    /// Relative to session start (e.g., "+1.234s")
    #[default]
    #[strum(to_string = "Relative")]
    Relative,

    /// Absolute time with milliseconds (e.g., "12:34:56.789")
    #[strum(to_string = "HH:MM:SS.mmm")]
    AbsoluteMillis,

    /// Absolute time without milliseconds (e.g., "12:34:56")
    #[strum(to_string = "HH:MM:SS")]
    Absolute,
}

impl TimestampFormat {
    /// Format a timestamp according to this format
    ///
    /// For `Relative` format, the timestamp is shown relative to `session_start`.
    /// For absolute formats, `session_start` is ignored.
    pub fn format(&self, time: SystemTime, session_start: SystemTime) -> String {
        match self {
            TimestampFormat::Relative => {
                let elapsed = time.duration_since(session_start).unwrap_or(Duration::ZERO);
                let secs = elapsed.as_secs_f64();
                format!("+{:.3}s", secs)
            }
            TimestampFormat::AbsoluteMillis => {
                let (hours, minutes, seconds, millis) = time_of_day_parts(time);
                format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
            }
            TimestampFormat::Absolute => {
                let (hours, minutes, seconds, _) = time_of_day_parts(time);
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
        }
    }
}

/// Extract hours, minutes, seconds, and milliseconds from a SystemTime
fn time_of_day_parts(time: SystemTime) -> (u64, u64, u64, u64) {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis() as u64;

    let time_of_day = total_secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    (hours, minutes, seconds, millis)
}
