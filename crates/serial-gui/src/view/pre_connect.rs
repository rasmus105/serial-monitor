//! Pre-connect view for port selection and configuration.

use iced::widget::{Space, button, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Fill, Length};
use serial_core::ui::serial_config::COMMON_BAUD_RATES;

use crate::app::{Message, PreConnectMsg, PreConnectState};
use crate::theme::{Theme, font_size};
use crate::widget_options::{
    DATA_BITS_OPTIONS, DataBitsOption, FLOW_CONTROL_OPTIONS, FlowControlOption, PARITY_OPTIONS,
    ParityOption, RX_CHUNKING_OPTIONS, RxChunkingOption, STOP_BITS_OPTIONS, StopBitsOption,
};

/// Width for port-related inputs
const PORT_INPUT_WIDTH: u32 = 300;
/// Width for config labels
const LABEL_WIDTH: u32 = 100;

/// Render the pre-connect view.
pub fn view(state: &PreConnectState) -> Element<'_, Message> {
    let title = text("Serial Monitor").size(font_size::TITLE);

    // Port selection
    let port_options: Vec<String> = state.ports.iter().map(|p| p.name.clone()).collect();
    let port_picker = pick_list(port_options, state.selected_port.clone(), |port| {
        Message::PreConnect(PreConnectMsg::SelectPort(port))
    })
    .placeholder("Select a port...")
    .width(PORT_INPUT_WIDTH)
    .style(Theme::pick_list);

    let refresh_btn = button(text("Refresh").size(font_size::BODY))
        .on_press(Message::PreConnect(PreConnectMsg::RefreshPorts))
        .style(Theme::button_secondary);

    let port_row = row![port_picker, refresh_btn]
        .spacing(10)
        .align_y(Alignment::Center);

    // Port info
    let port_info = if let Some(ref selected) = state.selected_port {
        if let Some(port) = state.ports.iter().find(|p| &p.name == selected) {
            let info = if let Some(ref product) = port.product {
                product.clone()
            } else if let Some(ref manufacturer) = port.manufacturer {
                manufacturer.clone()
            } else {
                "No description".to_string()
            };
            text(info)
                .size(font_size::SMALL)
                .color(Theme::TEXT_SECONDARY)
        } else {
            text("")
        }
    } else {
        text("")
    };

    // Custom port path input (for PTYs, etc.)
    let custom_port_input = text_input(
        "Or enter custom path (e.g., /dev/pts/5)...",
        &state.custom_port_path,
    )
    .on_input(|path| Message::PreConnect(PreConnectMsg::CustomPortPathChanged(path)))
    .width(PORT_INPUT_WIDTH)
    .style(Theme::text_input);

    // Baud rate selection
    let baud_options: Vec<u32> = COMMON_BAUD_RATES.to_vec();
    let baud_picker = pick_list(baud_options, Some(state.config.baud_rate), |baud| {
        Message::PreConnect(PreConnectMsg::SelectBaudRate(baud))
    })
    .width(120)
    .style(Theme::pick_list);

    // Data bits selection (using static array)
    let data_bits_picker = pick_list(
        DATA_BITS_OPTIONS,
        Some(DataBitsOption(state.config.data_bits)),
        |opt| Message::PreConnect(PreConnectMsg::SelectDataBits(opt.0)),
    )
    .width(80)
    .style(Theme::pick_list);

    // Parity selection (using static array)
    let parity_picker = pick_list(
        PARITY_OPTIONS,
        Some(ParityOption(state.config.parity)),
        |opt| Message::PreConnect(PreConnectMsg::SelectParity(opt.0)),
    )
    .width(80)
    .style(Theme::pick_list);

    // Stop bits selection (using static array)
    let stop_bits_picker = pick_list(
        STOP_BITS_OPTIONS,
        Some(StopBitsOption(state.config.stop_bits)),
        |opt| Message::PreConnect(PreConnectMsg::SelectStopBits(opt.0)),
    )
    .width(80)
    .style(Theme::pick_list);

    // Flow control selection (using static array)
    let flow_control_picker = pick_list(
        FLOW_CONTROL_OPTIONS,
        Some(FlowControlOption(state.config.flow_control)),
        |opt| Message::PreConnect(PreConnectMsg::SelectFlowControl(opt.0)),
    )
    .width(180)
    .style(Theme::pick_list);

    // RX Chunking selection (using static array)
    let rx_chunking_picker = pick_list(
        RX_CHUNKING_OPTIONS,
        Some(RxChunkingOption(state.rx_chunking_index)),
        |opt| Message::PreConnect(PreConnectMsg::SelectRxChunking(opt.0)),
    )
    .width(150)
    .style(Theme::pick_list);

    // Config rows
    let config_rows = column![
        row![
            text("Baud Rate:").size(font_size::BODY).width(LABEL_WIDTH),
            baud_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Data Bits:").size(font_size::BODY).width(LABEL_WIDTH),
            data_bits_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Parity:").size(font_size::BODY).width(LABEL_WIDTH),
            parity_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Stop Bits:").size(font_size::BODY).width(LABEL_WIDTH),
            stop_bits_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Flow Control:")
                .size(font_size::BODY)
                .width(LABEL_WIDTH),
            flow_control_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("RX Chunking:")
                .size(font_size::BODY)
                .width(LABEL_WIDTH),
            rx_chunking_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    // Connect button - enable if either port is selected or custom path is entered
    let can_connect = state.selected_port.is_some() || !state.custom_port_path.is_empty();
    let connect_btn = if state.connecting {
        button(text("Connecting...").size(font_size::BODY)).style(Theme::button_primary)
    } else if can_connect {
        button(text("Connect").size(font_size::BODY))
            .on_press(Message::PreConnect(PreConnectMsg::Connect))
            .style(Theme::button_primary)
    } else {
        button(text("Connect").size(font_size::BODY)).style(Theme::button_primary)
    };

    // Error message
    let error_text = if let Some(ref err) = state.error {
        text(err).color(Theme::STATUS_ERROR)
    } else {
        text("")
    };

    let content = column![
        title,
        Space::new().height(20),
        text("Port:").size(font_size::BODY),
        port_row,
        port_info,
        Space::new().height(5),
        custom_port_input,
        Space::new().height(15),
        config_rows,
        Space::new().height(20),
        connect_btn,
        error_text,
    ]
    .spacing(5)
    .padding(20)
    .width(Length::Shrink);

    container(content)
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .into()
}
