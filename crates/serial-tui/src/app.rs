//! Main application state machine and event loop.

use std::{io, time::{Duration, SystemTime}};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Terminal,
    backend::Backend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use serial_core::{
    ChunkingStrategy, KeepAwake, SerialConfig, Session, SessionConfig, SessionEvent, SessionHandle,
};

use crate::{
    event::{AppEvent, poll_event},
    theme::Theme,
    view::{file_sender::FileSenderView, graph::GraphView, pre_connect::PreConnectView, traffic::TrafficView},
    widget::{
        HelpOverlay, Toasts,
        help_overlay::HelpOverlayState,
        text_input::TextInputState,
        toast::render_toasts,
    },
};

/// Main application state.
pub struct App {
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Toast notifications.
    pub toasts: Toasts,
    /// Help overlay state.
    pub help: HelpOverlayState,
    /// Current view mode.
    pub mode: AppMode,
    /// Whether the config panel is visible.
    pub show_config: bool,
    /// Current focus area.
    pub focus: Focus,
    /// Command input state (vim-like ':' command mode).
    pub command_input: TextInputState,
    /// Whether command mode is active.
    pub command_mode: bool,
    /// Whether the terminal needs a full clear on next draw.
    pub needs_clear: bool,
}

/// Current view mode.
pub enum AppMode {
    /// Pre-connection: port selection and configuration.
    PreConnect(PreConnectView),
    /// Connected: traffic, graph, or file sender views.
    Connected(ConnectedState),
}

/// State when connected to a serial port.
pub struct ConnectedState {
    /// Active session handle.
    pub handle: SessionHandle,
    /// Current tab.
    pub tab: ConnectedTab,
    /// Traffic view state.
    pub traffic: TrafficView,
    /// Graph view state.
    pub graph: GraphView,
    /// File sender view state.
    pub file_sender: FileSenderView,
    /// Connection config (read-only display).
    pub serial_config: SerialConfig,
    /// Keep-awake handle (prevents system sleep while enabled).
    pub keep_awake: KeepAwake,
}

/// Tabs when connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectedTab {
    #[default]
    Traffic,
    Graph,
    FileSender,
}

impl ConnectedTab {
    pub fn title(self) -> &'static str {
        match self {
            ConnectedTab::Traffic => "Traffic",
            ConnectedTab::Graph => "Graph",
            ConnectedTab::FileSender => "File Sender",
        }
    }

    pub fn all() -> [ConnectedTab; 3] {
        [ConnectedTab::Traffic, ConnectedTab::Graph, ConnectedTab::FileSender]
    }
}

