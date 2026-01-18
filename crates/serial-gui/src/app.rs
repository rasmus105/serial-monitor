//! Main application state and logic.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use iced::widget::scrollable;
use iced::{Element, Subscription, Task, event, keyboard};
use serial_core::{
    ChunkingStrategy, DataBits, Direction, Encoding, FlowControl, LineDelimiter, Parity, PortInfo,
    SerialConfig, Session, SessionConfig, SessionEvent, SessionHandle, StopBits, list_ports,
};

use crate::view::{pre_connect, traffic};
use crate::widget_options::ScrollModeOption;

// =============================================================================
// Constants
// =============================================================================

/// Tick interval for polling session events (in milliseconds)
const TICK_INTERVAL_MS: u64 = 50;

/// Tolerance for scroll position comparison
const SCROLL_BOTTOM_TOLERANCE: f32 = 1.0;

// =============================================================================
// Application state
// =============================================================================

/// Minimum zoom level (50%)
const MIN_ZOOM: f32 = 0.5;
/// Maximum zoom level (200%)
const MAX_ZOOM: f32 = 2.0;
/// Zoom step per key press
const ZOOM_STEP: f32 = 0.1;

/// Main application state.
pub struct App {
    /// Current session state.
    state: SessionState,
    /// Pending connection result (used to transfer SessionHandle from async task).
    pending_connection: Arc<Mutex<Option<ConnectionResult>>>,
    /// Current zoom level (1.0 = 100%)
    pub zoom_level: f32,
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
    /// Off - no automatic scrolling whatsoever, just manual scroll
    Off { offset: f32 },
    /// Locked to bottom - always shows latest data, user cannot scroll up
    LockedToBottom,
    /// Auto-scroll - stays at bottom when new data arrives, but allows scrolling up.
    /// When scrolled up, new data doesn't auto-scroll. When scrolled back to bottom, resumes.
    AutoScroll { offset: f32 },
    /// Manual scroll - user has scrolled away from bottom in AutoScroll mode
    Manual { offset: f32 },
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState::AutoScroll { offset: 0.0 }
    }
}

/// Cached visible chunks to avoid cloning encoded strings on every scroll event.
///
/// Hex/binary encodings produce strings 3-9x longer than UTF-8, making per-scroll
/// cloning expensive. This cache stores the cloned data and is only rebuilt when
/// the visible range, buffer content, or encoding changes.
pub struct VisibleChunkCache {
    /// Cloned data for visible chunks: (direction, encoded_string, timestamp)
    pub chunks: Vec<(Direction, String, SystemTime)>,
    /// Start index in buffer (visible index, respecting filters)
    pub start_index: usize,
    /// End index in buffer (exclusive)
    pub end_index: usize,
    /// Buffer length when cache was built (to detect new data)
    pub buffer_len: usize,
    /// Encoding when cache was built
    pub encoding: Encoding,
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
    /// Cached visible chunks to avoid per-scroll cloning (uses RefCell for interior mutability in view).
    pub visible_cache: RefCell<Option<VisibleChunkCache>>,
}

// =============================================================================
// Messages
// =============================================================================

/// Application messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// Messages for pre-connect state
    PreConnect(PreConnectMsg),
    /// Messages for connected state
    Connected(ConnectedMsg),
    /// Periodic tick for polling events
    Tick,
    /// Zoom in (increase font size)
    ZoomIn,
    /// Zoom out (decrease font size)
    ZoomOut,
}

/// Messages for pre-connect state
#[derive(Debug, Clone)]
pub enum PreConnectMsg {
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
}

/// Messages for connected state
#[derive(Debug, Clone)]
pub enum ConnectedMsg {
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
    SelectScrollMode(ScrollModeOption),
}

// Convenience constructor for views
impl Message {
    pub fn refresh_ports() -> Self {
        Self::PreConnect(PreConnectMsg::RefreshPorts)
    }
}

// =============================================================================
// Helpers
// =============================================================================

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

/// Connection result that can be stored and retrieved.
struct ConnectionResult {
    handle: SessionHandle,
    port_name: String,
    config: SerialConfig,
}

