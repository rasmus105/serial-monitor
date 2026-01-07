//! Traffic view for connected state - displays sent/received data.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text,
    text_input, tooltip, Space,
};
use iced::{Alignment, Element, Fill, Length};
use serial_core::ui::descriptions;
use serial_core::ui::encoding::{encoding_display, ENCODING_VARIANTS};
use serial_core::ui::TimestampFormat;
use serial_core::{Direction, Encoding};
use std::fmt;
use std::time::{Duration, SystemTime};

/// Estimated row height in pixels for virtual scrolling calculations
const ROW_HEIGHT: f32 = 20.0;
/// Number of extra rows to render above and below the viewport as buffer
const RENDER_BUFFER: usize = 10;

use crate::app::{ConnectedState, Message, ScrollState};
use crate::theme::Theme;

/// Create a horizontal divider line
fn divider<'a>() -> Element<'a, Message> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Theme::BORDER)),
            ..Default::default()
        })
        .into()
}

/// Format a timestamp according to the selected format
fn format_timestamp(timestamp: SystemTime, session_start: SystemTime, format_index: usize) -> String {
    let format = match format_index {
        0 => TimestampFormat::Relative,
        1 => TimestampFormat::AbsoluteMillis,
        2 => TimestampFormat::Absolute,
        _ => TimestampFormat::Relative,
    };
    format.format(timestamp, session_start)
}

/// Get the maximum timestamp width for the current format
///
/// For relative timestamps, the width depends on session duration.
/// For absolute timestamps, the width is fixed.
fn max_timestamp_width(format_index: usize, session_start: SystemTime) -> usize {
    match format_index {
        0 => {
            // Relative: "+X.XXXs" where X grows with time
            // Calculate based on current elapsed time
            let elapsed = SystemTime::now()
                .duration_since(session_start)
                .unwrap_or(Duration::ZERO);
            let secs = elapsed.as_secs_f64();
            // Format is "+{secs:.3}s" - count digits before decimal
            let whole_part = secs as u64;
            let digits = if whole_part == 0 {
                1
            } else {
                (whole_part as f64).log10().floor() as usize + 1
            };
            // "+", digits, ".", 3 decimal digits, "s"
            1 + digits + 1 + 3 + 1
        }
        1 => 12, // "HH:MM:SS.mmm"
        2 => 8,  // "HH:MM:SS"
        _ => 10,
    }
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

// Wrapper type for timestamp format in pick_list
#[derive(Debug, Clone, Copy, PartialEq)]
struct TimestampFormatOption(usize);

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

// Wrapper type for scroll mode in pick_list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollModeOption {
    /// Auto-scroll: stays at bottom when new data arrives, allows scrolling up
    Auto,
    /// Locked: always shows latest, cannot scroll up
    Locked,
}

impl fmt::Display for ScrollModeOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScrollModeOption::Auto => write!(f, "Auto-scroll"),
            ScrollModeOption::Locked => write!(f, "Lock to bottom"),
        }
    }
}

/// Render the traffic view.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    // Main content area with optional config panel
    let main_content: Element<'_, Message> = if state.show_config_panel {
        row![
            traffic_area(state),
            config_panel(state),
        ]
        .spacing(5)
        .into()
    } else {
        // When panel is hidden, show a small button to reveal it
        let show_panel_btn = button(text(">").size(14))
            .on_press(Message::ToggleConfigPanel)
            .padding([10, 5]);
        
        row![
            traffic_area(state),
            container(show_panel_btn)
                .height(Fill)
                .align_y(Alignment::Center),
        ]
        .spacing(5)
        .into()
    };

    container(main_content)
        .width(Fill)
        .height(Fill)
        .padding(10)
        .into()
}

