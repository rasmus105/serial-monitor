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
    list_ports,
};

use crate::{
    event::{AppEvent, poll_event},
    settings::TuiSettings,
    theme::Theme,
    view::{file_sender::FileSenderView, graph::GraphView, pre_connect::PreConnectView, traffic::TrafficView},
    widget::{
        CompletionKind, CompletionPopup, CompletionState, ConfirmOverlay, ConfirmState, HelpOverlay, Toasts,
        ConnectModal, ConnectModalState, ConnectModalAction,
        SessionsModal, SessionsModalState, SessionsModalAction,
        help_overlay::HelpOverlayState,
        text_input::TextInputState,
        toast::render_toasts,
    },
};

/// What kind of argument a command takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandArg {
    None,
    Path,
    SerialPort,
}

/// Metadata about a command for completion.
struct CommandInfo {
    name: &'static str,
    alias: &'static str,
    arg: CommandArg,
}

/// Available commands for completion.
const COMMANDS: &[CommandInfo] = &[
    CommandInfo { name: "connect", alias: "c", arg: CommandArg::SerialPort },
    CommandInfo { name: "disconnect", alias: "d", arg: CommandArg::None },
    CommandInfo { name: "save", alias: "w", arg: CommandArg::Path },
    CommandInfo { name: "quit", alias: "q", arg: CommandArg::None },
    CommandInfo { name: "help", alias: "h", arg: CommandArg::None },
    CommandInfo { name: "sessions", alias: "s", arg: CommandArg::None },
];

/// Main application state.
pub struct App {
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Toast notifications.
    pub toasts: Toasts,
    /// Help overlay state.
    pub help: HelpOverlayState,
    /// Confirmation dialog state.
    pub confirm: ConfirmState,
    /// Connect modal state.
    pub connect_modal: ConnectModalState,
    /// Sessions modal state.
    pub sessions_modal: SessionsModalState,
    /// Session manager for all active sessions.
    pub sessions: SessionManager,
    /// Whether the config panel is visible.
    pub show_config: bool,
    /// Current focus area.
    pub focus: Focus,
    /// Command input state (vim-like ':' command mode).
    pub command_input: TextInputState,
    /// Whether command mode is active.
    pub command_mode: bool,
    /// Command completion state.
    pub completion: CompletionState,
    /// Whether the terminal needs a full clear on next draw.
    pub needs_clear: bool,
    /// Persistent settings.
    settings: TuiSettings,
}

/// Manager for all active sessions.
pub struct SessionManager {
    /// All active sessions.
    sessions: Vec<SessionEntry>,
    /// Index of the currently active session (the one being displayed).
    active_index: Option<usize>,
    /// Counter for generating unique session IDs.
    next_id: usize,
}

/// A single session entry.
pub struct SessionEntry {
    /// Unique identifier for this session.
    pub id: usize,
    /// Session state (connected or pre-connect).
    pub state: SessionState,
}

