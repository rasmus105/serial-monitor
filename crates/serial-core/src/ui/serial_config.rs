//! Display utilities for serial port configuration types
//!
//! Since `DataBits`, `Parity`, `StopBits`, and `FlowControl` are re-exported
//! from `tokio_serial`, we can't add strum derives to them directly. This
//! module provides const arrays and display name functions for these types.

use crate::{DataBits, FlowControl, Parity, StopBits};

/// All `DataBits` variants in order
pub const DATA_BITS_VARIANTS: &[DataBits] = &[
    DataBits::Five,
    DataBits::Six,
    DataBits::Seven,
    DataBits::Eight,
];

/// Display name for a `DataBits` value
pub fn data_bits_display(value: DataBits) -> &'static str {
    match value {
        DataBits::Five => "5",
        DataBits::Six => "6",
        DataBits::Seven => "7",
        DataBits::Eight => "8",
    }
}

/// All `Parity` variants in order
pub const PARITY_VARIANTS: &[Parity] = &[Parity::None, Parity::Odd, Parity::Even];

/// Display name for a `Parity` value
pub fn parity_display(value: Parity) -> &'static str {
    match value {
        Parity::None => "None",
        Parity::Odd => "Odd",
        Parity::Even => "Even",
    }
}

/// All `StopBits` variants in order
pub const STOP_BITS_VARIANTS: &[StopBits] = &[StopBits::One, StopBits::Two];

/// Display name for a `StopBits` value
pub fn stop_bits_display(value: StopBits) -> &'static str {
    match value {
        StopBits::One => "1",
        StopBits::Two => "2",
    }
}

/// All `FlowControl` variants in order
pub const FLOW_CONTROL_VARIANTS: &[FlowControl] = &[
    FlowControl::None,
    FlowControl::Software,
    FlowControl::Hardware,
];

/// Display name for a `FlowControl` value
pub fn flow_control_display(value: FlowControl) -> &'static str {
    match value {
        FlowControl::None => "None",
        FlowControl::Software => "Software (XON/XOFF)",
        FlowControl::Hardware => "Hardware (RTS/CTS)",
    }
}

/// Common baud rates for serial communication
pub const COMMON_BAUD_RATES: &[u32] = &[
    300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];
