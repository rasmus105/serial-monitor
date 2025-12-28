//! Main application state machine and event loop.

use std::{io, time::{Duration, SystemTime}};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Widget,
};
use serial_core::{
    SerialConfig, Session, SessionConfig, SessionEvent, SessionHandle,
};

use crate::{
    event::{AppEvent, poll_event},
    view::{file_sender::FileSenderView, graph::GraphView, pre_connect::PreConnectView, traffic::TrafficView},
    widget::{
        HelpOverlay, Toasts,
        help_overlay::HelpOverlayState,
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
        }
    }

    /// Main event loop.
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        // Initial port scan
        if let AppMode::PreConnect(ref mut view) = self.mode {
            view.refresh_ports();
        }

        loop {
            // Draw UI
            terminal.draw(|f| self.draw(f.area(), f.buffer_mut()))?;

            // Poll for events (with timeout for session events)
            if let Some(event) = poll_event(Duration::from_millis(50)) {
                self.handle_event(event).await;
            }

            // Handle session events if connected
            self.process_session_events();

            // Tick toasts
            self.toasts.tick();

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

    fn draw(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        // Main layout: main view + optional config panel
        let chunks = if self.show_config {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)])
                .split(area)
        };

        let main_area = chunks[0];
        let config_area = if self.show_config {
            Some(chunks[1])
        } else {
            None
        };

        // Draw based on mode
        match &self.mode {
            AppMode::PreConnect(view) => {
                view.draw(main_area, config_area, buf, self.focus);
            }
            AppMode::Connected(state) => {
                self.draw_connected(state, main_area, config_area, buf);
            }
        }

        // Draw toasts overlay
        render_toasts(&self.toasts, area, buf);

        // Draw help overlay
        HelpOverlay::new(&self.help).render(area, buf);
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
        &self,
        state: &ConnectedState,
        main_area: Rect,
        config_area: Option<Rect>,
        buf: &mut ratatui::buffer::Buffer,
    ) {
        match state.tab {
            ConnectedTab::Traffic => {
                state.traffic.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.handle,
                    &state.serial_config,
                    self.focus,
                );
            }
            ConnectedTab::Graph => {
                state.graph.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.handle,
                    &state.serial_config,
                    self.focus,
                );
            }
            ConnectedTab::FileSender => {
                state.file_sender.draw(
                    main_area,
                    config_area,
                    buf,
                    &state.serial_config,
                    self.focus,
                );
            }
        }
    }

    async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                // Handle help overlay first (it captures all input when visible)
                if self.help.visible {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                            self.help.hide();
                        }
                        KeyCode::Tab | KeyCode::Char('l') => self.help.next_tab(),
                        KeyCode::BackTab | KeyCode::Char('h') => self.help.prev_tab(),
                        KeyCode::Char('j') | KeyCode::Down => {
                            self.help.scroll = self.help.scroll.saturating_add(1);
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            self.help.scroll = self.help.scroll.saturating_sub(1);
                        }
                        _ => {}
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
                        self.help.toggle();
                        return;
                    }
                    KeyCode::Char('c')
                        if !matches!(self.mode, AppMode::PreConnect(ref v) if v.is_input_mode()) =>
                    {
                        self.show_config = !self.show_config;
                        return;
                    }
                    // Ctrl+h moves focus left (to Main panel)
                    KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) && !self.is_input_mode() => {
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
                                KeyCode::Char('1') => {
                                    state.tab = ConnectedTab::Traffic;
                                    return;
                                }
                                KeyCode::Char('2') => {
                                    state.tab = ConnectedTab::Graph;
                                    return;
                                }
                                KeyCode::Char('3') => {
                                    state.tab = ConnectedTab::FileSender;
                                    return;
                                }
                                KeyCode::Char('d') => {
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
            AppEvent::Mouse(_mouse) => {
                // TODO: Mouse support
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
            PreConnectAction::Connect { port, config } => {
                self.connect(&port, config).await;
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
            }
            TrafficAction::Toast(toast) => {
                self.toasts.push(toast);
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

    async fn connect(&mut self, port: &str, serial_config: SerialConfig) {
        self.toasts.info(format!("Connecting to {}...", port));

        let session_config = SessionConfig::default().line_delimited();

        match Session::connect_with_config(port, serial_config.clone(), session_config).await {
            Ok(handle) => {
                let mut traffic = TrafficView::new();
                traffic.session_start = Some(SystemTime::now());
                
                let state = ConnectedState {
                    handle,
                    tab: ConnectedTab::Traffic,
                    traffic,
                    graph: GraphView::new(),
                    file_sender: FileSenderView::new(),
                    serial_config,
                };
                self.mode = AppMode::Connected(state);
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
    Connect { port: String, config: SerialConfig },
    Toast(crate::widget::Toast),
}

/// Actions from traffic view.
pub enum TrafficAction {
    Send(Vec<u8>),
    Toast(crate::widget::Toast),
}

/// Actions from file sender view.
pub enum FileSenderAction {
    StartSending,
    CancelSending,
    Toast(crate::widget::Toast),
}
