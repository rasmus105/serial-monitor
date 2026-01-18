//! Config panel for the traffic view.

use iced::widget::{Space, button, checkbox, column, container, pick_list, row, scrollable, text};
use iced::{Alignment, Element, Fill, Length};
use serial_core::ui::descriptions;
use serial_core::ui::{format_bytes, format_duration};

use crate::app::{ConnectedMsg, ConnectedState, Message, ScrollState};
use crate::theme::{Theme, font_size};

use super::widgets::{
    ENCODING_OPTIONS, EncodingOption, SCROLL_MODE_OPTIONS, ScrollModeOption,
    TIMESTAMP_FORMAT_OPTIONS, TimestampFormatOption, collapsible_section_header, config_grid_cell,
    config_grid_cell_owned, config_row_info_owned, config_row_styled_indented_with_tooltip,
    config_row_styled_with_tooltip, divider,
};

/// Config panel width
const PANEL_WIDTH: f32 = 320.0;

/// Build the config panel on the right side
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    let stats = state.handle.statistics();
    let duration = stats.duration();
    let duration_str = format_duration(duration.as_secs());

    // === Panel Header with close button ===
    let header = row![
        text("●")
            .size(font_size::SMALL)
            .color(Theme::STATUS_CONNECTED),
        Space::new().width(4),
        text("Settings")
            .size(font_size::HEADER)
            .color(Theme::TEXT_PRIMARY),
        Space::new().width(Fill),
        button(text("x").size(font_size::SMALL))
            .on_press(Message::Connected(ConnectedMsg::ToggleConfigPanel))
            .padding([2, 8]),
    ]
    .align_y(Alignment::Center)
    .padding(8);

    // === Action Buttons (at the top) ===
    let disconnect_btn = button(text("Disconnect").size(font_size::BODY))
        .on_press(Message::Connected(ConnectedMsg::Disconnect))
        .style(Theme::button_secondary)
        .width(Fill);

    let clear_btn = button(text("Clear").size(font_size::BODY))
        .on_press(Message::Connected(ConnectedMsg::ClearBuffer))
        .style(Theme::button_secondary)
        .width(Fill);

    let actions_section =
        column![row![disconnect_btn, Space::new().width(5), clear_btn,].width(Fill),]
            .spacing(4)
            .padding(8);

    // === Connection Info Section (collapsible) ===
    let connection_section = build_connection_section(state);

    // === Statistics Section (collapsible) ===
    let stats_section = build_stats_section(state, &duration_str, stats);

    // === Display/Options Section (collapsible) ===
    let options_section = build_options_section(state);

    // === Combine All Sections ===
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
        .style(Theme::bordered_container)
        .width(Length::Fixed(PANEL_WIDTH))
        .height(Fill)
        .into()
}

/// Build the connection info section (collapsible)
fn build_connection_section(state: &ConnectedState) -> Element<'_, Message> {
    let collapsed = state.collapsed_sections.contains("Connection");

    if collapsed {
        column![collapsible_section_header("Connection", true),]
            .spacing(4)
            .padding(8)
            .into()
    } else {
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

        column![
            collapsible_section_header("Connection", false),
            connection_grid,
        ]
        .spacing(4)
        .padding(8)
        .into()
    }
}

/// Build the statistics section (collapsible)
fn build_stats_section<'a>(
    state: &'a ConnectedState,
    duration_str: &str,
    stats: &serial_core::Statistics,
) -> Element<'a, Message> {
    let stats_collapsed = state.collapsed_sections.contains("Statistics");

    if stats_collapsed {
        column![collapsible_section_header("Statistics", true),]
            .spacing(4)
            .padding(8)
            .into()
    } else {
        let duration_row =
            config_row_info_owned("Duration", duration_str.to_string(), Some(Theme::SUCCESS));
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
    }
}

