//! Application state and logic

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serial_core::{encode, list_ports, send_file, Encoding, FileSendConfig, FileSendHandle, FileSendProgress, PortInfo, SerialConfig, Session, SessionEvent, SessionHandle};

/// Current view/screen
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Port selection screen
    PortSelect,
    /// Traffic view (main view)
    Traffic,
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// Normal navigation mode
    Normal,
    /// Entering a port path manually
    PortInput,
    /// Entering data to send to serial port
    SendInput,
    /// Entering search pattern
    SearchInput,
    /// Entering file path to send
    FilePathInput,
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
    /// Should the terminal be fully cleared on next render?
    /// This is needed when content changes dramatically (e.g., encoding change)
    /// to prevent artifacts from ratatui's differential rendering.
    pub needs_full_clear: bool,
    /// Current view
    pub view: View,
    /// Input mode
    pub input_mode: InputMode,
    /// Input buffer for text entry
    pub input_buffer: String,
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
    /// Current display encoding
    pub encoding: Encoding,
    /// Current search pattern (if any)
    pub search_pattern: Option<String>,
    /// Current search match index (line index in the displayed data)
    pub search_match_index: Option<usize>,
    /// Total number of search matches
    pub search_match_count: usize,
    /// Active file send operation
    pub file_send: Option<FileSendHandle>,
    /// Latest file send progress
    pub file_send_progress: Option<FileSendProgress>,
    /// Tokio runtime handle for async operations
    runtime: tokio::runtime::Handle,
}

impl App {
    /// Create a new application
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        let ports = list_ports().unwrap_or_default();
        let status = if ports.is_empty() {
            "No serial ports found. Press ':' to enter path manually, 'r' to refresh.".to_string()
        } else {
            format!(
                "Found {} port(s). Select and press Enter, or ':' to enter path manually.",
                ports.len()
            )
        };

