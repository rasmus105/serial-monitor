//! Main application state and logic.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::widget::scrollable;
use iced::{Element, Subscription, Task};
use serial_core::{
    list_ports, ChunkingStrategy, DataBits, Encoding, FlowControl, LineDelimiter, Parity,
    PortInfo, SerialConfig, Session, SessionConfig, SessionEvent, SessionHandle, StopBits,
};

use crate::view::{pre_connect, traffic};

/// Main application state.
pub struct App {
    /// Current session state.
    state: SessionState,
    /// Pending connection result (used to transfer SessionHandle from async task).
    pending_connection: Arc<Mutex<Option<ConnectionResult>>>,
}

/// State of the current session.
pub enum SessionState {
    /// Not connected - showing port selection.
    PreConnect(PreConnectState),
    /// Connected to a serial port.
    Connected(ConnectedState),
}

/// State when not connected.
pub struct PreConnectState {
    /// Available serial ports.
    pub ports: Vec<PortInfo>,
    /// Currently selected port.
    pub selected_port: Option<String>,
    /// Custom port path input (for PTYs, etc).
    pub custom_port_path: String,
    /// Serial configuration.
    pub config: SerialConfig,
    /// RX chunking mode index (0=Raw, 1=LF, 2=CR, 3=CRLF).
    pub rx_chunking_index: usize,
    /// Error message to display.
    pub error: Option<String>,
    /// Whether we're currently connecting.
    pub connecting: bool,
}

impl Default for PreConnectState {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            selected_port: None,
            custom_port_path: String::new(),
            config: SerialConfig::default(),
            rx_chunking_index: 1, // Default to LF (like TUI)
            error: None,
            connecting: false,
        }
    }
}

/// Scroll behavior for the traffic view
#[derive(Debug, Clone)]
pub enum ScrollState {
    /// Locked to bottom - always shows latest data, user cannot scroll up
    LockedToBottom,
    /// Auto-scroll - stays at bottom when new data arrives, but allows scrolling up
    /// When scrolled up, new data doesn't auto-scroll. When scrolled back to bottom, resumes.
    AutoScroll { offset: f32 },
    /// Manual scroll - user has scrolled away from bottom, stays where user left it
    Manual { offset: f32 },
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState::AutoScroll { offset: 0.0 }
    }
}

/// State when connected to a serial port.
pub struct ConnectedState {
    /// Session handle for serial communication.
    pub handle: SessionHandle,
    /// Port name we're connected to.
    pub port_name: String,
    /// Serial configuration.
    pub config: SerialConfig,
    /// Current encoding for display.
    pub encoding: Encoding,
    /// Session start time for relative timestamps.
    pub session_start: std::time::SystemTime,
    /// Send input text.
    pub send_input: String,
    /// Whether to show TX data.
    pub show_tx: bool,
    /// Whether to show RX data.
    pub show_rx: bool,
    /// Line ending to append when sending (index: 0=None, 1=LF, 2=CR, 3=CRLF).
    pub send_line_ending_index: usize,
    /// Whether to show the config panel.
    pub show_config_panel: bool,
    /// Whether to show timestamps.
    pub show_timestamps: bool,
    /// Timestamp format index (0=Relative, 1=HH:MM:SS.mmm, 2=HH:MM:SS).
    pub timestamp_format_index: usize,
    /// Scroll state for virtual scrolling
    pub scroll_state: ScrollState,
    /// Viewport height (set by scroll callback)
    pub viewport_height: Option<f32>,
    /// Collapsed sections in the config panel (by section name).
    pub collapsed_sections: HashSet<String>,
}



/// Get the RX chunking strategy from index.
pub fn rx_chunking_from_index(index: usize) -> ChunkingStrategy {
    match index {
        0 => ChunkingStrategy::Raw,
        1 => ChunkingStrategy::with_delimiter(LineDelimiter::Newline),
        2 => ChunkingStrategy::with_delimiter(LineDelimiter::Cr),
        3 => ChunkingStrategy::with_delimiter(LineDelimiter::CrLf),
        _ => ChunkingStrategy::Raw,
    }
}

/// Application messages.
#[derive(Debug, Clone)]
pub enum Message {
    // Pre-connect messages
    RefreshPorts,
    PortsListed(Vec<PortInfo>),
    SelectPort(String),
    CustomPortPathChanged(String),
    SelectBaudRate(u32),
    SelectDataBits(DataBits),
    SelectParity(Parity),
    SelectStopBits(StopBits),
    SelectFlowControl(FlowControl),
    SelectRxChunking(usize),
    Connect,
    ConnectionComplete,
    ConnectionFailed(String),

    // Connected messages
    Disconnect,
    Disconnected,
    SendInput(String),
    Send,
    SendComplete,
    SelectEncoding(Encoding),
    ToggleShowTx,
    ToggleShowRx,
    SelectSendLineEnding(usize),
    ClearBuffer,
    ToggleConfigPanel,
    ToggleTimestamps,
    SelectTimestampFormat(usize),
    ToggleSectionCollapse(String),
    ScrollChanged(scrollable::Viewport),
    SelectScrollMode(crate::view::traffic::ScrollModeOption),
    Tick,
}

