//! Application state and logic

use std::time::Duration;

use serial_core::{
    send_file, start_file_saver, DataChunk, FileSaveConfig, FileSaverHandle, FileSendConfig,
    SearchEngine, Session, SessionEvent,
};

use crate::settings::{Settings, SettingsPanelState};

// Submodules
mod handlers;
pub mod state;
pub mod types;

// Re-export all public types
pub use serial_core::SearchMatch;
pub use state::{
    FileSendState, GraphState, GraphTimeWindow, InputState, PortSelectState, SendState,
    TabLayout, TabState, TextInputResult, TrafficState, TAB_COUNT,
};
pub use types::{
    ChunkingMode, ConfigField, ConfigFieldKind, ConfigOption, ConfigPanelState, ConfigSection,
    ConnectionState, DelimiterOption, EnumNavigation, FileSaveSettings, GraphConfigField,
    GraphFocus, HexGrouping, InputMode, InputEncodingMode, LineEndingOption, LocalStrumEnum,
    PaneContent, PaneFocus, PortSelectFocus, SendConfig, SendConfigField, SendFocus, SizeUnit,
    TimestampFormat, TrafficConfigField, TrafficFocus, View, WrapMode,
};

// =============================================================================
// App Struct - Main Application State
// =============================================================================

/// Main application state
pub struct App {
    /// Should the application quit?
    pub should_quit: bool,
    /// Should the terminal be fully cleared on next render?
    pub needs_full_clear: bool,
    /// Current view
    pub view: View,
    /// Connection state
    pub connection: ConnectionState,
    /// Status message
    pub status: String,

    /// Input state
    pub input: InputState,
    /// Port selection state
    pub port_select: PortSelectState,
    /// Tab layout for connected view (with persistent splits per tab)
    pub layout: TabLayout,
    /// Traffic view state
    pub traffic: TrafficState,
    /// Graph view state
    pub graph: GraphState,
    /// Send view state
    pub send: SendState,
    /// Search engine
    pub search: SearchEngine,
    /// File send state
    pub file_send: FileSendState,
    /// Application settings (including keybindings)
    pub settings: Settings,
    /// Settings panel state
    pub settings_panel: SettingsPanelState,
    /// File saver handle (if saving is active)
    pub file_saver: Option<FileSaverHandle>,

    /// Tokio runtime handle for async operations
    runtime: tokio::runtime::Handle,
}

impl App {
    /// Create a new application
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        let mut port_select = PortSelectState::default();
        let _ = port_select.refresh_ports();
        let status = if port_select.ports.is_empty() {
            "No serial ports found. Press ':' to enter path manually, 'r' to refresh.".to_string()
        } else {
            format!(
                "Found {} port(s). Select and press Enter, or ':' to enter path manually.",
                port_select.ports.len()
            )
        };