/// Build the main traffic display area (data lines + send input)
fn traffic_area(state: &ConnectedState) -> Element<'_, Message> {
    // Data display with virtual scrolling
    let data_content: Element<'_, Message> = if state.display_lines.is_empty() {
        container(text("No data yet...").color(Theme::MUTED).size(14))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
    } else {
        let timestamp_width = if state.show_timestamps {
            max_timestamp_width(state.timestamp_format_index, state.session_start)
        } else {
            0
        };

        let total_lines = state.display_lines.len();
        
        // Calculate visible range based on scroll state
        let (visible_start, visible_end) = match &state.scroll_state {
            ScrollState::LockedToBottom => {
                // When locked to bottom, we render the last N lines that fit
                // but still need to handle the case where viewport is unknown
                let viewport_lines = state.viewport_height
                    .map(|h| (h / ROW_HEIGHT).ceil() as usize)
                    .unwrap_or(50); // Default estimate
                let start = total_lines.saturating_sub(viewport_lines + RENDER_BUFFER);
                (start, total_lines)
            }
            ScrollState::AutoScroll { offset } | ScrollState::Manual { offset } => {
                // Calculate which lines are visible based on scroll offset
                let viewport_height = state.viewport_height.unwrap_or(500.0);
                let start_line = (*offset / ROW_HEIGHT).floor() as usize;
                let visible_count = (viewport_height / ROW_HEIGHT).ceil() as usize;
                
                let visible_start = start_line.saturating_sub(RENDER_BUFFER);
                let visible_end = (start_line + visible_count + RENDER_BUFFER).min(total_lines);
                (visible_start, visible_end)
            }
        };

        // Top spacer to maintain scroll position
        let top_spacer_height = visible_start as f32 * ROW_HEIGHT;
        
        // Only render visible lines
        let visible_lines: Vec<Element<'_, Message>> = state
            .display_lines
            .iter()
            .skip(visible_start)
            .take(visible_end - visible_start)
            .map(|line| {
                let (prefix, color) = match line.direction {
                    Direction::Tx => ("TX", Theme::TX),
                    Direction::Rx => ("RX", Theme::RX),
                };
                
                let mut line_row = row![text(prefix).color(color).size(14), Space::new().width(8),];

                if state.show_timestamps {
                    let timestamp =
                        format_timestamp(line.timestamp, state.session_start, state.timestamp_format_index);
                    // Right-align timestamp within fixed width
                    let padded_timestamp = format!("{:>width$}", timestamp, width = timestamp_width);
                    line_row = line_row.push(text(padded_timestamp).color(Theme::MUTED).size(12));
                    line_row = line_row.push(Space::new().width(10));
                }

                line_row = line_row.push(text(&line.content).size(14));
                line_row.into()
            })
            .collect();

        // Bottom spacer to maintain total scroll height
        let bottom_spacer_height = (total_lines - visible_end) as f32 * ROW_HEIGHT;
        
        // Build scrollable content with spacers for virtual scrolling
        let content = column![
            Space::new().width(Fill).height(top_spacer_height),
            column(visible_lines).spacing(2),
            Space::new().width(Fill).height(bottom_spacer_height),
        ]
        .padding(10)
        .width(Fill);

        // Determine if we should anchor to bottom
        let scrollable_widget = scrollable(content)
            .height(Fill)
            .width(Fill)
            .on_scroll(Message::ScrollChanged)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(8).scroller_width(8),
            ));

        // Only anchor to bottom when locked
        let scrollable_widget = if matches!(state.scroll_state, ScrollState::LockedToBottom) {
            scrollable_widget.anchor_bottom()
        } else {
            scrollable_widget
        };

        scrollable_widget.into()
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

    container(
        column![
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
        ]
        .spacing(5)
    )
    .width(Fill)
    .into()
}

