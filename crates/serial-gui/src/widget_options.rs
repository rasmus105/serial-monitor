//! Pick-list wrapper types for iced widgets.
//!
//! These wrapper types provide Display implementations needed by iced's pick_list widget.
//! They wrap serial-core types and provide human-readable labels.

use serial_core::ui::encoding::encoding_display;
use serial_core::ui::serial_config::{
    data_bits_display, flow_control_display, parity_display, stop_bits_display,
};
use serial_core::{DataBits, Encoding, FlowControl, Parity, StopBits};
use std::fmt;

// =============================================================================
// Serial config wrappers (used in pre_connect)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataBitsOption(pub DataBits);

impl fmt::Display for DataBitsOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", data_bits_display(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParityOption(pub Parity);

impl fmt::Display for ParityOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", parity_display(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopBitsOption(pub StopBits);

impl fmt::Display for StopBitsOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", stop_bits_display(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowControlOption(pub FlowControl);

impl fmt::Display for FlowControlOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", flow_control_display(self.0))
    }
}

/// RX Chunking option (index-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxChunkingOption(pub usize);

impl fmt::Display for RxChunkingOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => write!(f, "None (Raw)"),
            1 => write!(f, "LF (\\n)"),
            2 => write!(f, "CR (\\r)"),
            3 => write!(f, "CRLF (\\r\\n)"),
            _ => write!(f, "Unknown"),
        }
    }
}

// =============================================================================
// Display/traffic wrappers
// =============================================================================

/// Wrapper type for Encoding in pick_list
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncodingOption(pub Encoding);

impl fmt::Display for EncodingOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", encoding_display(self.0))
    }
}

/// Wrapper type for line ending options in pick_list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEndingOption(pub usize);

impl fmt::Display for LineEndingOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.0 {
            0 => "None",
            1 => "LF (\\n)",
            2 => "CR (\\r)",
            3 => "CRLF (\\r\\n)",
            _ => "None",
        };
        write!(f, "{}", label)
    }
}

/// Wrapper type for timestamp format in pick_list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimestampFormatOption(pub usize);

impl fmt::Display for TimestampFormatOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.0 {
            0 => "Relative",
            1 => "HH:MM:SS.mmm",
            2 => "HH:MM:SS",
            _ => "Relative",
        };
        write!(f, "{}", label)
    }
}

/// Wrapper type for scroll mode in pick_list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollModeOption {
    /// Off: No automatic scrolling whatsoever
    Off,
    /// Auto-scroll: stays at bottom when new data arrives, allows scrolling up
    Auto,
    /// Locked: always shows latest, cannot scroll up
    Locked,
}

impl fmt::Display for ScrollModeOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScrollModeOption::Off => write!(f, "Off"),
            ScrollModeOption::Auto => write!(f, "Auto-scroll"),
            ScrollModeOption::Locked => write!(f, "Lock to bottom"),
        }
    }
}

// =============================================================================
// Static option arrays (to avoid allocations every frame)
// =============================================================================

/// Data bits options
pub const DATA_BITS_OPTIONS: &[DataBitsOption] = &[
    DataBitsOption(DataBits::Five),
    DataBitsOption(DataBits::Six),
    DataBitsOption(DataBits::Seven),
    DataBitsOption(DataBits::Eight),
];

/// Parity options
pub const PARITY_OPTIONS: &[ParityOption] = &[
    ParityOption(Parity::None),
    ParityOption(Parity::Odd),
    ParityOption(Parity::Even),
];

/// Stop bits options
pub const STOP_BITS_OPTIONS: &[StopBitsOption] =
    &[StopBitsOption(StopBits::One), StopBitsOption(StopBits::Two)];

/// Flow control options
pub const FLOW_CONTROL_OPTIONS: &[FlowControlOption] = &[
    FlowControlOption(FlowControl::None),
    FlowControlOption(FlowControl::Software),
    FlowControlOption(FlowControl::Hardware),
];

/// RX chunking options
pub const RX_CHUNKING_OPTIONS: &[RxChunkingOption] = &[
    RxChunkingOption(0),
    RxChunkingOption(1),
    RxChunkingOption(2),
    RxChunkingOption(3),
];

/// Line ending options
pub const LINE_ENDING_OPTIONS: &[LineEndingOption] = &[
    LineEndingOption(0),
    LineEndingOption(1),
    LineEndingOption(2),
    LineEndingOption(3),
];

/// Timestamp format options
pub const TIMESTAMP_FORMAT_OPTIONS: &[TimestampFormatOption] = &[
    TimestampFormatOption(0),
    TimestampFormatOption(1),
    TimestampFormatOption(2),
];

/// Scroll mode options
pub const SCROLL_MODE_OPTIONS: &[ScrollModeOption] = &[
    ScrollModeOption::Off,
    ScrollModeOption::Auto,
    ScrollModeOption::Locked,
];

/// Static encoding options
pub const ENCODING_OPTIONS: &[EncodingOption] = &[
    EncodingOption(Encoding::Utf8),
    EncodingOption(Encoding::Ascii),
    EncodingOption(Encoding::Hex(serial_core::HexFormat {
        group_size: 1,
        separator: ' ',
        uppercase: true,
    })),
    EncodingOption(Encoding::Binary(serial_core::BinaryFormat {
        group_size: 8,
        separator: ' ',
    })),
];