        Self {
            should_quit: false,
            needs_full_clear: false,
            view: View::PortSelect,
            connection: ConnectionState::Disconnected,
            status,
            input: InputState::default(),
            port_select,
            layout: TabLayout::new(),
            traffic: TrafficState::default(),
            graph: GraphState::default(),
            send: SendState::default(),
            search: SearchEngine::new(),
            file_send: FileSendState::default(),
            settings: Settings::default(),
            settings_panel: SettingsPanelState::default(),
            file_saver: None,
            runtime,
        }
    }

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) {
        self.status = self.port_select.refresh_ports();
    }

    // =========================================================================
    // Connection Methods
    // =========================================================================

    pub(crate) fn connect_to_selected_port(&mut self) {
        if let Some(port) = self.port_select.ports.get(self.port_select.selected_port) {
            let port_name = port.name.clone();
            self.connect_to_port(&port_name);
        }
    }

    pub(crate) fn connect_to_port(&mut self, port_name: &str) {
        let serial_config = self.port_select.serial_config.clone();
        let session_config = self.port_select.build_session_config();

        self.status = format!("Connecting to {}...", port_name);

        match self
            .runtime
            .block_on(Session::connect_with_config(port_name, serial_config, session_config))
        {
            Ok(handle) => {
                self.connection = ConnectionState::Connected(handle);
                self.view = View::Connected;
                self.traffic.scroll_offset = 0;
                self.traffic.session_start = Some(std::time::SystemTime::now());

                // Copy pre-connection file save settings to traffic state
                self.traffic.file_save = self.port_select.file_save.clone();

                // Start file saving if enabled in pre-connection settings
                if self.traffic.file_save.enabled {
                    self.start_file_saving();
                }

                self.status = format!(
                    "Connected to {} @ {} baud",
                    port_name, self.port_select.serial_config.baud_rate
                );
            }
            Err(e) => {
                self.status = format!("Failed to connect: {}", e);
            }
        }
    }

    pub(crate) fn disconnect(&mut self) {
        // Stop file saving before disconnecting
        self.stop_file_saving();

        if let ConnectionState::Connected(handle) =
            std::mem::replace(&mut self.connection, ConnectionState::Disconnected)
        {
            let _ = self.runtime.block_on(handle.disconnect());
        }
    }

    // =========================================================================
    // Data Methods
    // =========================================================================

    pub(crate) fn send_data(&mut self, data: Vec<u8>) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let len = data.len();
            match self.runtime.block_on(handle.send(data)) {
                Ok(()) => {
                    self.status = format!("Sent {} bytes", len);
                }
                Err(e) => {
                    self.status = format!("Send failed: {}", e);
                }
            }
        }
    }

    // =========================================================================
    // File Send Methods
    // =========================================================================

    pub(crate) fn start_file_send(&mut self, path: &str) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let config = FileSendConfig::default()
                .with_chunk_size(self.send.chunk_size_bytes())
                .with_delay(self.send.chunk_delay_duration())
                .with_continuous(self.send.config.continuous);

            match self.runtime.block_on(send_file(handle, path, config)) {
                Ok(file_handle) => {
                    self.file_send.handle = Some(file_handle);
                    self.file_send.progress = None;
                    self.status = format!("Sending file: {}", path);
                }
                Err(e) => {
                    self.status = format!("Failed to send file: {}", e);
                }
            }
        } else {
            self.status = "Not connected.".to_string();
        }
    }

    pub(crate) fn cancel_file_send(&mut self) {
        if let Some(ref handle) = self.file_send.handle {
            self.runtime.block_on(handle.cancel());
        }
        self.file_send.handle = None;
        self.file_send.progress = None;
        self.status = "File send cancelled.".to_string();
    }

    /// Poll for file send progress
    pub fn poll_file_send(&mut self) {
        if let Some(ref mut handle) = self.file_send.handle {
            while let Some(progress) = handle.try_recv_progress() {
                let complete = progress.complete;
                let error = progress.error.clone();
                self.file_send.progress = Some(progress);

                if complete {
                    if let Some(err) = error {
                        self.status = format!("File send failed: {}", err);
                    } else {
                        self.status = "File send complete.".to_string();
                    }
                    self.file_send.handle = None;
                    break;
                }
            }
        }
    }

    // =========================================================================
    // File Saving Methods
    // =========================================================================

    /// Start file saving with current configuration
    pub(crate) fn start_file_saving(&mut self) {
        // Stop any existing file saver first
        self.stop_file_saving();

        // Get port name for auto-generated filename
        let port_name = if let ConnectionState::Connected(ref handle) = self.connection {
            handle.port_name().to_string()
        } else {
            "unknown".to_string()
        };

        // Build config
        let mut config =
            FileSaveConfig::new(self.traffic.file_save.directory.clone(), &port_name)
                .with_format(self.traffic.file_save.format);

        // Set custom filename if provided
        if !self.traffic.file_save.filename.is_empty() {
            config = config.with_filename(&self.traffic.file_save.filename);
        }

        // Start the file saver (spawns async task on the provided runtime)
        match start_file_saver(config, &self.runtime) {
            Ok(handle) => {
                let path = handle.file_path().display().to_string();
                self.file_saver = Some(handle);
                self.status = format!("Saving to: {}", path);
            }
            Err(e) => {
                self.status = format!("Failed to start file saving: {}", e);
                self.traffic.file_save.enabled = false;
            }
        }
    }

    /// Stop file saving
    pub(crate) fn stop_file_saving(&mut self) {
        if let Some(handle) = self.file_saver.take() {
            let _ = handle.stop();
            self.status = "File saving stopped.".to_string();
        }
    }

    /// Send data chunk to file saver (if active)
    fn write_to_file_saver(&self, chunk: &DataChunk) {
        if let Some(ref handle) = self.file_saver {
            let _ = handle.write(chunk.clone());
        }
    }

    // =========================================================================
    // Session Event Polling
    // =========================================================================

    /// Poll for session events (non-blocking)
    pub fn poll_session_events(&mut self) {
        // Collect events first to avoid borrow checker issues
        let events: Vec<SessionEvent> =
            if let ConnectionState::Connected(ref mut handle) = self.connection {
                let mut events = Vec::new();
                while let Some(event) = handle.try_recv_event() {
                    events.push(event);
                }
                events
            } else {
                Vec::new()
            };

        // Now process the events
        for event in events {
            match event {
                SessionEvent::Disconnected { error } => {
                    // Stop file saving on disconnect
                    self.stop_file_saving();

                    self.status = match error {
                        Some(e) => format!("Disconnected: {}", e),
                        None => "Disconnected.".to_string(),
                    };
                    self.connection = ConnectionState::Disconnected;
                    self.view = View::PortSelect;
                    self.needs_full_clear = true;
                    break;
                }
                SessionEvent::Error(e) => {
                    self.status = format!("Error: {}", e);
                }
                SessionEvent::DataReceived(chunk) => {
                    // Write received data to file saver
                    self.write_to_file_saver(&chunk);
                    // Process chunk for graph (if engine is active)
                    if self.graph.has_engine() {
                        self.graph.engine_mut().process_chunk(&chunk);
                    }
                }
                SessionEvent::DataSent(chunk) => {
                    // Write sent data to file saver
                    self.write_to_file_saver(&chunk);
                    // Process chunk for graph (if engine is active)
                    if self.graph.has_engine() {
                        self.graph.engine_mut().process_chunk(&chunk);
                    }
                }
                SessionEvent::Connected => {}
            }
        }
    }

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Get the tick rate for the event loop
    pub fn tick_rate(&self) -> Duration {
        Duration::from_millis(50)
    }

    /// Get page size for Ctrl-d/u scrolling (half screen)
    pub(crate) fn page_size(&self) -> usize {
        15
    }

    /// Initialize the graph engine with historical data (lazy initialization)
    pub fn initialize_graph(&mut self) {
        if self.graph.initialized {
            return;
        }

        if let ConnectionState::Connected(ref handle) = self.connection {
            let buffer = handle.buffer();
            self.graph.engine_mut().initialize(buffer.chunks());
            self.graph.initialized = true;
        }
    }
}
