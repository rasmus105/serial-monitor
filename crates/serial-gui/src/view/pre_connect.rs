//! Pre-connect view for port selection and configuration.

use iced::widget::{button, column, container, pick_list, row, text, text_input, Space};
use iced::{Alignment, Element, Fill, Length};
use serial_core::ui::serial_config::{
    data_bits_display, flow_control_display, parity_display, stop_bits_display,
    COMMON_BAUD_RATES, DATA_BITS_VARIANTS, FLOW_CONTROL_VARIANTS, PARITY_VARIANTS,
    STOP_BITS_VARIANTS,
};
use serial_core::{DataBits, FlowControl, Parity, StopBits};
use std::fmt;

use crate::app::{Message, PreConnectState};
use crate::theme::Theme;

// Wrapper types for pick_list (need Display + PartialEq)
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

// RX Chunking wrapper
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

/// Render the pre-connect view.
pub fn view(state: &PreConnectState) -> Element<'_, Message> {
    let title = text("Serial Monitor").size(28);

    // Port selection
    let port_options: Vec<String> = state.ports.iter().map(|p| p.name.clone()).collect();
    let port_picker = pick_list(
        port_options,
        state.selected_port.clone(),
        Message::SelectPort,
    )
    .placeholder("Select a port...")
    .width(300);

    let refresh_btn = button(text("Refresh")).on_press(Message::RefreshPorts);

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
            text(info).size(14).color(Theme::MUTED)
        } else {
            text("")
        }
    } else {
        text("")
    };

    // Custom port path input (for PTYs, etc.)
    let custom_port_input = text_input("Or enter custom path (e.g., /dev/pts/5)...", &state.custom_port_path)
        .on_input(Message::CustomPortPathChanged)
        .width(300);

    // Baud rate selection
    let baud_options: Vec<u32> = COMMON_BAUD_RATES.to_vec();
    let baud_picker = pick_list(
        baud_options,
        Some(state.config.baud_rate),
        Message::SelectBaudRate,
    )
    .width(120);

    // Data bits selection
    let data_bits_options: Vec<DataBitsOption> =
        DATA_BITS_VARIANTS.iter().copied().map(DataBitsOption).collect();
    let data_bits_picker = pick_list(
        data_bits_options,
        Some(DataBitsOption(state.config.data_bits)),
        |opt| Message::SelectDataBits(opt.0),
    )
    .width(80);

    // Parity selection
    let parity_options: Vec<ParityOption> =
        PARITY_VARIANTS.iter().copied().map(ParityOption).collect();
    let parity_picker = pick_list(
        parity_options,
        Some(ParityOption(state.config.parity)),
        |opt| Message::SelectParity(opt.0),
    )
    .width(80);

    // Stop bits selection
    let stop_bits_options: Vec<StopBitsOption> =
        STOP_BITS_VARIANTS.iter().copied().map(StopBitsOption).collect();
    let stop_bits_picker = pick_list(
        stop_bits_options,
        Some(StopBitsOption(state.config.stop_bits)),
        |opt| Message::SelectStopBits(opt.0),
    )
    .width(80);

    // Flow control selection
    let flow_control_options: Vec<FlowControlOption> = FLOW_CONTROL_VARIANTS
        .iter()
        .copied()
        .map(FlowControlOption)
        .collect();
    let flow_control_picker = pick_list(
        flow_control_options,
        Some(FlowControlOption(state.config.flow_control)),
        |opt| Message::SelectFlowControl(opt.0),
    )
    .width(180);

    // RX Chunking selection
    let rx_chunking_options: Vec<RxChunkingOption> = (0..4).map(RxChunkingOption).collect();
    let rx_chunking_picker = pick_list(
        rx_chunking_options,
        Some(RxChunkingOption(state.rx_chunking_index)),
        |opt| Message::SelectRxChunking(opt.0),
    )
    .width(150);

    // Config rows
    let label_width = 100;
    let config_rows = column![
        row![
            text("Baud Rate:").width(label_width),
            baud_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Data Bits:").width(label_width),
            data_bits_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Parity:").width(label_width),
            parity_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Stop Bits:").width(label_width),
            stop_bits_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Flow Control:").width(label_width),
            flow_control_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("RX Chunking:").width(label_width),
            rx_chunking_picker,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    // Connect button - enable if either port is selected or custom path is entered
    let can_connect = state.selected_port.is_some() || !state.custom_port_path.is_empty();
    let connect_btn = if state.connecting {
        button(text("Connecting..."))
    } else if can_connect {
        button(text("Connect")).on_press(Message::Connect)
    } else {
        button(text("Connect"))
    };

    // Error message
    let error_text = if let Some(ref err) = state.error {
        text(err).color(Theme::ERROR)
    } else {
        text("")
    };

    let content = column![
        title,
        Space::new().height(20),
        text("Port:"),
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