/// Focus area within the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Main,
    Config,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            toasts: Toasts::new(),
            help: HelpOverlayState::default(),
            mode: AppMode::PreConnect(PreConnectView::new()),
            show_config: true,
            focus: Focus::Main,
            command_input: TextInputState::new().with_placeholder("Enter command..."),
            command_mode: false,
            needs_clear: false,
        }
    }

    /// Main event loop.
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        // Initial port scan
        if let AppMode::PreConnect(ref mut view) = self.mode {
            view.refresh_ports();
        }

        loop {
            // Force full terminal redraw if needed before drawing
            // This handles clears requested from previous iterations (e.g., mode changes)
            if self.needs_clear {
                terminal.clear()?;
                self.needs_clear = false;
            }

            // Draw UI
            terminal.draw(|f| self.draw(f.area(), f.buffer_mut()))?;

            // Poll for events (with timeout for session events)
            if let Some(event) = poll_event(Duration::from_millis(50)) {
                self.handle_event(event).await;
            }

            // Handle session events if connected
            self.process_session_events();

            // Tick toasts - request clear if any expired (they use Clear which leaves artifacts)
            if self.toasts.tick() {
                self.needs_clear = true;
            }

            if self.should_quit {
                // Cleanup
                if let AppMode::Connected(state) = std::mem::replace(
                    &mut self.mode,
                    AppMode::PreConnect(PreConnectView::new()),
                ) {
                    let _ = state.handle.disconnect().await;
                }
                break;
            }
        }

        Ok(())
    }

    fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        // Reserve space at the bottom for command bar if in command mode
        let (content_area, command_area) = if self.command_mode {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        // Main layout: main view + optional config panel
        let chunks = if self.show_config {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(content_area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)])
                .split(content_area)
        };

        let main_area = chunks[0];
        let config_area = if self.show_config {
            Some(chunks[1])
        } else {
            None
        };

        // Draw based on mode
        match &mut self.mode {
            AppMode::PreConnect(view) => {
                view.draw(main_area, config_area, buf, self.focus);
            }
            AppMode::Connected(state) => {
                Self::draw_connected(state, main_area, config_area, buf, self.focus);
            }
        }

        // Draw command bar if in command mode
        if let Some(cmd_area) = command_area {
            self.draw_command_bar(cmd_area, buf);
        }

        // Draw toasts overlay
        render_toasts(&self.toasts, area, buf);

        // Draw help overlay
        HelpOverlay::new(&self.help).render(area, buf);
    }

    fn draw_command_bar(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Command ")
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let inner = block.inner(area);
        block.render(area, buf);

        // Draw the ":" prefix and input
        let prefix = Span::styled(":", Theme::keybind());
        let content = Span::raw(&self.command_input.content);
        let line = Line::from(vec![prefix, content]);

        Paragraph::new(line).render(inner, buf);

        // Draw cursor
        let cursor_x = inner.x + 1 + self.command_input.cursor_display_pos() as u16;
        if cursor_x < inner.x + inner.width {
            if let Some(cell) = buf.cell_mut((cursor_x, inner.y)) {
                cell.set_bg(Theme::PRIMARY);
                cell.set_fg(Theme::BG);
            }
        }
    }

    fn process_session_events(&mut self) {
        let events: Vec<SessionEvent> = if let AppMode::Connected(ref mut state) = self.mode {
            let mut events = Vec::new();
            while let Some(event) = state.handle.try_recv_event() {
                events.push(event);
            }
            events
        } else {
            return;
        };

        for event in events {
            self.handle_session_event(event);
        }
    }

    fn draw_connected(
        state: &mut ConnectedState,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut Buffer,
        focus: Focus,
    ) {
        match state.tab {
            ConnectedTab::Traffic => {
                state.traffic.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.handle,
                    &state.serial_config,
                    focus,
                );
            }
            ConnectedTab::Graph => {
                state.graph.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.handle,
                    &state.serial_config,
                    focus,
                );
            }
            ConnectedTab::FileSender => {
                state.file_sender.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.handle,
                    &state.serial_config,
                    focus,
                );
            }
        }
    }

    async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                // Handle command mode first
                if self.command_mode {
                    match key.code {
                        KeyCode::Enter => {
                            let cmd = self.command_input.take();
                            self.command_mode = false;
                            self.execute_command(&cmd).await;
                        }
                        KeyCode::Esc => {
                            self.command_mode = false;
                            self.command_input.clear();
                        }
                        _ => {
                            self.command_input.handle_key(key);
                        }
                    }
                    return;
                }

                // Handle help overlay (it captures all input when visible)
                if self.help.visible {
                    if self.help.handle_key(key) {
                        self.needs_clear = true;
                    }
                    return;
                }

                // Global keybindings
                match key.code {
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                        return;
                    }
                    KeyCode::Char('?') => {
                        if self.help.toggle() {
                            self.needs_clear = true;
                        }
                        return;
                    }
                    KeyCode::Char('c')
                        if !self.is_input_mode() =>
                    {
                        self.show_config = !self.show_config;
                        // Request clear to avoid rendering artifacts when layout changes
                        self.needs_clear = true;
                        return;
                    }
                    // ':' opens command mode (vim-style)
                    KeyCode::Char(':') if !self.is_input_mode() => {
                        self.command_mode = true;
                        self.command_input.clear();
                        return;
                    }
                    // Ctrl+h moves focus left (to Main panel)
                    KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) && !self.is_input_mode() => {
                        // Close dropdowns when switching focus
                        if let AppMode::Connected(ref mut state) = self.mode {
                            state.traffic.config_nav.close_dropdown();
                            state.graph.config_nav.close_dropdown();
                            state.file_sender.config_nav.close_dropdown();
                        } else if let AppMode::PreConnect(ref mut view) = self.mode {
                            view.config_nav.close_dropdown();
                        }
                        self.focus = Focus::Main;
                        return;
                    }
                    // Ctrl+l moves focus right (to Config panel)
                    KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) && !self.is_input_mode() => {
                        if self.show_config {
                            self.focus = Focus::Config;
                        }
                        return;
                    }
                    _ => {}
                }

                // Mode-specific handling
                match &mut self.mode {
                    AppMode::PreConnect(view) => {
                        if let Some(action) = view.handle_key(key, self.focus) {
                            self.handle_preconnect_action(action).await;
                        }
                    }
                    AppMode::Connected(state) => {
                        // Tab switching
                        let is_input_mode = match state.tab {
                            ConnectedTab::Traffic => state.traffic.is_input_mode(),
                            ConnectedTab::Graph => false,
                            ConnectedTab::FileSender => state.file_sender.is_input_mode(),
                        };

                        if !is_input_mode {
                            match key.code {
                                KeyCode::Char('1') if state.tab != ConnectedTab::Traffic => {
                                    state.traffic.config_nav.close_dropdown();
                                    state.graph.config_nav.close_dropdown();
                                    state.file_sender.config_nav.close_dropdown();
                                    state.tab = ConnectedTab::Traffic;
                                    self.needs_clear = true;
                                    return;
                                }
                                KeyCode::Char('2') if state.tab != ConnectedTab::Graph => {
                                    state.traffic.config_nav.close_dropdown();
                                    state.graph.config_nav.close_dropdown();
                                    state.file_sender.config_nav.close_dropdown();
                                    state.tab = ConnectedTab::Graph;
                                    self.needs_clear = true;
                                    return;
                                }
                                KeyCode::Char('3') if state.tab != ConnectedTab::FileSender => {
                                    state.traffic.config_nav.close_dropdown();
                                    state.graph.config_nav.close_dropdown();
                                    state.file_sender.config_nav.close_dropdown();
                                    state.tab = ConnectedTab::FileSender;
                                    self.needs_clear = true;
                                    return;
                                }
                                // 'd' disconnects only without modifiers (Ctrl+d is half-page scroll)
                                KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    self.disconnect().await;
                                    return;
                                }
                                _ => {}
                            }
                        }

                        // Tab-specific handling
                        match state.tab {
                            ConnectedTab::Traffic => {
                                if let Some(action) = state.traffic.handle_key(key, self.focus, &state.handle) {
                                    self.handle_traffic_action(action).await;
                                }
                            }
                            ConnectedTab::Graph => {
                                state.graph.handle_key(key, self.focus, &state.handle);
                            }
                            ConnectedTab::FileSender => {
                                if let Some(action) = state.file_sender.handle_key(key, self.focus) {
                                    self.handle_file_sender_action(action).await;
                                }
                            }
                        }
                    }
                }
            }
            AppEvent::Mouse(_) => {
                // Mouse events are ignored - native terminal selection works
            }
            AppEvent::Resize(_, _) => {
                // Terminal will redraw automatically
            }
            AppEvent::Tick => {
                // Update file sender progress if active
                if let AppMode::Connected(ref mut state) = self.mode {
                    state.file_sender.tick();
                }
            }
        }
    }

    async fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "connect" | "c" => {
                if parts.len() < 2 {
                    self.toasts.error("Usage: :connect <port_path>");
                    return;
                }
                let port_path = parts[1];
                // Get serial config and file save settings from pre-connect view if available, or use defaults
                let (serial_config, rx_chunking, file_save_enabled, file_save_format_index, file_save_encoding_index, file_save_directory) = match &self.mode {
                    AppMode::PreConnect(view) => {
                        (
                            view.config.to_serial_config(),
                            view.config.rx_chunking(),
                            view.config.file_save_enabled,
                            view.config.file_save_format_index,
                            view.config.file_save_encoding_index,
                            view.config.file_save_directory.clone(),
                        )
                    }
                    AppMode::Connected(state) => {
                        (state.serial_config.clone(), ChunkingStrategy::Raw, false, 1, 1, 
                         serial_core::buffer::default_cache_directory().to_string_lossy().into_owned())
                    }
                };
                // Build session config from global settings
                let settings = &self.help.settings;
                let session_config = SessionConfig {
                    rx_chunking,
                    tx_chunking: ChunkingStrategy::Raw,
                    buffer_size: settings.buffer_size(),
                    auto_save: settings.to_auto_save_config(),
                };
                self.connect(
                    port_path,
                    serial_config,
                    session_config,
                    settings.keep_awake,
                    file_save_enabled,
                    file_save_format_index,
                    file_save_encoding_index,
                    file_save_directory,
                ).await;
            }
            "disconnect" | "d" => {
                self.disconnect().await;
            }
            "quit" | "q" => {
                self.should_quit = true;
            }
            "save" | "w" => {
                if parts.len() < 2 {
                    self.toasts.error("Usage: :save <file_path>");
                    return;
                }
                let path = parts[1];
                self.save_buffer(path).await;
            }
            "help" | "h" => {
                self.toasts.info("Commands: :connect <path>, :disconnect, :save <path>, :quit");
            }
            _ => {
                self.toasts.error(format!("Unknown command: {}", parts[0]));
            }
        }
    }

    async fn save_buffer(&mut self, path: &str) {
        if let AppMode::Connected(ref state) = self.mode {
            let buffer = state.handle.buffer();
            let mut content = String::new();
            for chunk in buffer.chunks() {
                let dir = match chunk.direction {
                    serial_core::Direction::Tx => "TX",
                    serial_core::Direction::Rx => "RX",
                };
                content.push_str(&format!("[{}] {}\n", dir, chunk.encoded));
            }
            match std::fs::write(path, &content) {
                Ok(()) => {
                    self.toasts.success(format!("Saved {} bytes to {}", content.len(), path));
                }
                Err(e) => {
                    self.toasts.error(format!("Failed to save: {}", e));
                }
            }
        } else {
            self.toasts.error("Not connected - nothing to save");
        }
    }

    fn handle_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected => {
                self.toasts.success("Connected");
            }
            SessionEvent::Disconnected { error } => {
                if let Some(err) = error {
                    self.toasts.error(format!("Disconnected: {}", err));
                } else {
                    self.toasts.info("Disconnected");
                }
                // Return to pre-connect mode
                self.mode = AppMode::PreConnect(PreConnectView::new());
                if let AppMode::PreConnect(ref mut view) = self.mode {
                    view.refresh_ports();
                }
            }
            SessionEvent::Error(msg) => {
                self.toasts.error(msg);
            }
            SessionEvent::DataReceived { .. } | SessionEvent::DataSent { .. } => {
                // Data is already in the buffer, UI will pick it up on next render
            }
        }
    }

    async fn handle_preconnect_action(&mut self, action: PreConnectAction) {
        match action {
            PreConnectAction::Connect {
                port,
                serial_config,
                rx_chunking,
                file_save_enabled,
                file_save_format_index,
                file_save_encoding_index,
                file_save_directory,
            } => {
                // Build session config from global settings
                let settings = &self.help.settings;
                let session_config = SessionConfig {
                    rx_chunking,
                    tx_chunking: ChunkingStrategy::Raw,
                    buffer_size: settings.buffer_size(),
                    auto_save: settings.to_auto_save_config(),
                };
                self.connect(
                    &port,
                    serial_config,
                    session_config,
                    settings.keep_awake,
                    file_save_enabled,
                    file_save_format_index,
                    file_save_encoding_index,
                    file_save_directory,
                ).await;
            }
            PreConnectAction::Toast(toast) => {
                self.toasts.push(toast);
            }
        }
    }

    async fn handle_traffic_action(&mut self, action: TrafficAction) {
        match action {
            TrafficAction::Send(data) => {
                if let AppMode::Connected(ref state) = self.mode {
                    if let Err(e) = state.handle.send(data).await {
                        self.toasts.error(format!("Send failed: {}", e));
                    }
                }
                // Layout changed (send bar closed) - request clear to avoid artifacts
                self.needs_clear = true;
            }
            TrafficAction::Toast(toast) => {
                self.toasts.push(toast);
            }
            TrafficAction::RequestClear => {
                self.needs_clear = true;
            }
            TrafficAction::StartFileSaving => {
                if let AppMode::Connected(ref mut state) = self.mode {
                    let config = &state.traffic.config;
                    
                    // Build save config from traffic settings
                    let save_config = build_user_save_config(
                        &config.file_save_directory,
                        state.handle.port_name(),
                        config.file_save_format_index,
                        config.file_save_encoding_index,
                    );
                    
                    // Start saving
                    let runtime = tokio::runtime::Handle::current();
                    let mut buffer = state.handle.buffer_mut();
                    if let Err(e) = buffer.save(save_config, &runtime) {
                        drop(buffer);
                        state.traffic.config.file_save_enabled = false;
                        self.toasts.error(format!("Failed to start file saving: {}", e));
                    } else {
                        let path = buffer.save_path().map(|p| p.display().to_string()).unwrap_or_default();
                        drop(buffer);
                        self.toasts.success(format!("Saving to {}", path));
                    }
                }
            }
            TrafficAction::StopFileSaving => {
                if let AppMode::Connected(ref state) = self.mode {
                    state.handle.buffer_mut().stop_saving();
                    self.toasts.info("File saving stopped");
                }
            }
        }
    }

    async fn handle_file_sender_action(&mut self, action: FileSenderAction) {
        match action {
            FileSenderAction::StartSending => {
                if let AppMode::Connected(ref mut state) = self.mode {
                    if let Err(e) = state.file_sender.start_sending(&state.handle).await {
                        self.toasts.error(format!("Failed to start sending: {}", e));
                    } else {
                        self.toasts.info("File sending started");
                    }
                }
            }
            FileSenderAction::CancelSending => {
                if let AppMode::Connected(ref mut state) = self.mode {
                    state.file_sender.cancel_sending();
                    self.toasts.info("File sending cancelled");
                }
            }
            FileSenderAction::Toast(toast) => {
                self.toasts.push(toast);
            }
        }
    }

    async fn connect(
        &mut self,
        port: &str,
        serial_config: SerialConfig,
        session_config: SessionConfig,
        keep_awake: bool,
        file_save_enabled: bool,
        file_save_format_index: usize,
        file_save_encoding_index: usize,
        file_save_directory: String,
    ) {
        self.toasts.info(format!("Connecting to {}...", port));

        match Session::connect_with_config(port, serial_config.clone(), session_config).await {
            Ok(handle) => {
                let mut traffic = TrafficView::new();
                traffic.session_start = Some(SystemTime::now());
                
                // Apply file saving settings from pre-connect config
                traffic.config.file_save_enabled = file_save_enabled;
                traffic.config.file_save_format_index = file_save_format_index;
                traffic.config.file_save_encoding_index = file_save_encoding_index;
                traffic.config.file_save_directory = file_save_directory.clone();
                
                // Create keep-awake handle and enable if setting is on
                let mut keep_awake_handle = KeepAwake::new();
                if keep_awake {
                    keep_awake_handle.enable();
                    if !keep_awake_handle.is_active() {
                        self.toasts.info("Keep-awake not available on this system");
                    }
                }
                
                // Start file saving if enabled from pre-connect
                if file_save_enabled {
                    let save_config = build_user_save_config(
                        &file_save_directory,
                        handle.port_name(),
                        file_save_format_index,
                        file_save_encoding_index,
                    );
                    
                    let runtime = tokio::runtime::Handle::current();
                    let mut buffer = handle.buffer_mut();
                    match buffer.save(save_config, &runtime) {
                        Ok(()) => {
                            let path = buffer.save_path().map(|p| p.display().to_string()).unwrap_or_default();
                            drop(buffer);
                            self.toasts.success(format!("Saving to {}", path));
                        }
                        Err(e) => {
                            drop(buffer);
                            traffic.config.file_save_enabled = false;
                            self.toasts.error(format!("Failed to start file saving: {}", e));
                        }
                    }
                }
                
                let state = ConnectedState {
                    handle,
                    tab: ConnectedTab::Traffic,
                    traffic,
                    graph: GraphView::new(),
                    file_sender: FileSenderView::new(),
                    serial_config,
                    keep_awake: keep_awake_handle,
                };
                self.mode = AppMode::Connected(state);
                self.needs_clear = true;
            }
            Err(e) => {
                self.toasts.error(format!("Connection failed: {}", e));
            }
        }
    }

    async fn disconnect(&mut self) {
        if let AppMode::Connected(state) = std::mem::replace(
            &mut self.mode,
            AppMode::PreConnect(PreConnectView::new()),
        ) {
            let _ = state.handle.disconnect().await;
            self.toasts.info("Disconnected");
        }
        if let AppMode::PreConnect(ref mut view) = self.mode {
            view.refresh_ports();
        }
    }

    fn is_input_mode(&self) -> bool {
        if self.command_mode {
            return true;
        }
        match &self.mode {
            AppMode::PreConnect(view) => view.is_input_mode(),
            AppMode::Connected(state) => match state.tab {
                ConnectedTab::Traffic => state.traffic.is_input_mode(),
                ConnectedTab::Graph => false,
                ConnectedTab::FileSender => state.file_sender.is_input_mode(),
            },
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions from pre-connect view.
pub enum PreConnectAction {
    Connect {
        port: String,
        serial_config: SerialConfig,
        /// RX chunking strategy (from pre-connect config).
        rx_chunking: ChunkingStrategy,
        /// File saving settings from pre-connect config.
        file_save_enabled: bool,
        file_save_format_index: usize,
        file_save_encoding_index: usize,
        file_save_directory: String,
    },
    Toast(crate::widget::Toast),
}

/// Actions from traffic view.
pub enum TrafficAction {
    Send(Vec<u8>),
    Toast(crate::widget::Toast),
    /// Request a full terminal clear (for layout changes).
    RequestClear,
    /// Start file saving with the current config.
    StartFileSaving,
    /// Stop file saving.
    StopFileSaving,
}

/// Actions from file sender view.
pub enum FileSenderAction {
    StartSending,
    CancelSending,
    Toast(crate::widget::Toast),
}

/// Build a UserSaveConfig from traffic config settings.
fn build_user_save_config(
    directory: &str,
    port_name: &str,
    format_index: usize,
    encoding_index: usize,
) -> serial_core::UserSaveConfig {
    use serial_core::{Encoding, SaveFormat, SaveScope, UserSaveConfig, DirectionFilter};
    use std::path::PathBuf;
    use std::time::SystemTime;
    use chrono::{DateTime, Utc};
    
    // Generate filename with timestamp
    let clean_port_name = port_name
        .replace(['/', '\\'], "_")
        .trim_start_matches("_dev_")
        .to_string();
    let dt: DateTime<Utc> = SystemTime::now().into();
    let timestamp = dt.format("%Y-%m-%dT%H-%M-%S");
    
    // Determine format and extension
    let (format, extension) = if format_index == 0 {
        // Raw Binary
        (SaveFormat::Raw, "bin")
    } else {
        // Encoded Text
        let encoding = match encoding_index {
            0 => Encoding::Utf8,
            1 => Encoding::Ascii,
            2 => Encoding::Hex(Default::default()),
            3 => Encoding::Binary(Default::default()),
            _ => Encoding::Ascii,
        };
        let ext = match encoding {
            Encoding::Utf8 | Encoding::Ascii => "txt",
            Encoding::Hex(_) => "hex",
            Encoding::Binary(_) => "bin",
        };
        (
            SaveFormat::Encoded {
                encoding,
                include_timestamps: true,
                include_direction: true,
            },
            ext,
        )
    };
    
    let filename = format!("{}-{}.{}", clean_port_name, timestamp, extension);
    let path = PathBuf::from(directory).join(filename);
    
    UserSaveConfig {
        path,
        scope: SaveScope::ExistingAndContinue, // Save existing buffer + continue streaming
        format,
        directions: DirectionFilter::all(),
    }
}
