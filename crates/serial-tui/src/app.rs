//! Application state and logic

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use serial_core::{list_ports, PortInfo, SerialConfig, Session, SessionEvent, SessionHandle};

/// Current view/screen
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Port selection screen
    PortSelect,
    /// Traffic view (main view)
    Traffic,
}

/// Connection state
#[derive(Debug)]
pub enum ConnectionState {
    Disconnected,
    Connected(SessionHandle),
}

/// Application state
pub struct App {
    /// Should the application quit?
    pub should_quit: bool,
    /// Current view
    pub view: View,
    /// Available serial ports
    pub ports: Vec<PortInfo>,
    /// Selected port index
    pub selected_port: usize,
    /// Connection state
    pub connection: ConnectionState,
    /// Status message
    pub status: String,
    /// Scroll offset for traffic view
    pub scroll_offset: usize,
    /// Tokio runtime handle for async operations
    runtime: tokio::runtime::Handle,
}

impl App {
    /// Create a new application
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        let ports = list_ports().unwrap_or_default();
        let status = if ports.is_empty() {
            "No serial ports found. Press 'r' to refresh.".to_string()
        } else {
            format!("Found {} port(s). Select and press Enter to connect.", ports.len())
        };

        Self {
            should_quit: false,
            view: View::PortSelect,
            ports,
            selected_port: 0,
            connection: ConnectionState::Disconnected,
            status,
            scroll_offset: 0,
            runtime,
        }
    }

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) {
        self.ports = list_ports().unwrap_or_default();
        self.selected_port = 0;
        self.status = if self.ports.is_empty() {
            "No serial ports found.".to_string()
        } else {
            format!("Found {} port(s).", self.ports.len())
        };
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.view {
            View::PortSelect => self.handle_key_port_select(key),
            View::Traffic => self.handle_key_traffic(key),
        }
    }

    fn handle_key_port_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => self.refresh_ports(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_port > 0 {
                    self.selected_port -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.ports.is_empty() && self.selected_port < self.ports.len() - 1 {
                    self.selected_port += 1;
                }
            }
            KeyCode::Enter => {
                if !self.ports.is_empty() {
                    self.connect_to_selected_port();
                }
            }
            _ => {}
        }
    }

    fn handle_key_traffic(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => {
                self.disconnect();
                self.view = View::PortSelect;
                self.status = "Disconnected.".to_string();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Char('g') => {
                self.scroll_offset = 0;
            }
            KeyCode::Char('G') => {
                // Scroll to bottom - will be clamped in render
                self.scroll_offset = usize::MAX;
            }
            KeyCode::Esc => {
                self.disconnect();
                self.view = View::PortSelect;
                self.status = "Disconnected.".to_string();
            }
            _ => {}
        }
    }

    fn connect_to_selected_port(&mut self) {
        if let Some(port) = self.ports.get(self.selected_port) {
            let port_name = port.name.clone();
            let config = SerialConfig::default();

            self.status = format!("Connecting to {}...", port_name);

            // Use block_on to connect synchronously from the UI thread
            match self.runtime.block_on(Session::connect(&port_name, config)) {
                Ok(handle) => {
                    self.connection = ConnectionState::Connected(handle);
                    self.view = View::Traffic;
                    self.scroll_offset = 0;
                    self.status = format!("Connected to {} @ 115200 baud", port_name);
                }
                Err(e) => {
                    self.status = format!("Failed to connect: {}", e);
                }
            }
        }
    }

    fn disconnect(&mut self) {
        if let ConnectionState::Connected(handle) = std::mem::replace(
            &mut self.connection,
            ConnectionState::Disconnected,
        ) {
            // Fire and forget disconnect
            let _ = self.runtime.block_on(handle.disconnect());
        }
    }

    /// Poll for session events (non-blocking)
    pub fn poll_session_events(&mut self) {
        if let ConnectionState::Connected(ref mut handle) = self.connection {
            while let Some(event) = handle.try_recv_event() {
                match event {
                    SessionEvent::Disconnected { error } => {
                        self.status = match error {
                            Some(e) => format!("Disconnected: {}", e),
                            None => "Disconnected.".to_string(),
                        };
                        self.connection = ConnectionState::Disconnected;
                        self.view = View::PortSelect;
                        break;
                    }
                    SessionEvent::Error(e) => {
                        self.status = format!("Error: {}", e);
                    }
                    SessionEvent::DataReceived(_) | SessionEvent::DataSent(_) => {
                        // Data is already in the buffer, just need to refresh display
                    }
                    SessionEvent::Connected => {
                        // Already handled
                    }
                }
            }
        }
    }

    /// Get the tick rate for the event loop
    pub fn tick_rate(&self) -> Duration {
        Duration::from_millis(50)
    }
}
