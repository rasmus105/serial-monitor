//! Tooltip descriptions for UI fields
//!
//! Centralized descriptions that can be used by any frontend for tooltips,
//! help text, or accessibility labels.

/// Descriptions for display settings
pub mod display {
    /// Encoding setting tooltip
    pub const ENCODING: &str =
        "How to decode received bytes for display. UTF-8 shows text, Hex shows raw bytes, etc.";

    /// Show TX toggle tooltip
    pub const SHOW_TX: &str = "Show data you send to the device";

    /// Show RX toggle tooltip
    pub const SHOW_RX: &str = "Show data received from the device";

    /// Timestamps toggle tooltip
    pub const TIMESTAMPS: &str = "Show timestamp for each line of data";

    /// Timestamp format tooltip
    pub const TIMESTAMP_FORMAT: &str = "How to display timestamps. Relative shows time since connection, Absolute shows wall clock time.";

    /// Scroll mode tooltip
    pub const SCROLL_MODE: &str = "Auto-scroll follows new data but allows scrolling up. Lock to bottom always shows latest data.";
}

/// Descriptions for serial port settings
pub mod serial {
    /// Baud rate tooltip
    pub const BAUD_RATE: &str =
        "Communication speed in bits per second. Must match the device's configured baud rate.";

    /// Data bits tooltip
    pub const DATA_BITS: &str = "Number of data bits per frame. 8 is most common.";

    /// Parity tooltip
    pub const PARITY: &str = "Error detection bit. None is most common. Even/Odd add a parity bit for basic error checking.";

    /// Stop bits tooltip
    pub const STOP_BITS: &str = "Number of stop bits marking end of frame. 1 is most common.";

    /// Flow control tooltip
    pub const FLOW_CONTROL: &str = "Hardware (RTS/CTS) uses dedicated pins. Software (XON/XOFF) uses special bytes. None is most common.";

    /// RX chunking tooltip
    pub const RX_CHUNKING: &str =
        "How to split incoming data into lines. LF (newline) is most common for text protocols.";
}

/// Descriptions for actions
pub mod actions {
    /// Disconnect button tooltip
    pub const DISCONNECT: &str = "Close the serial port connection";

    /// Clear buffer tooltip
    pub const CLEAR: &str = "Clear all received and sent data from the display";

    /// Send line ending tooltip
    pub const LINE_ENDING: &str = "Characters to append when sending. LF (\\n) is common for Unix, CRLF (\\r\\n) for Windows.";
}
