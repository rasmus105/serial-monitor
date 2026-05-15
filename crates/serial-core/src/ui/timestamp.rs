//! Timestamp formatting utilities

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
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
                let local_time = DateTime::<Local>::from(time);
                local_time.format("%H:%M:%S%.3f").to_string()
            }
            TimestampFormat::Absolute => {
                let local_time = DateTime::<Local>::from(time);
                local_time.format("%H:%M:%S").to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use chrono::{DateTime, Local};

    use super::TimestampFormat;

    #[test]
    fn formats_relative_timestamp_from_session_start() {
        let session_start = UNIX_EPOCH + Duration::from_secs(10);
        let time = session_start + Duration::from_millis(1234);

        assert_eq!(
            TimestampFormat::Relative.format(time, session_start),
            "+1.234s"
        );
    }

    #[test]
    fn formats_absolute_timestamp_in_local_time() {
        let time = UNIX_EPOCH + Duration::from_secs(12 * 3600 + 34 * 60 + 56);
        let expected = DateTime::<Local>::from(time).format("%H:%M:%S").to_string();

        assert_eq!(TimestampFormat::Absolute.format(time, UNIX_EPOCH), expected);
    }

    #[test]
    fn formats_absolute_timestamp_with_local_milliseconds() {
        let time =
            UNIX_EPOCH + Duration::from_secs(12 * 3600 + 34 * 60 + 56) + Duration::from_millis(789);
        let expected = DateTime::<Local>::from(time)
            .format("%H:%M:%S%.3f")
            .to_string();

        assert_eq!(
            TimestampFormat::AbsoluteMillis.format(time, UNIX_EPOCH),
            expected
        );
    }
}