/// Connection result that can be stored and retrieved.
struct ConnectionResult {
    handle: SessionHandle,
    port_name: String,
    config: SerialConfig,
}

impl Default for App {
    fn default() -> Self {
        App::new().0
    }
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            state: SessionState::PreConnect(PreConnectState::default()),
            pending_connection: Arc::new(Mutex::new(None)),
        };
        (app, Task::done(Message::RefreshPorts))
    }

    pub fn title(&self) -> String {
        match &self.state {
            SessionState::PreConnect(_) => "Serial Monitor".to_string(),
            SessionState::Connected(state) => {
                format!("Serial Monitor - {}", state.port_name)
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Pre-connect messages
            Message::RefreshPorts => {
                return Task::perform(async { list_ports().unwrap_or_default() }, Message::PortsListed);
            }
            Message::PortsListed(ports) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    // Auto-select first port if none selected
                    if state.selected_port.is_none() && !ports.is_empty() {
                        state.selected_port = Some(ports[0].name.clone());
                    }
                    state.ports = ports;
                }
            }
            Message::SelectPort(port) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.selected_port = Some(port);
                    state.custom_port_path.clear(); // Clear custom path when selecting from list
                }
            }
            Message::CustomPortPathChanged(path) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.custom_port_path = path;
                    if !state.custom_port_path.is_empty() {
                        state.selected_port = None; // Clear selected when using custom path
                    }
                }
            }
            Message::SelectBaudRate(baud) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.baud_rate = baud;
                }
            }
            Message::SelectDataBits(data_bits) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.data_bits = data_bits;
                }
            }
            Message::SelectParity(parity) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.parity = parity;
                }
            }
            Message::SelectStopBits(stop_bits) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.stop_bits = stop_bits;
                }
            }
            Message::SelectFlowControl(flow_control) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.flow_control = flow_control;
                }
            }
            Message::SelectRxChunking(index) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.rx_chunking_index = index;
                }
            }
            Message::Connect => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    // Determine port to connect to: custom path takes priority
                    let port = if !state.custom_port_path.is_empty() {
                        state.custom_port_path.clone()
                    } else {
                        state.selected_port.clone().unwrap_or_default()
                    };

                    if !port.is_empty() {
                        state.connecting = true;
                        state.error = None;
                        let config = state.config.clone();
                        let rx_chunking = rx_chunking_from_index(state.rx_chunking_index);
                        let pending = Arc::clone(&self.pending_connection);

                        return Task::perform(
                            async move {
                                let session_config = SessionConfig {
                                    rx_chunking,
                                    ..Default::default()
                                };
                                match Session::connect_with_config(&port, config.clone(), session_config).await {
                                    Ok(handle) => {
                                        // Store the result in the shared state
                                        let result = ConnectionResult {
                                            handle,
                                            port_name: port,
                                            config,
                                        };
                                        *pending.lock().unwrap() = Some(result);
                                        Ok(())
                                    }
                                    Err(e) => Err(e.to_string()),
                                }
                            },
                            |result: Result<(), String>| match result {
                                Ok(()) => Message::ConnectionComplete,
                                Err(e) => Message::ConnectionFailed(e),
                            },
                        );
                    }
                }
            }
            Message::ConnectionComplete => {
                // Take the connection result from shared state
                if let Some(result) = self.pending_connection.lock().unwrap().take() {
                    self.state = SessionState::Connected(ConnectedState {
                        handle: result.handle,
                        port_name: result.port_name,
                        config: result.config,
                        encoding: Encoding::Utf8,
                        session_start: std::time::SystemTime::now(),
                        send_input: String::new(),
                        show_tx: true,
                        show_rx: true,
                        send_line_ending_index: 1, // Default to LF
                        show_config_panel: true,
                        show_timestamps: true,
                        timestamp_format_index: 0, // Relative
                        scroll_state: ScrollState::default(),
                        viewport_height: None,
                        collapsed_sections: HashSet::from(["Statistics".to_string()]),
                    });
                }
            }
            Message::ConnectionFailed(error) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.connecting = false;
                    state.error = Some(error);
                }
            }

            // Connected messages
            Message::Disconnect => {
                if let SessionState::Connected(state) = std::mem::replace(
                    &mut self.state,
                    SessionState::PreConnect(PreConnectState::default()),
                ) {
                    return Task::perform(
                        async move {
                            let _ = state.handle.disconnect().await;
                        },
                        |_| Message::Disconnected,
                    );
                }
            }
            Message::Disconnected => {
                // Refresh ports after disconnect
                return Task::perform(async { list_ports().unwrap_or_default() }, Message::PortsListed);
            }
            Message::SendInput(input) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.send_input = input;
                }
            }
            Message::Send => {
                if let SessionState::Connected(state) = &mut self.state
                    && !state.send_input.is_empty()
                {
                    let mut data = state.send_input.clone().into_bytes();
                    state.send_input.clear();

                    // Append line ending based on selection
                    match state.send_line_ending_index {
                        1 => data.push(b'\n'),        // LF
                        2 => data.push(b'\r'),        // CR
                        3 => data.extend_from_slice(b"\r\n"), // CRLF
                        _ => {}                       // None
                    }

                    // Use the session handle's send method
                    let sender = state.handle.clone_command_sender();
                    return Task::perform(
                        async move {
                            let _ = sender.send(serial_core::SessionCommand::Send(data)).await;
                        },
                        |_| Message::SendComplete,
                    );
                }
            }
            Message::SendComplete => {
                // Data was sent, tick will update the display
            }
            Message::SelectEncoding(encoding) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.encoding = encoding;
                    // Update buffer encoding
                    state.handle.buffer_mut().set_encoding(encoding);
                }
            }
            Message::ToggleShowTx => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_tx = !state.show_tx;
                    state.handle.buffer_mut().set_show_tx(state.show_tx);
                }
            }
            Message::ToggleShowRx => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_rx = !state.show_rx;
                    state.handle.buffer_mut().set_show_rx(state.show_rx);
                }
            }
            Message::SelectSendLineEnding(index) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.send_line_ending_index = index;
                }
            }
            Message::ClearBuffer => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.handle.buffer_mut().clear();
                }
            }
            Message::ToggleConfigPanel => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_config_panel = !state.show_config_panel;
                }
            }
            Message::ToggleTimestamps => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_timestamps = !state.show_timestamps;
                }
            }
            Message::SelectTimestampFormat(index) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.timestamp_format_index = index;
                }
            }
            Message::ToggleSectionCollapse(section) => {
                if let SessionState::Connected(state) = &mut self.state {
                    if state.collapsed_sections.contains(&section) {
                        state.collapsed_sections.remove(&section);
                    } else {
                        state.collapsed_sections.insert(section);
                    }
                }
            }
            Message::ScrollChanged(viewport) => {
                if let SessionState::Connected(state) = &mut self.state {
                    // Update viewport height for virtual scrolling calculations
                    state.viewport_height = Some(viewport.bounds().height);
                    
                    let content_height = viewport.content_bounds().height;
                    let viewport_height = viewport.bounds().height;
                    let offset = viewport.absolute_offset().y;
                    let max_scroll = (content_height - viewport_height).max(0.0);
                    
                    // Check if we're at the bottom (with small tolerance for float comparison)
                    let at_bottom = offset >= max_scroll - 1.0;
                    
                    match &state.scroll_state {
                        ScrollState::LockedToBottom => {
                            // Stay locked - this shouldn't normally trigger scroll changes
                        }
                        ScrollState::AutoScroll { .. } => {
                            if at_bottom {
                                // Stay in auto-scroll mode at bottom
                                state.scroll_state = ScrollState::AutoScroll { offset };
                            } else {
                                // User scrolled up - switch to manual mode
                                state.scroll_state = ScrollState::Manual { offset };
                            }
                        }
                        ScrollState::Manual { .. } => {
                            if at_bottom {
                                // User scrolled back to bottom - resume auto-scroll
                                state.scroll_state = ScrollState::AutoScroll { offset };
                            } else {
                                // Update offset
                                state.scroll_state = ScrollState::Manual { offset };
                            }
                        }
                    }
                }
            }
            Message::SelectScrollMode(mode) => {
                if let SessionState::Connected(state) = &mut self.state {
                    use crate::view::traffic::ScrollModeOption;
                    state.scroll_state = match mode {
                        ScrollModeOption::Auto => ScrollState::AutoScroll { offset: 0.0 },
                        ScrollModeOption::Locked => ScrollState::LockedToBottom,
                    };
                }
            }
            Message::Tick => {
                if let SessionState::Connected(state) = &mut self.state {
                    // Poll for session events - no need to copy buffer data,
                    // the view will borrow directly from the buffer
                    while let Some(event) = state.handle.try_recv_event() {
                        match event {
                            SessionEvent::Disconnected { error } => {
                                let msg = error.unwrap_or_else(|| "Disconnected".to_string());
                                self.state = SessionState::PreConnect(PreConnectState {
                                    config: state.config.clone(),
                                    error: Some(msg),
                                    ..Default::default()
                                });
                                return Task::perform(
                                    async { list_ports().unwrap_or_default() },
                                    Message::PortsListed,
                                );
                            }
                            SessionEvent::Error(_e) => {
                                // Errors are now shown via other means (toast, etc.)
                                // No message field needed
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.state {
            SessionState::PreConnect(state) => pre_connect::view(state),
            SessionState::Connected(state) => traffic::view(state),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match &self.state {
            SessionState::PreConnect(_) => Subscription::none(),
            SessionState::Connected(_) => {
                // Poll for updates every 50ms
                iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick)
            }
        }
    }
}