/// State of a session.
#[allow(clippy::large_enum_variant)]
pub enum SessionState {
    /// Pre-connection state.
    PreConnect(PreConnectView),
    /// Connected state.
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

impl SessionManager {
    /// Creates a new session manager with one PreConnect session.
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_index: None,
            next_id: 0,
        }
    }

    /// Get the active session.
    pub fn active(&self) -> Option<&SessionEntry> {
        self.active_index.and_then(|i| self.sessions.get(i))
    }

    /// Get the active session mutably.
    pub fn active_mut(&mut self) -> Option<&mut SessionEntry> {
        self.active_index.and_then(|i| self.sessions.get_mut(i))
    }

    /// Get the active session's state.
    pub fn active_state(&self) -> Option<&SessionState> {
        self.active().map(|e| &e.state)
    }

    /// Get the active session's state mutably.
    pub fn active_state_mut(&mut self) -> Option<&mut SessionState> {
        self.active_mut().map(|e| &mut e.state)
    }

    /// Add a new connected session, returns its ID.
    pub fn add_connected(&mut self, state: ConnectedState) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(SessionEntry {
            id,
            state: SessionState::Connected(state),
        });
        // Switch to the new session
        self.active_index = Some(self.sessions.len() - 1);
        id
    }

    /// Add a new PreConnect session, returns its ID.
    pub fn add_preconnect(&mut self, view: PreConnectView) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(SessionEntry {
            id,
            state: SessionState::PreConnect(view),
        });
        // Switch to the new session
        self.active_index = Some(self.sessions.len() - 1);
        id
    }

    /// Switch to a session by index.
    pub fn switch_to(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_index = Some(index);
        }
    }

    /// Remove a session by index, auto-switch if removing active.
    /// Returns the removed session entry if it existed.
    pub fn remove(&mut self, index: usize) -> Option<SessionEntry> {
        if index >= self.sessions.len() {
            return None;
        }

        let entry = self.sessions.remove(index);

        // Update active index
        if self.sessions.is_empty() {
            self.active_index = None;
        } else if let Some(active) = self.active_index {
            if active == index {
                // Removed the active session - switch to nearest
                self.active_index = Some(index.min(self.sessions.len() - 1));
            } else if active > index {
                // Active session shifted down
                self.active_index = Some(active - 1);
            }
        }

        Some(entry)
    }

    /// Number of sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether there are no sessions.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Iterate over sessions mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut SessionEntry> {
        self.sessions.iter_mut()
    }

    /// Get the active index.
    pub fn active_index(&self) -> Option<usize> {
        self.active_index
    }

    /// Get a slice of all sessions.
    pub fn sessions_slice(&self) -> &[SessionEntry] {
        &self.sessions
    }

    /// Drain all sessions, returning an iterator that takes ownership.
    pub fn drain(&mut self) -> impl Iterator<Item = SessionEntry> + '_ {
        self.active_index = None;
        self.sessions.drain(..)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        // Load persistent settings
        let settings = TuiSettings::load();
        
        // Create pre-connect view with saved settings
        let mut pre_connect = PreConnectView::new();
        pre_connect.apply_settings(&settings.pre_connect);
        
        // Create help overlay with saved global settings
        let help = HelpOverlayState {
            settings: settings.global.clone().into(),
            ..Default::default()
        };
        
        // Create session manager with initial PreConnect session
        let mut sessions = SessionManager::new();
        sessions.add_preconnect(pre_connect);
        
        Self {
            should_quit: false,
            toasts: Toasts::new(),
            help,
            confirm: ConfirmState::default(),
            connect_modal: ConnectModalState::default(),
            sessions_modal: SessionsModalState::default(),
            sessions,
            show_config: true,
            focus: Focus::Main,
            command_input: TextInputState::new().with_placeholder("Enter command..."),
            command_mode: false,
            completion: CompletionState::default(),
            needs_clear: false,
            settings,
        }
    }

    /// Main event loop.
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        // Initial port scan for any PreConnect sessions
        if let Some(SessionState::PreConnect(view)) = self.sessions.active_state_mut() {
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

            // Drain all pending events before rendering next frame.
            // This prevents event queue backlog when events arrive faster than
            // we can render (e.g., fast mouse wheel scrolling).
            while let Some(event) = poll_event(Duration::from_millis(0)) {
                self.handle_event(event).await;
            }

            // Wait for next event or timeout (for session events and periodic updates)
            if let Some(event) = poll_event(Duration::from_millis(50)) {
                self.handle_event(event).await;
            }

            // Handle session events for ALL sessions (including background ones)
            self.process_session_events();

            // Tick toasts - request clear if any expired (they use Clear which leaves artifacts)
            if self.toasts.tick() {
                self.needs_clear = true;
            }

            if self.should_quit {
                // Save settings before quitting
                self.save_settings();
                
                // Cleanup - disconnect all connected sessions
                // We drain all sessions to take ownership
                let sessions: Vec<SessionEntry> = self.sessions.drain().collect();
                for entry in sessions {
                    if let SessionState::Connected(state) = entry.state {
                        let _ = state.handle.disconnect().await;
                    }
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

        // Draw based on active session state
        if let Some(entry) = self.sessions.active_mut() {
            match &mut entry.state {
                SessionState::PreConnect(view) => {
                    view.draw(main_area, config_area, buf, self.focus);
                }
                SessionState::Connected(state) => {
                    Self::draw_connected(state, main_area, config_area, buf, self.focus);
                }
            }
        }

        // Draw command bar if in command mode
        if let Some(cmd_area) = command_area {
            self.draw_command_bar(cmd_area, buf);
            
            // Draw completion popup above the command bar (needs full area for proper positioning)
            let is_disconnected = matches!(self.sessions.active_state(), Some(SessionState::PreConnect(_)) | None);
            let input_y = cmd_area.y;
            let input_x = cmd_area.x + 2; // After border + ":" prefix
            CompletionPopup::new(&self.completion, input_y, input_x)
                .disconnected(is_disconnected)
                .render(area, buf);
        }

        // Draw loading overlay (from graph view if reparsing)
        if let Some(SessionState::Connected(state)) = self.sessions.active_state_mut()
            && let Some(ref mut loading) = state.graph.loading
        {
            loading.mark_visible();
            crate::widget::LoadingOverlay::new(loading).render(area, buf);
        }

        // Draw toasts overlay
        render_toasts(&self.toasts, area, buf);

        // Draw confirmation overlay
        ConfirmOverlay::new(&self.confirm).render(area, buf);

        // Draw connect modal overlay
        ConnectModal::new(&self.connect_modal).render(area, buf);

        // Draw sessions modal overlay
        SessionsModal::new(
            &self.sessions_modal,
            self.sessions.sessions_slice(),
            self.sessions.active_index(),
        ).render(area, buf);

        // Draw help overlay
        HelpOverlay::new(&self.help).render(area, buf);
    }

    fn draw_command_bar(&self, area: Rect, buf: &mut Buffer) {
        // Use disconnected theme when not connected
        let is_disconnected = matches!(self.sessions.active_state(), Some(SessionState::PreConnect(_)) | None);
        
        let block = Block::default()
            .title(" Command ")
            .borders(Borders::ALL)
            .border_style(if is_disconnected {
                Theme::border_disconnected()
            } else {
                Theme::border_focused()
            });

        let inner = block.inner(area);
        block.render(area, buf);

        // Draw the ":" prefix and input
        let prefix = Span::styled(":", if is_disconnected {
            Theme::keybind_disconnected()
        } else {
            Theme::keybind()
        });
        let content = Span::raw(&self.command_input.content);
        let line = Line::from(vec![prefix, content]);

        Paragraph::new(line).render(inner, buf);

        // Draw cursor
        let cursor_x = inner.x + 1 + self.command_input.cursor_display_pos() as u16;
        if cursor_x < inner.x + inner.width
            && let Some(cell) = buf.cell_mut((cursor_x, inner.y))
        {
            cell.set_bg(if is_disconnected { Theme::DISCONNECTED } else { Theme::PRIMARY });
            cell.set_fg(Theme::BG);
        }
    }

    fn process_session_events(&mut self) {
        // Collect events from ALL sessions (including background ones)
        // We collect (session_index, events) pairs to handle them correctly
        let mut all_events: Vec<(usize, Vec<SessionEvent>)> = Vec::new();
        
        for (index, entry) in self.sessions.iter_mut().enumerate() {
            if let SessionState::Connected(state) = &mut entry.state {
                let mut events = Vec::new();
                while let Some(event) = state.handle.try_recv_event() {
                    events.push(event);
                }
                if !events.is_empty() {
                    all_events.push((index, events));
                }
            }
        }

        // Process events - note we need to be careful about disconnection
        // which may remove sessions
        let active_index = self.sessions.active_index();
        for (session_index, events) in all_events {
            let is_active = Some(session_index) == active_index;
            for event in events {
                self.handle_session_event(event, session_index, is_active);
            }
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
                            self.completion.hide();
                            self.execute_command(&cmd).await;
                        }
                        KeyCode::Esc => {
                            self.command_mode = false;
                            self.command_input.clear();
                            self.completion.hide();
                        }
                        KeyCode::Tab => {
                            if !self.completion.visible {
                                self.update_completions();
                            } else {
                                self.completion.next();
                            }
                            self.apply_completion();
                        }
                        KeyCode::BackTab => {
                            if self.completion.visible {
                                self.completion.prev();
                                self.apply_completion();
                            }
                        }
                        _ => {
                            self.command_input.handle_key(key);
                            self.completion.hide();
                        }
                    }
                    return;
                }

                // Handle help overlay (it captures all input when visible)
                if self.help.visible {
                    if self.help.handle_key(key) {
                        self.needs_clear = true;
                        // Sync global pattern settings to connected sessions when help is closed
                        self.sync_global_pattern_settings();
                    }
                    return;
                }

                // Handle confirmation overlay (captures all input when visible)
                if self.confirm.visible {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            self.confirm.hide();
                            self.needs_clear = true;
                            self.disconnect().await;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N')
                        | KeyCode::Char('q') | KeyCode::Char('Q')
                        | KeyCode::Esc => {
                            self.confirm.hide();
                            self.needs_clear = true;
                        }
                        _ => {}
                    }
                    return;
                }

                // Handle connect modal (captures all input when visible)
                if self.connect_modal.visible {
                    match self.connect_modal.handle_key(key) {
                        ConnectModalAction::Cancel => {
                            self.connect_modal.hide();
                            self.needs_clear = true;
                        }
                        ConnectModalAction::Connect => {
                            let port_path = self.connect_modal.port_path.clone();
                            let serial_config = self.connect_modal.config.to_serial_config();
                            let rx_chunking = self.connect_modal.config.rx_chunking();
                            let file_save_enabled = self.connect_modal.config.file_save_enabled;
                            let file_save_format_index = self.connect_modal.config.file_save_format_index;
                            let file_save_encoding_index = self.connect_modal.config.file_save_encoding_index;
                            let file_save_directory = self.connect_modal.config.file_save_directory.clone();
                            self.connect_modal.hide();
                            self.needs_clear = true;
                            
                            // Build session config from global settings
                            let settings = &self.help.settings;
                            let session_config = SessionConfig {
                                rx_chunking,
                                tx_chunking: ChunkingStrategy::Raw,
                                buffer_size: settings.buffer_size(),
                                auto_save: settings.to_auto_save_config(),
                            };
                            self.connect(
                                &port_path,
                                serial_config,
                                session_config,
                                settings.keep_awake,
                                file_save_enabled,
                                file_save_format_index,
                                file_save_encoding_index,
                                file_save_directory,
                            ).await;
                        }
                        ConnectModalAction::None => {}
                    }
                    return;
                }

                // Handle sessions modal (captures all input when visible)
                if self.sessions_modal.visible {
                    let session_count = self.sessions.len();
                    match self.sessions_modal.handle_key(key, session_count) {
                        SessionsModalAction::Close => {
                            self.sessions_modal.hide();
                            self.needs_clear = true;
                        }
                        SessionsModalAction::SwitchTo(index) => {
                            self.sessions.switch_to(index);
                            self.sessions_modal.hide();
                            self.needs_clear = true;
                        }
                        SessionsModalAction::ConfirmDisconnect(index) => {
                            // Disconnect the session at the given index
                            self.disconnect_session(index).await;
                            // Update selection if needed
                            if self.sessions_modal.selected >= self.sessions.len() {
                                self.sessions_modal.selected = self.sessions.len().saturating_sub(1);
                            }
                            // Close modal if no sessions left
                            if self.sessions.is_empty() {
                                self.sessions_modal.hide();
                            }
                            self.needs_clear = true;
                        }
                        SessionsModalAction::None => {}
                    }
                    return;
                }

                // Global keybindings
                match key.code {
                    KeyCode::Char('q') => {
                        // When connected, show confirmation prompt instead of quitting
                        if matches!(self.sessions.active_state(), Some(SessionState::Connected(_))) {
                            self.confirm.show("Disconnect from port?");
                        } else {
                            self.should_quit = true;
                        }
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
                        // If hiding config panel and focus was on Config, move focus to Main
                        if !self.show_config && self.focus == Focus::Config {
                            self.focus = Focus::Main;
                        }
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
                        if let Some(SessionState::Connected(state)) = self.sessions.active_state_mut() {
                            state.traffic.config_nav.close_dropdown();
                            state.graph.config_nav.close_dropdown();
                            state.file_sender.config_nav.close_dropdown();
                        } else if let Some(SessionState::PreConnect(view)) = self.sessions.active_state_mut() {
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

                // Session-specific handling
                if let Some(entry) = self.sessions.active_mut() {
                    match &mut entry.state {
                        SessionState::PreConnect(view) => {
                            if let Some(action) = view.handle_key(key, self.focus) {
                                self.handle_preconnect_action(action).await;
                            }
                        }
                        SessionState::Connected(state) => {
                            // Tab switching
                            let is_input_mode = match state.tab {
                                ConnectedTab::Traffic => state.traffic.is_input_mode(),
                                ConnectedTab::Graph => state.graph.is_input_mode(),
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
                                        self.confirm.show("Disconnect from port?");
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
                                    if let Some(action) = state.graph.handle_key(key, self.focus, &state.handle) {
                                        self.handle_graph_action(action);
                                    }
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
            }
            AppEvent::Mouse(_) => {
                // Mouse events are ignored - native terminal selection works
            }
            AppEvent::Resize(_, _) => {
                // Terminal will redraw automatically
            }
            AppEvent::Tick => {
                // Update file sender progress if active
                let action = if let Some(SessionState::Connected(state)) =
                    self.sessions.active_state_mut()
                {
                    let action = state.file_sender.tick();
                    // Dismiss loading overlay if it can be dismissed
                    state.graph.dismiss_loading_if_ready();
                    action
                } else {
                    None
                };
                if let Some(action) = action {
                    self.handle_file_sender_action(action).await;
                }
            }
        }
    }

    async fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "connect" | "c" => {
                if parts.len() < 2 {
                    self.toasts.error("Usage: :connect <port_path>");
                    return;
                }
                let port_path = parts[1].to_string();
                // Show the connect modal instead of connecting directly
                self.connect_modal.show(port_path);
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
                self.help.visible = true;
                self.help.tab = crate::widget::help_overlay::HelpTab::Commands;
            }
            "sessions" | "s" => {
                self.sessions_modal.show();
            }
            _ => {
                self.toasts.error(format!("Unknown command: {}", parts[0]));
            }
        }
    }

    async fn save_buffer(&mut self, path: &str) {
        if let Some(SessionState::Connected(state)) = self.sessions.active_state() {
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

    fn handle_session_event(&mut self, event: SessionEvent, session_index: usize, is_active: bool) {
        match event {
            SessionEvent::Connected => {
                if is_active {
                    self.toasts.success("Connected");
                }
            }
            SessionEvent::Disconnected { error } => {
                if is_active {
                    if let Some(err) = error {
                        self.toasts.error(format!("Disconnected: {}", err));
                    } else {
                        self.toasts.info("Disconnected");
                    }
                }
                // Remove the disconnected session
                self.sessions.remove(session_index);
                
                // If no sessions left, create a fresh PreConnect session
                if self.sessions.is_empty() {
                    let mut view = PreConnectView::new();
                    view.refresh_ports();
                    self.sessions.add_preconnect(view);
                }
                self.needs_clear = true;
            }
            SessionEvent::Error(msg) => {
                if is_active {
                    self.toasts.error(msg);
                }
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
                // Save pre-connect settings before transitioning to connected state
                // This ensures settings are preserved even if we quit while connected
                if let Some(SessionState::PreConnect(view)) = self.sessions.active_state() {
                    self.settings.pre_connect = view.to_settings();
                }
                
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
                if let Some(SessionState::Connected(state)) = self.sessions.active_state()
                    && let Err(e) = state.handle.send(data).await
                {
                    self.toasts.error(format!("Send failed: {}", e));
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
                if let Some(SessionState::Connected(state)) = self.sessions.active_state_mut() {
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
                if let Some(SessionState::Connected(state)) = self.sessions.active_state() {
                    state.handle.buffer_mut().stop_saving();
                    self.toasts.info("File saving stopped");
                }
            }
        }
    }

    async fn handle_file_sender_action(&mut self, action: FileSenderAction) {
        match action {
            FileSenderAction::StartSending => {
                if let Some(SessionState::Connected(state)) = self.sessions.active_state_mut() {
                    if let Err(e) = state.file_sender.start_sending(&state.handle).await {
                        self.toasts.error(format!("Failed to start sending: {}", e));
                    } else {
                        self.toasts.info("File sending started");
                    }
                }
            }
            FileSenderAction::CancelSending => {
                if let Some(SessionState::Connected(state)) = self.sessions.active_state_mut() {
                    state.file_sender.cancel_sending();
                    self.toasts.info("File sending cancelled");
                }
            }
            FileSenderAction::Toast(toast) => {
                self.toasts.push(toast);
            }
        }
    }

    fn handle_graph_action(&mut self, action: GraphAction) {
        match action {
            GraphAction::Toast(toast) => {
                self.toasts.push(toast);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
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
                
                // Apply saved settings to traffic view first
                traffic.apply_settings(&self.settings.traffic);
                // Override file save settings with what was configured in pre-connect
                traffic.config.file_save_enabled = file_save_enabled;
                traffic.config.file_save_format_index = file_save_format_index;
                traffic.config.file_save_encoding_index = file_save_encoding_index;
                traffic.config.file_save_directory = file_save_directory.clone();
                // Apply global pattern matching settings (these override traffic settings)
                traffic.config.search_mode_index = self.help.settings.search_mode_index;
                traffic.config.filter_mode_index = self.help.settings.filter_mode_index;
                
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
                
                // Create and configure graph view
                let mut graph = GraphView::new();
                graph.apply_settings(&self.settings.graph);
                
                // Create and configure file sender view
                let mut file_sender = FileSenderView::new();
                file_sender.apply_settings(&self.settings.file_sender);
                
                let state = ConnectedState {
                    handle,
                    tab: ConnectedTab::Traffic,
                    traffic,
                    graph,
                    file_sender,
                    serial_config,
                    keep_awake: keep_awake_handle,
                };
                
                // Remove the current PreConnect session (if any) and add the connected session
                // This replaces the current session rather than adding a new one
                if let Some(active_idx) = self.sessions.active_index()
                    && matches!(self.sessions.active_state(), Some(SessionState::PreConnect(_)))
                {
                    self.sessions.remove(active_idx);
                }
                self.sessions.add_connected(state);
                self.needs_clear = true;
            }
            Err(e) => {
                self.toasts.error(format!("Connection failed: {}", e));
            }
        }
    }

    async fn disconnect(&mut self) {
        // Get the current active index
        if let Some(active_idx) = self.sessions.active_index() {
            // Remove the active session (this will disconnect it)
            if let Some(entry) = self.sessions.remove(active_idx)
                && let SessionState::Connected(state) = entry.state
            {
                let _ = state.handle.disconnect().await;
                self.toasts.info("Disconnected");
            }
        }
        
        // If no sessions left, create a fresh PreConnect session
        if self.sessions.is_empty() {
            let mut view = PreConnectView::new();
            view.refresh_ports();
            self.sessions.add_preconnect(view);
        }
        
        self.needs_clear = true;
    }

    /// Disconnect a specific session by index.
    async fn disconnect_session(&mut self, index: usize) {
        if let Some(entry) = self.sessions.remove(index)
            && let SessionState::Connected(state) = entry.state
        {
            let _ = state.handle.disconnect().await;
            self.toasts.info("Session disconnected");
        }
        
        // If no sessions left, create a fresh PreConnect session
        if self.sessions.is_empty() {
            let mut view = PreConnectView::new();
            view.refresh_ports();
            self.sessions.add_preconnect(view);
        }
    }

    fn is_input_mode(&self) -> bool {
        if self.command_mode {
            return true;
        }
        match self.sessions.active_state() {
            Some(SessionState::PreConnect(view)) => view.is_input_mode(),
            Some(SessionState::Connected(state)) => match state.tab {
                ConnectedTab::Traffic => state.traffic.is_input_mode(),
                ConnectedTab::Graph => state.graph.is_input_mode(),
                ConnectedTab::FileSender => state.file_sender.is_input_mode(),
            },
            None => false,
        }
    }

    /// Sync global pattern matching settings to all connected sessions.
    fn sync_global_pattern_settings(&mut self) {
        let search_mode = self.help.settings.search_mode_index;
        let filter_mode = self.help.settings.filter_mode_index;
        
        for entry in self.sessions.iter_mut() {
            if let SessionState::Connected(state) = &mut entry.state {
                state.traffic.config.search_mode_index = search_mode;
                state.traffic.config.filter_mode_index = filter_mode;
            }
        }
    }

    /// Update completion options based on current command input.
    fn update_completions(&mut self) {
        use crate::widget::text_input::find_path_completions;

        let input = &self.command_input.content;
        let trimmed = input.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        // Determine if we're completing a command name or an argument
        // We're completing a command if:
        // 1. No input at all, OR
        // 2. Only one word and no trailing space (still typing the command)
        let completing_command = parts.is_empty() || (parts.len() == 1 && !input.ends_with(' '));

        let (completions, kind) = if completing_command {
            // Completing command name
            let prefix = parts.first().copied().unwrap_or("");
            let options: Vec<String> = COMMANDS
                .iter()
                .filter(|cmd| cmd.name.starts_with(prefix) || cmd.alias.starts_with(prefix))
                .map(|cmd| cmd.name.to_string())
                .collect();
            (options, CompletionKind::Command)
        } else {
            // We have a command, now complete its argument
            let cmd_name = parts[0];
            let cmd_info = COMMANDS.iter().find(|c| c.name == cmd_name || c.alias == cmd_name);

            let options = if let Some(info) = cmd_info {
                // Get the argument prefix (everything after the command)
                let arg_prefix = parts.get(1).copied().unwrap_or("");
                
                match info.arg {
                    CommandArg::Path => {
                        find_path_completions(arg_prefix)
                    }
                    CommandArg::SerialPort => {
                        list_ports()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|p| {
                                // Match if full path starts with prefix OR
                                // if the port name (filename part) contains the prefix
                                p.name.starts_with(arg_prefix) || 
                                p.name.rsplit('/').next()
                                    .map(|filename| filename.starts_with(arg_prefix))
                                    .unwrap_or(false)
                            })
                            .map(|p| p.name)
                            .collect()
                    }
                    CommandArg::None => Vec::new(),
                }
            } else {
                // Unknown command, no completions
                Vec::new()
            };
            (options, CompletionKind::Argument)
        };

        self.completion.show(completions, kind);
    }

    /// Apply the selected completion to the command input.
    fn apply_completion(&mut self) {
        if let Some(value) = self.completion.selected_value() {
            let input = &self.command_input.content;
            let parts: Vec<&str> = input.split_whitespace().collect();

            // Use the stored kind to determine how to apply the completion
            let new_content = match self.completion.kind {
                CompletionKind::Command => {
                    // Completing a command - replace with command + space
                    format!("{} ", value)
                }
                CompletionKind::Argument => {
                    // Completing an argument - keep the command, replace the argument
                    let cmd = parts.first().copied().unwrap_or("");
                    format!("{} {}", cmd, value)
                }
            };

            self.command_input.set_content(new_content);
        }
    }
    
    /// Collect and save all settings.
    fn save_settings(&mut self) {
        // Collect settings from current view mode
        // Note: We only update settings for views that exist in the current mode.
        // Settings for other views are preserved from the last time they were active.
        match self.sessions.active_state() {
            Some(SessionState::PreConnect(view)) => {
                self.settings.pre_connect = view.to_settings();
            }
            Some(SessionState::Connected(state)) => {
                self.settings.traffic = state.traffic.to_settings();
                self.settings.graph = state.graph.to_settings();
                self.settings.file_sender = state.file_sender.to_settings();
            }
            None => {}
        }
        
        // Collect global settings from help overlay
        self.settings.global = (&self.help.settings).into();
        
        // Save to file
        if let Err(e) = self.settings.save() {
            // Don't show toast since we're quitting, but log to stderr
            eprintln!("Warning: Failed to save settings: {}", e);
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

/// Actions from graph view.
pub enum GraphAction {
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