/// Build the config panel on the right side
fn config_panel(state: &ConnectedState) -> Element<'_, Message> {
    let stats = state.handle.statistics();
    let duration = stats.duration();
    let duration_str = format_duration(duration);

    // === Panel Header with close button ===
    let header = row![
        text("Settings").size(14).color(Theme::PRIMARY),
        Space::new().width(Fill),
        button(text("x").size(12))
            .on_press(Message::ToggleConfigPanel)
            .padding([2, 8]),
    ]
    .align_y(Alignment::Center)
    .padding(8);

    // === Action Buttons (at the top) ===
    let disconnect_btn = button(text("Disconnect").size(12))
        .on_press(Message::Disconnect)
        .width(Fill);

    let clear_btn = button(text("Clear").size(12))
        .on_press(Message::ClearBuffer)
        .width(Fill);

    let actions_section = column![
        section_header("Actions"),
        row![disconnect_btn, Space::new().width(5), clear_btn,].width(Fill),
    ]
    .spacing(4)
    .padding(8);

    // === Connection Info Section (grid layout) ===
    let port_value = state.port_name.as_str();
    let baud_value = format!("{}", state.config.baud_rate);
    let data_bits_value = match state.config.data_bits {
        serial_core::DataBits::Five => "5",
        serial_core::DataBits::Six => "6",
        serial_core::DataBits::Seven => "7",
        serial_core::DataBits::Eight => "8",
    };
    let parity_value = match state.config.parity {
        serial_core::Parity::None => "None",
        serial_core::Parity::Odd => "Odd",
        serial_core::Parity::Even => "Even",
    };
    let stop_bits_value = match state.config.stop_bits {
        serial_core::StopBits::One => "1",
        serial_core::StopBits::Two => "2",
    };
    let flow_control_value = match state.config.flow_control {
        serial_core::FlowControl::None => "None",
        serial_core::FlowControl::Software => "SW",
        serial_core::FlowControl::Hardware => "HW",
    };

    // Grid layout: 3 rows x 2 columns
    let connection_grid = column![
        // Row 1: Port | Baud
        row![
            config_grid_cell("Port", port_value),
            Space::new().width(8),
            config_grid_cell_owned("Baud", baud_value),
        ],
        // Row 2: Data Bits | Parity  
        row![
            config_grid_cell("Data", data_bits_value),
            Space::new().width(8),
            config_grid_cell("Parity", parity_value),
        ],
        // Row 3: Stop Bits | Flow Control
        row![
            config_grid_cell("Stop", stop_bits_value),
            Space::new().width(8),
            config_grid_cell("Flow", flow_control_value),
        ],
    ]
    .spacing(4);

    let connection_section = column![
        section_header("Connection"),
        connection_grid,
    ]
    .spacing(4)
    .padding(8);

    // === Statistics Section (collapsible) ===
    let stats_collapsed = state.collapsed_sections.contains("Statistics");
    let stats_section: Element<'_, Message> = if stats_collapsed {
        column![
            collapsible_section_header("Statistics", true),
        ]
        .spacing(4)
        .padding(8)
        .into()
    } else {
        let duration_row = config_row_info_owned("Duration", duration_str, Some(Theme::SUCCESS));
        let rx_row = config_row_info_owned("RX", format_bytes(stats.bytes_rx()), Some(Theme::RX));
        let tx_row = config_row_info_owned("TX", format_bytes(stats.bytes_tx()), Some(Theme::TX));

        column![
            collapsible_section_header("Statistics", false),
            duration_row,
            rx_row,
            tx_row,
        ]
        .spacing(4)
        .padding(8)
        .into()
    };

    // === Display/Options Section (at the bottom) ===
    let encoding_options: Vec<EncodingOption> =
        ENCODING_VARIANTS.iter().copied().map(EncodingOption).collect();
    let current_encoding = EncodingOption(state.encoding);
    let encoding_picker = pick_list(encoding_options, Some(current_encoding), |opt| {
        Message::SelectEncoding(opt.0)
    })
    .width(Length::Fixed(100.0))
    .text_size(12);

    let encoding_row = config_row_with_tooltip(
        "Encoding",
        encoding_picker,
        descriptions::display::ENCODING,
    );

    // Show TX/RX as separate rows with consistent layout
    let show_tx_toggle = checkbox(state.show_tx)
        .on_toggle(|_| Message::ToggleShowTx)
        .text_size(12);
    let show_tx_row = config_row_with_tooltip(
        "Show TX",
        show_tx_toggle,
        descriptions::display::SHOW_TX,
    );

    let show_rx_toggle = checkbox(state.show_rx)
        .on_toggle(|_| Message::ToggleShowRx)
        .text_size(12);
    let show_rx_row = config_row_with_tooltip(
        "Show RX",
        show_rx_toggle,
        descriptions::display::SHOW_RX,
    );

    // Timestamps with format as sub-option
    let timestamps_toggle = checkbox(state.show_timestamps)
        .on_toggle(|_| Message::ToggleTimestamps)
        .text_size(12);
    let timestamps_row = config_row_with_tooltip(
        "Timestamps",
        timestamps_toggle,
        descriptions::display::TIMESTAMPS,
    );

    // Timestamp format - always visible but grayed out when disabled
    let format_options: Vec<TimestampFormatOption> = (0..3).map(TimestampFormatOption).collect();
    let current_format = TimestampFormatOption(state.timestamp_format_index);
    let format_picker: Element<'_, Message> = if state.show_timestamps {
        pick_list(format_options, Some(current_format), |opt| {
            Message::SelectTimestampFormat(opt.0)
        })
        .width(Length::Fixed(120.0))
        .text_size(12)
        .into()
    } else {
        // Disabled state - show current value but not interactive
        container(
            text(format!("{}", current_format))
                .size(12)
                .color(Theme::MUTED)
        )
        .width(Length::Fixed(120.0))
        .into()
    };
    let timestamp_format_row = config_row_indented_with_tooltip(
        "Format",
        format_picker,
        !state.show_timestamps,
        descriptions::display::TIMESTAMP_FORMAT,
    );

    // Auto-scroll settings
    let current_scroll_mode = match &state.scroll_state {
        ScrollState::LockedToBottom => ScrollModeOption::Locked,
        ScrollState::AutoScroll { .. } | ScrollState::Manual { .. } => ScrollModeOption::Auto,
    };
    let scroll_mode_options = vec![ScrollModeOption::Auto, ScrollModeOption::Locked];
    let scroll_mode_picker = pick_list(scroll_mode_options, Some(current_scroll_mode), |opt| {
        Message::SelectScrollMode(opt)
    })
    .width(Length::Fixed(120.0))
    .text_size(12);
    let auto_scroll_row = config_row_with_tooltip(
        "Scroll",
        scroll_mode_picker,
        descriptions::display::SCROLL_MODE,
    );

    // Show current scroll state indicator when in manual mode
    let scroll_status_row: Element<'_, Message> = match &state.scroll_state {
        ScrollState::Manual { .. } => {
            config_row_indented(
                "Status",
                text("Paused (scroll down to resume)").size(11).color(Theme::MUTED),
                false,
            )
        }
        _ => Space::new().height(0).into(),
    };

    let options_section = column![
        section_header("Options"),
        encoding_row,
        show_tx_row,
        show_rx_row,
        timestamps_row,
        timestamp_format_row,
        auto_scroll_row,
        scroll_status_row,
    ]
    .spacing(4)
    .padding(8);

    // === Combine All Sections (new order) ===
    let panel_content = column![
        actions_section,
        divider(),
        connection_section,
        divider(),
        stats_section,
        divider(),
        options_section,
    ]
    .spacing(0);

    let full_panel = column![
        header,
        divider(),
        scrollable(panel_content)
            .height(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(4).scroller_width(4),
            )),
    ];

    container(full_panel)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Theme::BG)),
            border: iced::Border {
                color: Theme::BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .width(Length::Fixed(320.0))
        .height(Fill)
        .into()
}