/// Build the display options section (collapsible)
fn build_options_section(state: &ConnectedState) -> Element<'_, Message> {
    let collapsed = state.collapsed_sections.contains("Options");

    if collapsed {
        column![collapsible_section_header("Options", true),]
            .spacing(0)
            .padding(8)
            .into()
    } else {
        let current_encoding = EncodingOption(state.encoding);
        let encoding_picker = pick_list(ENCODING_OPTIONS, Some(current_encoding), |opt| {
            Message::Connected(ConnectedMsg::SelectEncoding(opt.0))
        })
        .width(Length::Fixed(100.0))
        .text_size(font_size::SMALL)
        .style(Theme::pick_list);

        let encoding_row = config_row_styled_with_tooltip(
            "Encoding",
            encoding_picker,
            descriptions::display::ENCODING,
            0,
        );

        // Show TX/RX as separate rows with consistent layout
        let show_tx_toggle = checkbox(state.show_tx)
            .on_toggle(|_| Message::Connected(ConnectedMsg::ToggleShowTx))
            .text_size(font_size::SMALL);
        let show_tx_row = config_row_styled_with_tooltip(
            "Show TX",
            show_tx_toggle,
            descriptions::display::SHOW_TX,
            1,
        );

        let show_rx_toggle = checkbox(state.show_rx)
            .on_toggle(|_| Message::Connected(ConnectedMsg::ToggleShowRx))
            .text_size(font_size::SMALL);
        let show_rx_row = config_row_styled_with_tooltip(
            "Show RX",
            show_rx_toggle,
            descriptions::display::SHOW_RX,
            2,
        );

        // Timestamps with format as sub-option
        let timestamps_toggle = checkbox(state.show_timestamps)
            .on_toggle(|_| Message::Connected(ConnectedMsg::ToggleTimestamps))
            .text_size(font_size::SMALL);
        let timestamps_row = config_row_styled_with_tooltip(
            "Timestamps",
            timestamps_toggle,
            descriptions::display::TIMESTAMPS,
            3,
        );

        // Timestamp format - always visible but grayed out when disabled
        let current_format = TimestampFormatOption(state.timestamp_format_index);
        let format_picker: Element<'_, Message> = if state.show_timestamps {
            pick_list(TIMESTAMP_FORMAT_OPTIONS, Some(current_format), |opt| {
                Message::Connected(ConnectedMsg::SelectTimestampFormat(opt.0))
            })
            .width(Length::Fixed(120.0))
            .text_size(font_size::SMALL)
            .style(Theme::pick_list)
            .into()
        } else {
            // Disabled state - show current value but not interactive
            container(
                text(format!("{}", current_format))
                    .size(font_size::SMALL)
                    .color(Theme::MUTED),
            )
            .width(Length::Fixed(120.0))
            .into()
        };
        let timestamp_format_row = config_row_styled_indented_with_tooltip(
            "Format",
            format_picker,
            !state.show_timestamps,
            descriptions::display::TIMESTAMP_FORMAT,
            4,
        );

        // Auto-scroll settings
        let current_scroll_mode = match &state.scroll_state {
            ScrollState::LockedToBottom => ScrollModeOption::Locked,
            ScrollState::AutoScroll { .. } | ScrollState::Manual { .. } => ScrollModeOption::Auto,
        };
        let scroll_mode_picker = pick_list(SCROLL_MODE_OPTIONS, Some(current_scroll_mode), |opt| {
            Message::Connected(ConnectedMsg::SelectScrollMode(opt))
        })
        .width(Length::Fixed(120.0))
        .text_size(font_size::SMALL)
        .style(Theme::pick_list);
        let auto_scroll_row = config_row_styled_with_tooltip(
            "Scroll",
            scroll_mode_picker,
            descriptions::display::SCROLL_MODE,
            5,
        );

        // Show current scroll state indicator when in manual mode
        let scroll_status_row: Element<'_, Message> = match &state.scroll_state {
            ScrollState::Manual { .. } => config_row_styled_indented_with_tooltip(
                "Status",
                text("Paused (scroll down to resume)")
                    .size(font_size::SMALL)
                    .color(Theme::MUTED),
                false,
                "", // No tooltip for status
                6,
            ),
            _ => Space::new().height(0).into(),
        };

        column![
            collapsible_section_header("Options", false),
            encoding_row,
            show_tx_row,
            show_rx_row,
            timestamps_row,
            timestamp_format_row,
            auto_scroll_row,
            scroll_status_row,
        ]
        .spacing(0)
        .padding(8)
        .into()
    }
}
