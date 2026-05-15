//! Shared utilities for widgets.

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use serial_core::{DataBits, FlowControl, Parity, SerialConfig, StopBits};

use crate::keybind::KeyHint;

// Re-export formatting utilities from core
pub use serial_core::ui::{format_bytes, format_duration, format_rate};

/// Format serial settings in the conventional compact form, e.g. `115200 8N1`.
pub fn format_serial_config_compact(config: &SerialConfig) -> String {
    format!(
        "{} {}{}{}",
        config.baud_rate,
        data_bits_label(config.data_bits),
        parity_label(config.parity),
        stop_bits_label(config.stop_bits)
    )
}

/// Format flow control for compact status displays.
pub fn format_flow_control(flow_control: FlowControl) -> &'static str {
    match flow_control {
        FlowControl::None => "None",
        FlowControl::Software => "Software",
        FlowControl::Hardware => "Hardware",
    }
}

fn data_bits_label(data_bits: DataBits) -> &'static str {
    match data_bits {
        DataBits::Five => "5",
        DataBits::Six => "6",
        DataBits::Seven => "7",
        DataBits::Eight => "8",
    }
}

fn parity_label(parity: Parity) -> &'static str {
    match parity {
        Parity::None => "N",
        Parity::Odd => "O",
        Parity::Even => "E",
    }
}

fn stop_bits_label(stop_bits: StopBits) -> &'static str {
    match stop_bits {
        StopBits::One => "1",
        StopBits::Two => "2",
    }
}

/// Build a help bar line from key hints.
///
/// Creates a line of styled spans like: "Enter connect  r refresh  / search"
pub fn build_help_line(hints: &[KeyHint], key_style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(hint.key.to_string(), key_style));
        spans.push(Span::raw(format!(" {}", hint.description)));
    }
    Line::from(spans)
}
