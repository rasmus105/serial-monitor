use log::{debug, error, info, warn};
use std::{io, thread::sleep, time::Duration};

use crate::app_state::{self, MessageToUi};
use serialport;

fn attempt_to_connect(
    config: &SerialConfig,
    path: String,
) -> serialport::Result<Box<dyn serialport::SerialPort>> {
    let mut port_config = serialport::new(path, config.baud_rate)
        .data_bits(config.data_bits)
        .flow_control(config.flow_control)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(config.timeout);

    if let Some(dtr) = config.dtr_on_open {
        port_config = port_config.dtr_on_open(dtr);
    } else {
        port_config = port_config.preserve_dtr_on_open();
    }

    port_config.open()
}

fn handle_rx(state: &mut SerialState, message: app_state::MessageToSerial) {
    match message {
        app_state::MessageToSerial::Connect(device) => {
            if let ConnectionStatus::Connected(_) = state.status {
                error!("Invalid message to serial! (can not connect when already connected)");
                return;
            }

            match attempt_to_connect(&state.config, device) {
                Ok(port) => {
                    state.status = ConnectionStatus::Connected(port);
                    // go to connection loop
                    connected_loop(state);
                }
                Err(e) => {
                    // send error to the UI so it can be displayed to the user.
                    if let Err(send_err) = state
                        .to_ui_tx
                        .send(app_state::MessageToUi::ConnectionError(e.to_string()))
                    {
                        error!("Failed to send connection error to UI: {}", send_err);
                    }
                }
            }
        }
        app_state::MessageToSerial::Disconnect => {
            if let ConnectionStatus::NotConnected = state.status {
                error!("Invalid message to serial! (can not disconnect when not connected)");
                return;
            }

            state.status = ConnectionStatus::NotConnected;
            info!("Serial port disconnected");
        }
    }
}

fn connected_loop(state: &mut SerialState) {
    let mut utf8_buffer = String::new();

    loop {
        // Check if we should disconnect
        if let ConnectionStatus::NotConnected = state.status {
            debug!("Connection lost, exiting connected loop");
            break;
        }

        // 1. Read serial traffic and send to UI
        if let ConnectionStatus::Connected(ref mut port) = state.status {
            let mut buf = vec![0u8; 1024];
            match port.read(&mut buf) {
                Ok(bytes_read) => {
                    if bytes_read > 0 {
                        buf.truncate(bytes_read);

                        if state.settings.parse_utf8 {
                            // Accumulate UTF-8 data to handle partial sequences
                            let chunk = String::from_utf8_lossy(&buf);
                            utf8_buffer.push_str(&chunk);

                            // Split by lines and keep the last incomplete line
                            let lines: Vec<&str> = utf8_buffer.split('\n').collect();
                            if lines.len() > 1 {
                                let complete_lines: Vec<String> = lines[..lines.len() - 1]
                                    .iter()
                                    .map(|s| s.to_string())
                                    .collect();

                                if let Err(e) = state
                                    .to_ui_tx
                                    .send(MessageToUi::UTF8Traffic(complete_lines))
                                {
                                    error!("Failed to send UTF8 traffic to UI: {}", e);
                                }

                                // Keep the last incomplete line
                                utf8_buffer = lines[lines.len() - 1].to_string();
                            }
                        }

                        if state.settings.parse_raw {
                            if let Err(e) = state.to_ui_tx.send(MessageToUi::RawTraffic(buf)) {
                                error!("Failed to send raw traffic to UI: {}", e);
                            }
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    // Timeout is expected, continue
                }
                Err(e) => {
                    error!("Serial read error: {}", e);
                    if let Err(send_err) = state
                        .to_ui_tx
                        .send(MessageToUi::ConnectionError(e.to_string()))
                    {
                        error!("Failed to send read error to UI: {}", send_err);
                    }
                    state.status = ConnectionStatus::NotConnected;
                    break;
                }
            }
        }

        // 2. Check messages (may need to disconnect or take some other action)
        while let Ok(message) = state.rx.try_recv() {
            handle_rx(state, message);
            if let ConnectionStatus::NotConnected = state.status {
                break;
            }
        }

        // Small sleep to prevent busy-waiting
        sleep(Duration::from_millis(10));
    }
}

// copy of `serialport::SerialPortBuilder` except for path
struct SerialConfig {
    /// The baud rate in symbols-per-second
    baud_rate: u32,
    /// Number of bits used to represent a character sent on the line
    data_bits: serialport::DataBits,
    /// The type of signalling to use for controlling data transfer
    flow_control: serialport::FlowControl,
    /// The type of parity to use for error checking
    parity: serialport::Parity,
    /// Number of bits to use to signal the end of a character
    stop_bits: serialport::StopBits,
    /// Amount of time to wait to receive data before timing out
    timeout: core::time::Duration,
    /// The state to set DTR to when opening the device
    dtr_on_open: Option<bool>,
}

struct SerialSettings {
    parse_utf8: bool,
    parse_raw: bool,
}

enum ConnectionStatus {
    NotConnected,
    Connected(Box<dyn serialport::SerialPort>),
}

struct SerialState {
    status: ConnectionStatus,
    settings: SerialSettings,
    config: SerialConfig,
    to_ui_tx: std::sync::mpsc::Sender<app_state::MessageToUi>,
    rx: std::sync::mpsc::Receiver<app_state::MessageToSerial>,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 115200,
            data_bits: serialport::DataBits::Eight,
            flow_control: serialport::FlowControl::None,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
            timeout: core::time::Duration::from_millis(100),
            dtr_on_open: None,
        }
    }
}

pub fn serial_thread(
    rx: std::sync::mpsc::Receiver<app_state::MessageToSerial>,
    to_ui_tx: std::sync::mpsc::Sender<app_state::MessageToUi>,
) {
    let mut state = SerialState {
        status: ConnectionStatus::NotConnected,
        config: SerialConfig::default(),
        settings: SerialSettings {
            parse_utf8: true,
            parse_raw: false,
        },
        to_ui_tx,
        rx,
    };

    loop {
        // 1. Update available devices
        if let Ok(ports) = serialport::available_ports() {
            let port_names: Vec<String> = ports.iter().map(|p| p.port_name.clone()).collect();

            if let Err(e) = state
                .to_ui_tx
                .send(app_state::MessageToUi::AvailableDevices(port_names))
            {
                error!("Failed to send available devices to UI: {}", e);
            }
        }

        // 2. Check for received messages
        while let Ok(message) = state.rx.try_recv() {
            handle_rx(&mut state, message);
        }

        sleep(Duration::from_millis(100));
    }
}
