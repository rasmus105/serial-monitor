//! Traffic view for connected state - displays sent/received data.

use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Fill};
use serial_core::ui::encoding::{encoding_display, ENCODING_VARIANTS};
use serial_core::{Direction, Encoding};
use std::fmt;
use std::time::{Duration, SystemTime};

use crate::app::{ConnectedState, Message, MessageKind};
use crate::theme::Theme;

/// Format a timestamp relative to session start
fn format_timestamp(timestamp: SystemTime, session_start: SystemTime) -> String {
    let elapsed = timestamp.duration_since(session_start).unwrap_or(Duration::ZERO);
    format!("+{:.3}s", elapsed.as_secs_f64())
}

// Wrapper type for Encoding in pick_list
#[derive(Debug, Clone, Copy, PartialEq)]
struct EncodingOption(Encoding);

impl fmt::Display for EncodingOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", encoding_display(self.0))
    }
}

// Wrapper type for line ending options in pick_list
#[derive(Debug, Clone, Copy, PartialEq)]
struct LineEndingOption(usize);

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

/// Render the traffic view.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    // Encoding selector
    let encoding_options: Vec<EncodingOption> =
        ENCODING_VARIANTS.iter().copied().map(EncodingOption).collect();
    let current_encoding = EncodingOption(state.encoding);
    let encoding_picker = pick_list(encoding_options, Some(current_encoding), |opt| {
        Message::SelectEncoding(opt.0)
    })
    .width(100);

    // Header with connection info
    let header = row![
        text(&state.port_name).size(18),
        Space::new().width(10),
        text(format!("@ {} baud", state.config.baud_rate))
            .size(14)
            .color(Theme::MUTED),
        Space::new().width(Fill),
        text("Encoding:").size(14),
        Space::new().width(5),
        encoding_picker,
        Space::new().width(20),
        button(text("Disconnect")).on_press(Message::Disconnect),
    ]
    .padding(10)
    .align_y(Alignment::Center);

    // Statistics
    let stats = state.handle.statistics();
    let stats_text = text(format!(
        "RX: {} bytes | TX: {} bytes | Duration: {}s",
        stats.bytes_rx(),
        stats.bytes_tx(),
        stats.duration().as_secs()
    ))
    .size(12)
    .color(Theme::MUTED);

    // Filter controls toolbar
    let show_tx_checkbox = checkbox(state.show_tx)
        .label("TX")
        .on_toggle(|_| Message::ToggleShowTx)
        .text_size(14);
    let show_rx_checkbox = checkbox(state.show_rx)
        .label("RX")
        .on_toggle(|_| Message::ToggleShowRx)
        .text_size(14);
    let clear_btn = button(text("Clear").size(14)).on_press(Message::ClearBuffer);

    let filter_row = row![
        text("Show:").size(14),
        Space::new().width(8),
        show_tx_checkbox,
        Space::new().width(10),
        show_rx_checkbox,
        Space::new().width(Fill),
        clear_btn,
    ]
    .padding([5, 10])
    .align_y(Alignment::Center);

    // Data display
    let data_content: Element<'_, Message> = if state.display_lines.is_empty() {
        container(text("No data yet...").color(Theme::MUTED).size(14))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
    } else {
        let lines: Vec<Element<'_, Message>> = state
            .display_lines
            .iter()
            .map(|line| {
                let (prefix, color) = match line.direction {
                    Direction::Tx => ("TX", Theme::TX),
                    Direction::Rx => ("RX", Theme::RX),
                };
                let timestamp = format_timestamp(line.timestamp, state.session_start);
                row![
                    text(prefix).color(color).size(14),
                    Space::new().width(8),
                    text(timestamp).color(Theme::MUTED).size(12),
                    Space::new().width(10),
                    text(&line.content).size(14),
                ]
                .into()
            })
            .collect();

        scrollable(column(lines).spacing(2).padding(10))
            .height(Fill)
            .anchor_bottom()
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(8)
                    .scroller_width(8),
            ))
            .into()
    };

    // Line ending selector
    let line_ending_options: Vec<LineEndingOption> = (0..4).map(LineEndingOption).collect();
    let current_line_ending = LineEndingOption(state.send_line_ending_index);
    let line_ending_picker = pick_list(line_ending_options, Some(current_line_ending), |opt| {
        Message::SelectSendLineEnding(opt.0)
    })
    .width(120);

    // Send input
    let send_input = text_input("Type to send...", &state.send_input)
        .on_input(Message::SendInput)
        .on_submit(Message::Send)
        .width(Fill);

    let send_btn = button(text("Send")).on_press(Message::Send);

    let send_row = row![
        send_input,
        Space::new().width(10),
        line_ending_picker,
        Space::new().width(10),
        send_btn,
    ]
    .spacing(0)
    .padding(10)
    .align_y(Alignment::Center);

    // Message display
    let message_row = if let Some((msg, kind)) = &state.message {
        let color = match kind {
            MessageKind::Info => Theme::INFO,
            MessageKind::Error => Theme::ERROR,
        };
        text(msg).color(color).size(12)
    } else {
        text("")
    };

    let content = column![
        header,
        container(stats_text).width(Fill).center_x(Fill),
        filter_row,
        container(data_content)
            .width(Fill)
            .height(Fill)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(Theme::BG)),
                border: iced::Border {
                    color: Theme::BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        send_row,
        message_row,
    ]
    .spacing(5);

    container(content)
        .width(Fill)
        .height(Fill)
        .padding(10)
        .into()
}