        Self {
            should_quit: false,
            needs_full_clear: false,
            view: View::PortSelect,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            ports,
            selected_port: 0,
            connection: ConnectionState::Disconnected,
            status,
            scroll_offset: 0,
            encoding: Encoding::default(),
            search_pattern: None,
            search_match_index: None,
            search_match_count: 0,
            file_send: None,
            file_send_progress: None,
            runtime,
        }
    }

    /// Refresh the list of available ports
    pub fn refresh_ports(&mut self) {
        self.ports = list_ports().unwrap_or_default();
        self.selected_port = 0;
        self.status = if self.ports.is_empty() {
            "No serial ports found. Press ':' to enter path manually.".to_string()
        } else {
            format!("Found {} port(s).", self.ports.len())
        };
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.input_mode {
            InputMode::Normal => match self.view {
                View::PortSelect => self.handle_key_port_select(key),
                View::Traffic => self.handle_key_traffic(key),
            },
            InputMode::PortInput => self.handle_key_port_input(key),
            InputMode::SendInput => self.handle_key_send_input(key),
            InputMode::SearchInput => self.handle_key_search_input(key),
            InputMode::FilePathInput => self.handle_key_file_path_input(key),
        }
    }

    fn handle_key_port_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('r') => self.refresh_ports(),
            KeyCode::Char(':') => {
                self.input_mode = InputMode::PortInput;
                self.input_buffer.clear();
                self.status = "Enter port path (e.g., /dev/pts/5):".to_string();
            }
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

    fn handle_key_port_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    let port_path = self.input_buffer.clone();
                    self.input_mode = InputMode::Normal;
                    self.input_buffer.clear();
                    self.connect_to_port(&port_path);
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status = "Cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                // Don't insert control characters
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input_buffer.push(c);
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
                self.needs_full_clear = true;
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
            // Page up (Ctrl-u)
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(self.page_size());
            }
            // Page down (Ctrl-d)
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_add(self.page_size());
            }
            // Cycle encoding
            KeyCode::Char('e') => {
                self.encoding = self.encoding.cycle_next();
                self.status = format!("Encoding: {}", self.encoding);
                // Request full terminal clear to prevent artifacts from wrapping differences
                self.needs_full_clear = true;
                // Re-run search with new encoding if there's an active search
                if self.search_pattern.is_some() {
                    self.update_search_matches();
                }
            }
            // Enter send mode (vim-like insert mode)
            KeyCode::Char('i') => {
                self.input_mode = InputMode::SendInput;
                self.input_buffer.clear();
                self.status = "Type to send (Enter: send with newline, Esc: cancel)".to_string();
            }
            // Enter search mode
            KeyCode::Char('/') => {
                self.input_mode = InputMode::SearchInput;
                self.input_buffer.clear();
                self.status = "Search: ".to_string();
            }
            // Next search match
            KeyCode::Char('n') => {
                self.goto_next_match();
            }
            // Previous search match
            KeyCode::Char('N') => {
                self.goto_prev_match();
            }
            // Send file
            KeyCode::Char('f') => {
                if self.file_send.is_some() {
                    // Cancel ongoing file send
                    self.cancel_file_send();
                } else {
                    self.input_mode = InputMode::FilePathInput;
                    self.input_buffer.clear();
                    self.status = "Enter file path to send:".to_string();
                }
            }
            // Clear search
            KeyCode::Esc => {
                if self.search_pattern.is_some() {
                    self.clear_search();
                    self.status = "Search cleared.".to_string();
                } else {
                    self.disconnect();
                    self.view = View::PortSelect;
                    self.needs_full_clear = true;
                    self.status = "Disconnected.".to_string();
                }
            }
            _ => {}
        }
    }

    fn handle_key_search_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    self.search_pattern = Some(self.input_buffer.clone());
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Normal;
                    self.update_search_matches();
                    // Jump to first match
                    self.goto_next_match();
                } else {
                    self.input_mode = InputMode::Normal;
                    self.status = "Search cancelled.".to_string();
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status = "Search cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input_buffer.push(c);
                }
            }
            _ => {}
        }
    }

    /// Find all lines that match the current search pattern
    fn find_matching_lines(&self) -> Vec<usize> {
        let pattern = match &self.search_pattern {
            Some(p) => p,
            None => return vec![],
        };

        let mut matches = Vec::new();

        if let ConnectionState::Connected(ref handle) = self.connection {
            let buffer = handle.buffer();
            for (idx, chunk) in buffer.chunks().enumerate() {
                let encoded = encode(&chunk.data, self.encoding);
                if encoded.to_lowercase().contains(&pattern.to_lowercase()) {
                    matches.push(idx);
                }
            }
        }

        matches
    }

    /// Update search match count (call after search or encoding change)
    fn update_search_matches(&mut self) {
        let matches = self.find_matching_lines();
        self.search_match_count = matches.len();
        
        if matches.is_empty() {
            self.search_match_index = None;
            if let Some(ref pattern) = self.search_pattern {
                self.status = format!("Pattern not found: {}", pattern);
            }
        } else {
            self.status = format!(
                "Found {} match{}",
                self.search_match_count,
                if self.search_match_count == 1 { "" } else { "es" }
            );
        }
    }

    /// Go to next search match
    fn goto_next_match(&mut self) {
        let matches = self.find_matching_lines();
        if matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        let next_idx = match self.search_match_index {
            Some(current) => {
                // Find next match after current
                matches
                    .iter()
                    .position(|&m| m > current)
                    .unwrap_or(0) // Wrap to first match
            }
            None => 0,
        };

        self.search_match_index = Some(matches[next_idx]);
        self.scroll_offset = matches[next_idx];
        self.status = format!(
            "Match {}/{}: {}",
            next_idx + 1,
            matches.len(),
            self.search_pattern.as_deref().unwrap_or("")
        );
    }

    /// Go to previous search match
    fn goto_prev_match(&mut self) {
        let matches = self.find_matching_lines();
        if matches.is_empty() {
            self.status = "No matches".to_string();
            return;
        }

        let prev_idx = match self.search_match_index {
            Some(current) => {
                // Find previous match before current
                matches
                    .iter()
                    .rposition(|&m| m < current)
                    .unwrap_or(matches.len() - 1) // Wrap to last match
            }
            None => matches.len() - 1,
        };

        self.search_match_index = Some(matches[prev_idx]);
        self.scroll_offset = matches[prev_idx];
        self.status = format!(
            "Match {}/{}: {}",
            prev_idx + 1,
            matches.len(),
            self.search_pattern.as_deref().unwrap_or("")
        );
    }

    /// Clear search state
    fn clear_search(&mut self) {
        self.search_pattern = None;
        self.search_match_index = None;
        self.search_match_count = 0;
    }

    fn handle_key_file_path_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    let path = self.input_buffer.clone();
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Normal;
                    self.start_file_send(&path);
                } else {
                    self.input_mode = InputMode::Normal;
                    self.status = "File send cancelled.".to_string();
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status = "File send cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input_buffer.push(c);
                }
            }
            _ => {}
        }
    }

    fn start_file_send(&mut self, path: &str) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let config = FileSendConfig::default()
                .with_chunk_size(64)
                .with_delay(std::time::Duration::from_millis(10));

            match self.runtime.block_on(send_file(handle, path, config)) {
                Ok(file_handle) => {
                    self.file_send = Some(file_handle);
                    self.file_send_progress = None;
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

    fn cancel_file_send(&mut self) {
        if let Some(ref handle) = self.file_send {
            self.runtime.block_on(handle.cancel());
        }
        self.file_send = None;
        self.file_send_progress = None;
        self.status = "File send cancelled.".to_string();
    }

    /// Poll for file send progress
    pub fn poll_file_send(&mut self) {
        if let Some(ref mut handle) = self.file_send {
            while let Some(progress) = handle.try_recv_progress() {
                let complete = progress.complete;
                let error = progress.error.clone();
                self.file_send_progress = Some(progress);

                if complete {
                    if let Some(err) = error {
                        self.status = format!("File send failed: {}", err);
                    } else {
                        self.status = "File send complete.".to_string();
                    }
                    self.file_send = None;
                    break;
                }
            }
        }
    }

    fn handle_key_send_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    // Send with newline appended
                    let mut data = self.input_buffer.clone();
                    data.push('\n');
                    self.send_data(data.into_bytes());
                    self.input_buffer.clear();
                }
                // Stay in input mode for continuous sending
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.status = "Send cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                // Ctrl+Enter sends without newline
                if c == 'j' && key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl-J is often interpreted as Enter by terminals
                    if !self.input_buffer.is_empty() {
                        let data = self.input_buffer.clone();
                        self.send_data(data.into_bytes());
                        self.input_buffer.clear();
                    }
                } else if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input_buffer.push(c);
                }
            }
            _ => {}
        }
    }

    fn send_data(&mut self, data: Vec<u8>) {
        if let ConnectionState::Connected(ref handle) = self.connection {
            let len = data.len();
            // Use block_on since we're in a sync context
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

    fn connect_to_selected_port(&mut self) {
        if let Some(port) = self.ports.get(self.selected_port) {
            let port_name = port.name.clone();
            self.connect_to_port(&port_name);
        }
    }

    fn connect_to_port(&mut self, port_name: &str) {
        let config = SerialConfig::default();

        self.status = format!("Connecting to {}...", port_name);

        // Use block_on to connect synchronously from the UI thread
        match self.runtime.block_on(Session::connect(port_name, config)) {
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

    fn disconnect(&mut self) {
        if let ConnectionState::Connected(handle) =
            std::mem::replace(&mut self.connection, ConnectionState::Disconnected)
        {
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
                        self.needs_full_clear = true;
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

    /// Get page size for Ctrl-d/u scrolling (half screen)
    /// Returns a reasonable default since we don't know terminal height here
    fn page_size(&self) -> usize {
        // Default to ~half a typical terminal height
        // The actual clamping happens in render
        15
    }
}