/// Create a grid cell for connection info (label above, value below)
fn config_grid_cell<'a>(label: &'a str, value: &'a str) -> Element<'a, Message> {
    column![
        text(label).size(10).color(Theme::MUTED),
        text(value).size(12),
    ]
    .spacing(1)
    .width(Fill)
    .into()
}

/// Create a grid cell for connection info with owned value
fn config_grid_cell_owned<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    column![
        text(label).size(10).color(Theme::MUTED),
        text(value).size(12),
    ]
    .spacing(1)
    .width(Fill)
    .into()
}

/// Create a section header with divider
fn section_header<'a>(title: &'a str) -> Element<'a, Message> {
    column![
        text(title).size(12).color(Theme::MUTED),
        Space::new().height(2),
    ]
    .into()
}

/// Create a collapsible section header that can be clicked to toggle
fn collapsible_section_header<'a>(title: &'static str, collapsed: bool) -> Element<'a, Message> {
    let icon = if collapsed { "+" } else { "-" };
    
    button(
        row![
            text(icon).size(12).color(Theme::MUTED),
            Space::new().width(4),
            text(title).size(12).color(Theme::MUTED),
        ]
        .align_y(Alignment::Center)
    )
    .on_press(Message::ToggleSectionCollapse(title.to_string()))
    .padding([2, 4])
    .style(|_theme, _status| button::Style {
        background: None,
        text_color: Theme::MUTED,
        ..Default::default()
    })
    .into()
}

/// Create a config row with label on left, widget on right
#[allow(dead_code)]
fn config_row<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![
        text(label).size(12),
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create a config row with tooltip on the label
fn config_row_with_tooltip<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    tooltip_text: &'a str,
) -> Element<'a, Message> {
    let label_with_tooltip = tooltip(
        text(label).size(12),
        container(text(tooltip_text).size(11))
            .padding(8)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(Theme::BG)),
                border: iced::Border {
                    color: Theme::BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        tooltip::Position::Left,
    )
    .gap(5);

    row![
        label_with_tooltip,
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create an indented config row (for sub-options)
fn config_row_indented<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    disabled: bool,
) -> Element<'a, Message> {
    let label_color = if disabled { Theme::MUTED } else { Theme::TEXT };
    row![
        Space::new().width(16), // Indent
        text(label).size(12).color(label_color),
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create an indented config row with tooltip
fn config_row_indented_with_tooltip<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    disabled: bool,
    tooltip_text: &'a str,
) -> Element<'a, Message> {
    let label_color = if disabled { Theme::MUTED } else { Theme::TEXT };
    let label_with_tooltip = tooltip(
        text(label).size(12).color(label_color),
        container(text(tooltip_text).size(11))
            .padding(8)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(Theme::BG)),
                border: iced::Border {
                    color: Theme::BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        tooltip::Position::Left,
    )
    .gap(5);

    row![
        Space::new().width(16), // Indent
        label_with_tooltip,
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create an info row (read-only, right-aligned value) with owned string
fn config_row_info_owned<'a>(
    label: &'a str,
    value: String,
    color: Option<iced::Color>,
) -> Element<'a, Message> {
    let value_text = text(value).size(12);
    let value_text = if let Some(c) = color {
        value_text.color(c)
    } else {
        value_text
    };
    
    row![
        text(label).size(12).color(Theme::MUTED),
        Space::new().width(Fill),
        value_text,
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Format duration as human readable string
fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{:02}:{:02}", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format bytes as human readable string
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}