// =============================================================================
// App implementation
// =============================================================================

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
            zoom_level: 1.0,
        };
        (app, Task::done(Message::refresh_ports()))
    }

    pub fn title(&self) -> String {
        match &self.state {
            SessionState::PreConnect(_) => "Serial Monitor".to_string(),
            SessionState::Connected(state) => {
                format!("Serial Monitor - {}", state.port_name)
            }
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.zoom_level
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PreConnect(msg) => self.handle_pre_connect(msg),
            Message::Connected(msg) => self.handle_connected(msg),
            Message::Tick => self.handle_tick(),
            Message::ZoomIn => {
                self.zoom_level = (self.zoom_level + ZOOM_STEP).min(MAX_ZOOM);
                // Invalidate visible cache when zoom changes
                if let SessionState::Connected(state) = &mut self.state {
                    *state.visible_cache.get_mut() = None;
                }
                Task::none()
            }
            Message::ZoomOut => {
                self.zoom_level = (self.zoom_level - ZOOM_STEP).max(MIN_ZOOM);
                // Invalidate visible cache when zoom changes
                if let SessionState::Connected(state) = &mut self.state {
                    *state.visible_cache.get_mut() = None;
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.state {
            SessionState::PreConnect(state) => pre_connect::view(state),
            SessionState::Connected(state) => traffic::view(state),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Listen to all keyboard events (including captured ones) for zoom
        let keyboard_sub = event::listen_with(|event, _status, _window| {
            if let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            {
                // Only allow zoom with no modifiers (except shift for '+')
                // Reject if ctrl, alt, or logo/super are pressed
                if modifiers.control() || modifiers.alt() || modifiers.logo() {
                    return None;
                }

                match key {
                    // '+' is shift+= on most keyboards - iced reports "=" with SHIFT modifier
                    keyboard::Key::Character(ref c) if c == "=" && modifiers.shift() => {
                        return Some(Message::ZoomIn);
                    }
                    // '-' should have no modifiers at all
                    keyboard::Key::Character(ref c) if c == "-" && !modifiers.shift() => {
                        return Some(Message::ZoomOut);
                    }
                    _ => {}
                }
            }
            None
        });

        match &self.state {
            SessionState::PreConnect(_) => keyboard_sub,
            SessionState::Connected(_) => {
                let tick = iced::time::every(Duration::from_millis(TICK_INTERVAL_MS))
                    .map(|_| Message::Tick);
                Subscription::batch([keyboard_sub, tick])
            }
        }
    }

    // =========================================================================
    // Pre-connect message handlers
    // =========================================================================

    fn handle_pre_connect(&mut self, msg: PreConnectMsg) -> Task<Message> {
        use PreConnectMsg::*;
        match msg {
            RefreshPorts => Task::perform(async { list_ports().unwrap_or_default() }, |ports| {
                Message::PreConnect(PortsListed(ports))
            }),
            PortsListed(ports) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    if state.selected_port.is_none() && !ports.is_empty() {
                        state.selected_port = Some(ports[0].name.clone());
                    }
                    state.ports = ports;
                }
                Task::none()
            }
            SelectPort(port) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.selected_port = Some(port);
                    state.custom_port_path.clear();
                }
                Task::none()
            }
            CustomPortPathChanged(path) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    if !path.is_empty() {
                        state.selected_port = None;
                    }
                    state.custom_port_path = path;
                }
                Task::none()
            }
            SelectBaudRate(baud) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.baud_rate = baud;
                }
                Task::none()
            }
            SelectDataBits(data_bits) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.data_bits = data_bits;
                }
                Task::none()
            }
            SelectParity(parity) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.parity = parity;
                }
                Task::none()
            }
            SelectStopBits(stop_bits) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.stop_bits = stop_bits;
                }
                Task::none()
            }
            SelectFlowControl(flow_control) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.config.flow_control = flow_control;
                }
                Task::none()
            }
            SelectRxChunking(index) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.rx_chunking_index = index;
                }
                Task::none()
            }
            Connect => self.handle_connect(),
            ConnectionComplete => {
                if let Some(result) = self
                    .pending_connection
                    .lock()
                    .expect("mutex poisoned")
                    .take()
                {
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
                        visible_cache: RefCell::new(None),
                    });
                }
                Task::none()
            }
            ConnectionFailed(error) => {
                if let SessionState::PreConnect(state) = &mut self.state {
                    state.connecting = false;
                    state.error = Some(error);
                }
                Task::none()
            }
        }
    }

    fn handle_connect(&mut self) -> Task<Message> {
        let SessionState::PreConnect(state) = &mut self.state else {
            return Task::none();
        };

        let port = if !state.custom_port_path.is_empty() {
            state.custom_port_path.clone()
        } else {
            state.selected_port.clone().unwrap_or_default()
        };

        if port.is_empty() {
            return Task::none();
        }

        state.connecting = true;
        state.error = None;
        let config = state.config.clone();
        let rx_chunking = rx_chunking_from_index(state.rx_chunking_index);
        let pending = Arc::clone(&self.pending_connection);

        Task::perform(
            async move {
                let session_config = SessionConfig {
                    rx_chunking,
                    ..Default::default()
                };
                match Session::connect_with_config(&port, config.clone(), session_config).await {
                    Ok(handle) => {
                        let result = ConnectionResult {
                            handle,
                            port_name: port,
                            config,
                        };
                        *pending.lock().expect("mutex poisoned") = Some(result);
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            },
            |result: Result<(), String>| match result {
                Ok(()) => Message::PreConnect(PreConnectMsg::ConnectionComplete),
                Err(e) => Message::PreConnect(PreConnectMsg::ConnectionFailed(e)),
            },
        )
    }

    // =========================================================================
    // Connected message handlers
    // =========================================================================

    fn handle_connected(&mut self, msg: ConnectedMsg) -> Task<Message> {
        use ConnectedMsg::*;
        match msg {
            Disconnect => self.handle_disconnect(),
            Disconnected => Task::perform(async { list_ports().unwrap_or_default() }, |ports| {
                Message::PreConnect(PreConnectMsg::PortsListed(ports))
            }),
            SendInput(input) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.send_input = input;
                }
                Task::none()
            }
            Send => self.handle_send(),
            SendComplete => Task::none(),
            SelectEncoding(encoding) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.encoding = encoding;
                    state.handle.buffer_mut().set_encoding(encoding);
                    *state.visible_cache.get_mut() = None; // Invalidate cache
                }
                Task::none()
            }
            ToggleShowTx => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_tx = !state.show_tx;
                    state.handle.buffer_mut().set_show_tx(state.show_tx);
                    *state.visible_cache.get_mut() = None; // Invalidate cache
                }
                Task::none()
            }
            ToggleShowRx => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_rx = !state.show_rx;
                    state.handle.buffer_mut().set_show_rx(state.show_rx);
                    *state.visible_cache.get_mut() = None; // Invalidate cache
                }
                Task::none()
            }
            SelectSendLineEnding(index) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.send_line_ending_index = index;
                }
                Task::none()
            }
            ClearBuffer => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.handle.buffer_mut().clear();
                    *state.visible_cache.get_mut() = None; // Invalidate cache
                }
                Task::none()
            }
            ToggleConfigPanel => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_config_panel = !state.show_config_panel;
                }
                Task::none()
            }
            ToggleTimestamps => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.show_timestamps = !state.show_timestamps;
                }
                Task::none()
            }
            SelectTimestampFormat(index) => {
                if let SessionState::Connected(state) = &mut self.state {
                    state.timestamp_format_index = index;
                }
                Task::none()
            }
            ToggleSectionCollapse(section) => {
                if let SessionState::Connected(state) = &mut self.state {
                    if state.collapsed_sections.contains(&section) {
                        state.collapsed_sections.remove(&section);
                    } else {
                        state.collapsed_sections.insert(section);
                    }
                }
                Task::none()
            }
            ScrollChanged(viewport) => {
                self.handle_scroll_changed(viewport);
                Task::none()
            }
            SelectScrollMode(mode) => {
                if let SessionState::Connected(state) = &mut self.state {
                    // Preserve current offset when switching modes
                    let current_offset = match &state.scroll_state {
                        ScrollState::Off { offset }
                        | ScrollState::AutoScroll { offset }
                        | ScrollState::Manual { offset } => *offset,
                        ScrollState::LockedToBottom => 0.0,
                    };
                    state.scroll_state = match mode {
                        ScrollModeOption::Off => ScrollState::Off {
                            offset: current_offset,
                        },
                        ScrollModeOption::Auto => ScrollState::AutoScroll {
                            offset: current_offset,
                        },
                        ScrollModeOption::Locked => ScrollState::LockedToBottom,
                    };
                }
                Task::none()
            }
        }
    }

    fn handle_disconnect(&mut self) -> Task<Message> {
        if let SessionState::Connected(state) = std::mem::replace(
            &mut self.state,
            SessionState::PreConnect(PreConnectState::default()),
        ) {
            return Task::perform(
                async move {
                    let _ = state.handle.disconnect().await;
                },
                |_| Message::Connected(ConnectedMsg::Disconnected),
            );
        }
        Task::none()
    }

    fn handle_send(&mut self) -> Task<Message> {
        let SessionState::Connected(state) = &mut self.state else {
            return Task::none();
        };

        if state.send_input.is_empty() {
            return Task::none();
        }

        let mut data = state.send_input.clone().into_bytes();
        state.send_input.clear();

        // Append line ending based on selection
        match state.send_line_ending_index {
            1 => data.push(b'\n'),
            2 => data.push(b'\r'),
            3 => data.extend_from_slice(b"\r\n"),
            _ => {}
        }

        let sender = state.handle.clone_command_sender();
        Task::perform(
            async move {
                let _ = sender.send(serial_core::SessionCommand::Send(data)).await;
            },
            |_| Message::Connected(ConnectedMsg::SendComplete),
        )
    }

    fn handle_scroll_changed(&mut self, viewport: scrollable::Viewport) {
        let SessionState::Connected(state) = &mut self.state else {
            return;
        };

        state.viewport_height = Some(viewport.bounds().height);

        let content_height = viewport.content_bounds().height;
        let viewport_height = viewport.bounds().height;
        let offset = viewport.absolute_offset().y;
        let max_scroll = (content_height - viewport_height).max(0.0);
        let at_bottom = offset >= max_scroll - SCROLL_BOTTOM_TOLERANCE;

        match &state.scroll_state {
            ScrollState::Off { .. } => {
                // Just update the offset, no automatic behavior
                state.scroll_state = ScrollState::Off { offset };
            }
            ScrollState::LockedToBottom => {
                // Stay locked
            }
            ScrollState::AutoScroll { .. } => {
                state.scroll_state = if at_bottom {
                    ScrollState::AutoScroll { offset }
                } else {
                    ScrollState::Manual { offset }
                };
            }
            ScrollState::Manual { .. } => {
                state.scroll_state = if at_bottom {
                    ScrollState::AutoScroll { offset }
                } else {
                    ScrollState::Manual { offset }
                };
            }
        }
    }

    // =========================================================================
    // Tick handler
    // =========================================================================

    fn handle_tick(&mut self) -> Task<Message> {
        let SessionState::Connected(state) = &mut self.state else {
            return Task::none();
        };

        while let Some(event) = state.handle.try_recv_event() {
            match event {
                SessionEvent::Disconnected { error } => {
                    let msg = error.unwrap_or_else(|| "Disconnected".to_string());
                    self.state = SessionState::PreConnect(PreConnectState {
                        config: state.config.clone(),
                        error: Some(msg),
                        ..Default::default()
                    });
                    return Task::perform(async { list_ports().unwrap_or_default() }, |ports| {
                        Message::PreConnect(PreConnectMsg::PortsListed(ports))
                    });
                }
                SessionEvent::Error(_e) => {
                    // Errors are shown via other means (toast, etc.)
                }
                _ => {}
            }
        }
        Task::none()
    }
}
