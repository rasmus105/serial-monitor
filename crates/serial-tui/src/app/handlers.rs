//! Key event handlers for the application.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serial_core::{encode, PatternMode};

use crate::command::{
    map_global_nav_key, DropdownCommand, GlobalNavCommand, PortSelectCommand, TrafficCommand,
};
use crate::settings::{key_event_to_binding, map_settings_key, GeneralSetting, SettingsCommand};

use super::state::TextInputResult;
use super::types::{
    EnumNavigation, InputMode, PaneContent, PaneFocus, PortSelectFocus, TrafficConfigField,
    TrafficFocus, View,
};
use super::App;

/// Approximate visible height for config panel scroll calculations.
/// The actual height is set during rendering, but this provides a reasonable default.
const CONFIG_VISIBLE_HEIGHT: usize = 15;

/// Action to perform after text input submission
enum TextInputAction {
    /// Connect to a port
    ConnectToPort,
    /// Execute a command
    ExecuteCommand,
    /// Send a file
    SendFile,
    /// Apply port config text value
    ApplyPortConfig,
    /// Apply traffic config text value
    ApplyTrafficConfig,
    /// Apply graph config text value
    ApplyGraphConfig,
}

/// Type of dropdown being handled
enum DropdownType {
    /// Port configuration dropdown
    PortConfig,
    /// Settings panel dropdown (pattern mode)
    Settings,
    /// Traffic configuration dropdown
    TrafficConfig,
    /// Graph configuration dropdown
    GraphConfig,
}

/// Result of dropdown navigation
pub(super) enum DropdownResult {
    /// Navigation handled (up/down), stay in dropdown mode
    Navigated,
    /// User confirmed selection
    Confirmed,
    /// User cancelled
    Cancelled,
    /// Key not handled by dropdown
    NotHandled,
}

/// Handle dropdown navigation for any dropdown.
/// This is a free function to avoid borrow checker issues with &mut self.
pub(super) fn handle_dropdown_key(
    key: KeyEvent,
    options_count: usize,
    dropdown_index: &mut usize,
    dropdown_bindings: &crate::settings::CommandBindings<DropdownCommand>,
) -> DropdownResult {
    // First try global navigation commands
    if let Some(nav_cmd) = map_global_nav_key(&key) {
        match nav_cmd {
            GlobalNavCommand::Up => {
                if *dropdown_index > 0 {
                    *dropdown_index -= 1;
                }
                return DropdownResult::Navigated;
            }
            GlobalNavCommand::Down => {
                if *dropdown_index < options_count.saturating_sub(1) {
                    *dropdown_index += 1;
                }
                return DropdownResult::Navigated;
            }
            GlobalNavCommand::Confirm => {
                return DropdownResult::Confirmed;
            }
            GlobalNavCommand::Cancel => {
                return DropdownResult::Cancelled;
            }
            _ => {}
        }
    }

    // Fall back to dropdown-specific bindings
    if let Some(cmd) = dropdown_bindings.find_command(&key) {
        match cmd {
            DropdownCommand::Confirm => return DropdownResult::Confirmed,
            DropdownCommand::Cancel => return DropdownResult::Cancelled,
        }
    }

    DropdownResult::NotHandled
}

impl App {
    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Settings dropdown takes priority when open (even over settings panel)
        if self.input.mode == InputMode::SettingsDropdown {
            self.handle_key_settings_dropdown(key);
            return;
        }

        // Settings panel takes priority when open
        if self.settings_panel.open {
            self.handle_key_settings(key);
            return;
        }

        // Check for settings toggle key (? works everywhere)
        if key.code == KeyCode::Char('?') {
            self.settings_panel.open();
            self.needs_full_clear = true;
            return;
        }

        match self.input.mode {
            InputMode::Normal => match self.view {
                View::PortSelect => self.handle_key_port_select(key),
                View::Connected => self.handle_key_connected(key),
            },
            InputMode::PortInput => self.handle_key_port_input(key),
            InputMode::SendInput => self.handle_key_send_input(key),
            InputMode::SearchInput => self.handle_key_search_input(key),
            InputMode::FilePathInput => self.handle_key_file_path_input(key),
            InputMode::ConfigDropdown => self.handle_key_config_dropdown(key),
            InputMode::TrafficConfigDropdown => self.handle_key_traffic_config_dropdown(key),
            InputMode::GraphConfigDropdown => self.handle_key_graph_config_dropdown(key),
            InputMode::SettingsDropdown => self.handle_key_settings_dropdown(key),
            InputMode::WindowCommand => self.handle_key_window_command(key),
            InputMode::CommandLine => self.handle_key_command_line(key),
            InputMode::SplitSelect => self.handle_key_split_select(key),
            InputMode::ConfigTextInput => self.handle_key_config_text_input(key),
            InputMode::TrafficConfigTextInput => self.handle_key_traffic_config_text_input(key),
            InputMode::GraphConfigTextInput => self.handle_key_graph_config_text_input(key),
        }
    }

    pub(super) fn handle_key_settings(&mut self, key: KeyEvent) {
        // If recording a key binding, capture the key
        if self.settings_panel.recording_key {
            // Escape cancels recording
            if key.code == KeyCode::Esc {
                self.settings_panel.stop_recording();
                self.status = "Key binding cancelled.".to_string();
                return;
            }

            // Record the binding
            let binding = key_event_to_binding(&key);
            if let Some(cmd) = self.settings_panel.selected_any_command() {
                if let Some(edit_idx) = self.settings_panel.editing_binding_index {
                    // Replace existing binding
                    let mut bindings = self.settings.get_bindings(cmd);
                    if edit_idx < bindings.len() {
                        bindings[edit_idx] = binding;
                        self.settings.set_bindings(cmd, bindings);
                    }
                } else {
                    // Add new binding
                    self.settings.add_binding(cmd, binding);
                }
                self.status = format!("Added binding: {}", binding.display());
            }
            self.settings_panel.stop_recording();
            return;
        }

        // Use visible height for scroll calculations (approximate, will be set properly by render)
        let visible_height = self.settings_panel_visible_height();

        // Handle General tab separately - it has simpler controls
        if self.settings_panel.tab == crate::settings::SettingsTab::General {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.settings_panel.close();
                    self.needs_full_clear = true;
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    // Open dropdown for the selected setting
                    match self.settings_panel.selected_general_setting {
                        GeneralSetting::SearchMode => {
                            self.settings_panel.dropdown_index = match self.search.mode() {
                                PatternMode::Regex => 0,
                                PatternMode::Normal => 1,
                            };
                        }
                        GeneralSetting::FilterMode => {
                            self.settings_panel.dropdown_index = match self.traffic.filter.mode() {
                                PatternMode::Regex => 0,
                                PatternMode::Normal => 1,
                            };
                        }
                    }
                    self.input.mode = InputMode::SettingsDropdown;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.settings_panel.selected_general_setting =
                        self.settings_panel.selected_general_setting.next();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.settings_panel.selected_general_setting =
                        self.settings_panel.selected_general_setting.prev();
                }
                KeyCode::Char('l') | KeyCode::Tab => {
                    self.settings_panel.tab = self.settings_panel.tab.next();
                }
                KeyCode::Char('h') | KeyCode::BackTab => {
                    self.settings_panel.tab = self.settings_panel.tab.prev();
                }
                _ => {}
            }
            return;
        }

        // First check for global navigation commands (j/k, Ctrl+u/d, etc.)
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    self.settings_panel.move_up(visible_height);
                    return;
                }
                GlobalNavCommand::Down => {
                    self.settings_panel.move_down(visible_height);
                    return;
                }
                GlobalNavCommand::PageUp => {
                    self.settings_panel.page_up(visible_height);
                    return;
                }
                GlobalNavCommand::PageDown => {
                    self.settings_panel.page_down(visible_height);
                    return;
                }
                GlobalNavCommand::Confirm => {
                    // Start recording to add a binding
                    self.settings_panel.start_recording();
                    self.status = "Press a key to add binding (Esc to cancel)...".to_string();
                    return;
                }
                GlobalNavCommand::Cancel => {
                    self.settings_panel.close();
                    self.needs_full_clear = true;
                    return;
                }
                GlobalNavCommand::Top => {
                    self.settings_panel.go_to_top();
                    return;
                }
                GlobalNavCommand::Bottom => {
                    self.settings_panel.go_to_bottom(visible_height);
                    return;
                }
            }
        }

        // Then check for settings-specific commands
        let Some(cmd) = map_settings_key(&key) else {
            return;
        };

        match cmd {
            SettingsCommand::Close => {
                self.settings_panel.close();
                self.needs_full_clear = true;
            }
            SettingsCommand::NextTab => {
                self.settings_panel.tab = self.settings_panel.tab.next();
            }
            SettingsCommand::PrevTab => {
                self.settings_panel.tab = self.settings_panel.tab.prev();
            }
            // Navigation is handled by global commands above, but keep as fallback
            SettingsCommand::MoveUp => {
                self.settings_panel.move_up(visible_height);
            }
            SettingsCommand::MoveDown => {
                self.settings_panel.move_down(visible_height);
            }
            SettingsCommand::AddBinding => {
                self.settings_panel.start_recording();
                self.status = "Press a key to add binding (Esc to cancel)...".to_string();
            }
            SettingsCommand::DeleteBinding => {
                // Delete the last binding for the selected command
                if let Some(cmd) = self.settings_panel.selected_any_command() {
                    let bindings = self.settings.get_bindings(cmd);
                    if !bindings.is_empty() {
                        let last = *bindings.last().unwrap();
                        self.settings.remove_binding(cmd, &last);
                        self.status = format!("Removed binding: {}", last.display());
                    } else {
                        self.status = "No bindings to remove.".to_string();
                    }
                }
            }
            SettingsCommand::ResetToDefault => {
                if let Some(cmd) = self.settings_panel.selected_any_command() {
                    self.settings.reset_command(cmd);
                    self.status = format!("Reset {} to defaults.", cmd.name());
                }
            }
            SettingsCommand::Confirm => {
                // Start recording to replace/add a binding
                self.settings_panel.start_recording();
                self.status = "Press a key to add binding (Esc to cancel)...".to_string();
            }
        }
    }

    /// Get approximate visible height for settings panel
    /// This is used for scroll calculations before rendering
    fn settings_panel_visible_height(&self) -> usize {
        // Approximate: 80% of terminal height minus borders/tabs/help
        // A more accurate value gets set during rendering
        20
    }

    pub(super) fn handle_key_port_select(&mut self, key: KeyEvent) {
        // First check global navigation commands
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    match self.port_select.focus {
                        PortSelectFocus::PortList => {
                            if self.port_select.selected_port > 0 {
                                self.port_select.selected_port -= 1;
                            }
                        }
                        PortSelectFocus::Config => {
                            self.port_select.config.field = self.port_select.config.field.prev();
                            self.port_select.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                        }
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    match self.port_select.focus {
                        PortSelectFocus::PortList => {
                            if !self.port_select.ports.is_empty()
                                && self.port_select.selected_port < self.port_select.ports.len() - 1
                            {
                                self.port_select.selected_port += 1;
                            }
                        }
                        PortSelectFocus::Config => {
                            self.port_select.config.field = self.port_select.config.field.next();
                            self.port_select.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                        }
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    match self.port_select.focus {
                        PortSelectFocus::PortList => {
                            if !self.port_select.ports.is_empty() {
                                self.connect_to_selected_port();
                            }
                        }
                        PortSelectFocus::Config => {
                            self.confirm_port_config_field();
                        }
                    }
                    return;
                }
                // PageUp/PageDown/Top/Bottom/Cancel not used in port select
                _ => {}
            }
        }

        // Then check context-specific commands
        let cmd = self.settings.keybindings.port_select.find_command(&key);

        // Handle context-sensitive commands
        let cmd = match cmd {
            Some(PortSelectCommand::FocusPortList) if !self.port_select.config.visible => None,
            Some(PortSelectCommand::FocusConfig) if !self.port_select.config.visible => None,
            other => other,
        };

        let Some(cmd) = cmd else {
            // Check for command line entry with ':'
            if key.code == KeyCode::Char(':') && key.modifiers.is_empty() {
                self.enter_input_mode(InputMode::CommandLine);
                return;
            }
            return;
        };

        match cmd {
            PortSelectCommand::Quit => self.should_quit = true,
            PortSelectCommand::RefreshPorts => self.refresh_ports(),
            PortSelectCommand::EnterPortPath => {
                self.enter_input_mode(InputMode::PortInput);
            }
            PortSelectCommand::ToggleConfigPanel => {
                self.port_select.config.visible = !self.port_select.config.visible;
            }
            PortSelectCommand::FocusPortList => {
                self.port_select.focus = PortSelectFocus::PortList;
            }
            PortSelectCommand::FocusConfig => {
                self.port_select.focus = PortSelectFocus::Config;
            }
            PortSelectCommand::Confirm => {
                // Handled by global nav above
            }
        }
    }

    pub(super) fn handle_key_config_dropdown(&mut self, key: KeyEvent) {
        self.handle_dropdown(key, DropdownType::PortConfig);
    }

    pub(super) fn handle_key_settings_dropdown(&mut self, key: KeyEvent) {
        self.handle_dropdown(key, DropdownType::Settings);
    }

    /// Apply the settings dropdown selection to the appropriate setting
    fn apply_settings_dropdown_selection(&mut self) {
        match self.settings_panel.selected_general_setting {
            GeneralSetting::SearchMode => {
                let mode = match self.settings_panel.dropdown_index {
                    0 => PatternMode::Regex,
                    _ => PatternMode::Normal,
                };
                // Update search mode
                if let Err(e) = self.search.set_mode(mode) {
                    self.status = e;
                    return;
                }
                self.status = format!("Search mode: {}", self.search.mode().name());
                // Re-run search if there's an active pattern
                if self.search.has_pattern() {
                    self.update_search_matches();
                }
            }
            GeneralSetting::FilterMode => {
                let mode = match self.settings_panel.dropdown_index {
                    0 => PatternMode::Regex,
                    _ => PatternMode::Normal,
                };
                // Update filter mode through PatternMatcher
                if let Some(pattern) = self.traffic.filter.pattern().map(String::from) {
                    if let Err(e) = self.traffic.filter.set_pattern(&pattern, mode) {
                        self.status = e;
                        return;
                    }
                } else {
                    // No pattern set yet, just update the mode for future patterns
                    let _ = self.traffic.filter.set_pattern("", mode);
                }
                self.status = format!("Filter mode: {}", self.traffic.filter.mode().name());
            }
        }
    }

    pub(super) fn handle_key_port_input(&mut self, key: KeyEvent) {
        self.handle_simple_text_input(key, TextInputAction::ConnectToPort, "Cancelled.");
    }

    pub(super) fn handle_key_traffic(&mut self, key: KeyEvent) {
        // Handle quit confirmation dialog first
        if self.traffic.quit_confirm {
            self.handle_key_quit_confirm(key);
            return;
        }

        let config_visible = self.traffic.config.visible;
        let config_focused = self.traffic.focus == TrafficFocus::Config;

        // First check global navigation commands (j/k, Ctrl+u/d, g, G, etc.)
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    if config_focused {
                        // Move up in config panel
                        self.traffic.config.field = self.traffic.config.field.prev();
                        self.traffic.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                    } else {
                        // Scroll up in traffic
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_sub(1);
                    }
                    return;
                }
                GlobalNavCommand::Down => {
                    if config_focused {
                        // Move down in config panel
                        self.traffic.config.field = self.traffic.config.field.next();
                        self.traffic.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                    } else {
                        // Scroll down in traffic
                        self.traffic.scroll_offset = self.traffic.scroll_offset.saturating_add(1);
                    }
                    return;
                }
                GlobalNavCommand::Top => {
                    if !config_focused {
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset = 0;
                    }
                    return;
                }
                GlobalNavCommand::Bottom => {
                    if !config_focused {
                        self.traffic.was_at_bottom = true;
                        self.traffic.scroll_offset = usize::MAX;
                    }
                    return;
                }
                GlobalNavCommand::PageUp => {
                    if !config_focused {
                        self.traffic.was_at_bottom = false;
                        self.traffic.scroll_offset =
                            self.traffic.scroll_offset.saturating_sub(self.page_size());
                    }
                    return;
                }
                GlobalNavCommand::PageDown => {
                    if !config_focused {
                        self.traffic.scroll_offset =
                            self.traffic.scroll_offset.saturating_add(self.page_size());
                    }
                    return;
                }
                GlobalNavCommand::Confirm => {
                    if config_focused {
                        self.confirm_traffic_config_field();
                    }
                    return;
                }
                GlobalNavCommand::Cancel => {
                    if config_focused {
                        // When config panel is focused, Esc returns focus to traffic
                        self.traffic.focus = TrafficFocus::Traffic;
                    } else if self.search.has_pattern() {
                        self.search.clear();
                        self.status = "Search cleared.".to_string();
                    }
                    // If nothing to cancel, Esc does nothing (use 'q' to disconnect)
                    return;
                }
            }
        }

        // Then check context-specific traffic commands
        let cmd = self.settings.keybindings.traffic.find_command(&key);

        // Handle context-sensitive commands
        let cmd = match cmd {
            Some(TrafficCommand::FocusConfig) if !config_visible => None,
            other => other,
        };

        let Some(cmd) = cmd else { return };

        match cmd {
            TrafficCommand::Disconnect => {
                self.traffic.quit_confirm = true;
            }
            TrafficCommand::CycleEncoding => {
                self.traffic.encoding = self.traffic.encoding.cycle_next();
                self.status = format!("Encoding: {}", self.traffic.encoding);
                self.needs_full_clear = true;
                // Invalidate and re-search when encoding changes
                if self.search.has_pattern() {
                    self.search.invalidate();
                    self.update_search_matches();
                }
            }
            TrafficCommand::EnterSendMode => {
                self.enter_input_mode(InputMode::SendInput);
            }
            TrafficCommand::EnterSearchMode => {
                self.enter_input_mode(InputMode::SearchInput);
            }
            TrafficCommand::NextMatch => {
                self.goto_next_match();
            }
            TrafficCommand::PrevMatch => {
                self.goto_prev_match();
            }
            TrafficCommand::ToggleFileSend => {
                if self.file_send.handle.is_some() {
                    self.cancel_file_send();
                } else {
                    self.enter_input_mode(InputMode::FilePathInput);
                }
            }
            TrafficCommand::ToggleConfigPanel => {
                self.traffic.config.visible = !self.traffic.config.visible;
                if self.traffic.config.visible {
                    // Focus the config panel when opening it
                    self.traffic.focus = TrafficFocus::Config;
                } else {
                    self.traffic.focus = TrafficFocus::Traffic;
                }
                self.needs_full_clear = true;
            }
            TrafficCommand::FocusTraffic => {
                self.traffic.focus = TrafficFocus::Traffic;
            }
            TrafficCommand::FocusConfig => {
                if self.traffic.config.visible {
                    self.traffic.focus = TrafficFocus::Config;
                }
            }
            TrafficCommand::ToggleLineNumbers => {
                self.traffic.show_line_numbers = !self.traffic.show_line_numbers;
                self.status = if self.traffic.show_line_numbers {
                    "Line numbers: ON".to_string()
                } else {
                    "Line numbers: OFF".to_string()
                };
            }
            TrafficCommand::ToggleTimestamps => {
                self.traffic.show_timestamps = !self.traffic.show_timestamps;
                self.status = if self.traffic.show_timestamps {
                    "Timestamps: ON".to_string()
                } else {
                    "Timestamps: OFF".to_string()
                };
            }
        }
    }

    /// Handle key events in the connected view
    pub(super) fn handle_key_connected(&mut self, key: KeyEvent) {
        // Handle quit confirmation dialog first
        if self.traffic.quit_confirm {
            self.handle_key_quit_confirm(key);
            return;
        }

        // Check for Ctrl+w prefix for window commands
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
            self.input.mode = InputMode::WindowCommand;
            self.status = "Ctrl+W: v=vsplit, q=close, h/l=navigate".to_string();
            return;
        }

        // Check for command line entry
        if key.modifiers.is_empty() && key.code == KeyCode::Char(':') {
            self.enter_input_mode(InputMode::CommandLine);
            return;
        }

        // Tab switching with number keys (1, 2, 3) - switches to that tab
        if key.modifiers.is_empty() {
            match key.code {
                KeyCode::Char('1') => {
                    self.layout.switch_tab(1);
                    self.needs_full_clear = true;
                    self.status = "Tab 1: Traffic".to_string();
                    return;
                }
                KeyCode::Char('2') => {
                    self.layout.switch_tab(2);
                    self.needs_full_clear = true;
                    self.status = "Tab 2: Graph".to_string();
                    return;
                }
                KeyCode::Char('3') => {
                    self.layout.switch_tab(3);
                    self.needs_full_clear = true;
                    self.status = "Tab 3: Send".to_string();
                    return;
                }
                // h/l for pane navigation (includes config panel)
                KeyCode::Char('h') => {
                    if self.navigate_focus_left() {
                        self.update_focus_status();
                        return;
                    }
                }
                KeyCode::Char('l') => {
                    if self.navigate_focus_right() {
                        self.update_focus_status();
                        return;
                    }
                }
                _ => {}
            }
        }

        // Tab key cycles focus within the current tab
        if key.code == KeyCode::Tab && key.modifiers.is_empty() {
            self.layout.toggle_focus();
            self.update_focus_status();
            return;
        }

        // If config panel is focused, handle config navigation first
        // This takes priority over pane-specific handlers
        if self.traffic.config.visible
            && self.traffic.focus == TrafficFocus::Config
            && self.handle_key_config_panel(key)
        {
            return;
        }

        // Delegate to content-specific handler based on focused pane
        match self.layout.focused_content() {
            PaneContent::Traffic => self.handle_key_traffic(key),
            PaneContent::Graph => self.handle_key_graph(key),
            PaneContent::AdvancedSend => self.handle_key_advanced_send(key),
        }
    }

    /// Handle key events when config panel is focused (works from any pane)
    /// Returns true if the key was handled
    fn handle_key_config_panel(&mut self, key: KeyEvent) -> bool {
        // Global navigation for config panel
        if let Some(nav_cmd) = map_global_nav_key(&key) {
            match nav_cmd {
                GlobalNavCommand::Up => {
                    self.traffic.config.field = self.traffic.config.field.prev();
                    self.traffic.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                    return true;
                }
                GlobalNavCommand::Down => {
                    self.traffic.config.field = self.traffic.config.field.next();
                    self.traffic.config.adjust_scroll(CONFIG_VISIBLE_HEIGHT);
                    return true;
                }
                GlobalNavCommand::Confirm => {
                    self.confirm_traffic_config_field();
                    return true;
                }
                GlobalNavCommand::Cancel => {
                    // Return focus to the pane
                    self.traffic.focus = TrafficFocus::Traffic;
                    self.update_focus_status();
                    return true;
                }
                // PageUp/PageDown/Top/Bottom not used in config panel
                _ => {}
            }
        }

        // 'c' closes config panel
        if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
            self.traffic.config.visible = false;
            self.traffic.focus = TrafficFocus::Traffic;
            self.needs_full_clear = true;
            return true;
        }

        false
    }

    /// Handle window/split commands (after Ctrl+W prefix)
    pub(super) fn handle_key_window_command(&mut self, key: KeyEvent) {
        self.input.mode = InputMode::Normal;

        match key.code {
            // Vertical split - show split selection prompt
            KeyCode::Char('v') => {
                if self.layout.is_split() {
                    self.status = "Already split - close with Ctrl+W q first".to_string();
                } else {
                    self.enter_split_select_mode();
                }
            }
            // Close secondary pane
            KeyCode::Char('q') => match self.layout.close_secondary() {
                Ok(()) => {
                    self.needs_full_clear = true;
                    self.status = "Closed secondary pane".to_string();
                }
                Err(msg) => {
                    self.status = msg.to_string();
                }
            },
            // Navigation between panes
            KeyCode::Char('h') => {
                self.layout.focus_left();
                self.update_focus_status();
            }
            KeyCode::Char('l') => {
                self.layout.focus_right();
                self.update_focus_status();
            }
            // Cycle focus with Tab or w
            KeyCode::Char('w') | KeyCode::Tab => {
                self.layout.toggle_focus();
                self.update_focus_status();
            }
            // Cancel
            KeyCode::Esc => {
                self.status = "Window command cancelled".to_string();
            }
            _ => {
                self.status = "Unknown window command (v=vsplit, q=close, h/l=nav)".to_string();
            }
        }
    }

    /// Handle split selection mode (choosing which content to split with)
    pub(super) fn handle_key_split_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c @ '1'..='3') => {
                self.input.mode = InputMode::Normal;
                let tab_num = c.to_digit(10).unwrap() as u8;
                if let Some(content) = PaneContent::from_tab_number(tab_num) {
                    match self.layout.vsplit(content) {
                        Ok(()) => {
                            self.needs_full_clear = true;
                            self.status = format!("Split with {}", content.display_name());
                        }
                        Err(msg) => {
                            self.status = msg.to_string();
                        }
                    }
                }
            }
            KeyCode::Esc => {
                self.input.mode = InputMode::Normal;
                self.status = "Split cancelled".to_string();
            }
            _ => {
                // Invalid selection, show options again
                self.status = self.split_selection_prompt();
            }
        }
    }

    pub(super) fn update_focus_status(&mut self) {
        if self.traffic.config.visible && self.traffic.focus == TrafficFocus::Config {
            self.status = "Focus: Config".to_string();
        } else {
            let content = self.layout.focused_content();
            let pane_indicator = if self.layout.is_split() {
                match self.layout.focus() {
                    PaneFocus::Primary => " (Primary)",
                    PaneFocus::Secondary => " (Secondary)",
                }
            } else {
                ""
            };
            self.status = format!("Focus: {}{}", content.display_name(), pane_indicator);
        }
    }

    /// Generate the split selection prompt showing available split options
    fn split_selection_prompt(&self) -> String {
        let primary = self.layout.primary_content();
        let options = primary.available_splits();
        let prompt = options
            .iter()
            .map(|c| format!("[{}] {}", c.tab_number(), c.display_name()))
            .collect::<Vec<_>>()
            .join("  ");
        format!("Split with: {}  [Esc: cancel]", prompt)
    }

    /// Enter split selection mode with the appropriate prompt
    fn enter_split_select_mode(&mut self) {
        self.input.mode = InputMode::SplitSelect;
        self.status = self.split_selection_prompt();
    }

    /// Navigate focus left: Config -> Secondary -> Primary
    /// Returns true if focus changed
    pub(super) fn navigate_focus_left(&mut self) -> bool {
        // If config panel is focused, move to the rightmost pane
        if self.traffic.config.visible && self.traffic.focus == TrafficFocus::Config {
            self.traffic.focus = TrafficFocus::Traffic;
            // If split, focus the secondary (rightmost) pane
            if self.layout.is_split() {
                self.layout.active_state_mut().focus = PaneFocus::Secondary;
            }
            return true;
        }

        // If in split view, try to move left between panes
        if self.layout.is_split() && self.layout.focus() == PaneFocus::Secondary {
            self.layout.focus_left();
            return true;
        }

        // Already at leftmost position
        false
    }

    /// Navigate focus right: Primary -> Secondary -> Config
    /// Returns true if focus changed
    pub(super) fn navigate_focus_right(&mut self) -> bool {
        // If config panel is visible and we're on traffic side
        if self.traffic.focus == TrafficFocus::Traffic {
            // If split, check if we're on secondary pane
            if self.layout.is_split() {
                if self.layout.focus() == PaneFocus::Primary {
                    // Move to secondary pane
                    self.layout.focus_right();
                    return true;
                } else if self.traffic.config.visible {
                    // Already on secondary, move to config
                    self.traffic.focus = TrafficFocus::Config;
                    return true;
                }
            } else if self.traffic.config.visible {
                // No split, move directly to config
                self.traffic.focus = TrafficFocus::Config;
                return true;
            }
        }

        // Already at rightmost position
        false
    }

    /// Handle common key events for placeholder panes (Graph, AdvancedSend).
    /// These panes share basic functionality until fully implemented.
    /// Returns true if the key was handled, false otherwise.
    fn handle_key_placeholder_pane(&mut self, key: KeyEvent) -> bool {
        // Check for disconnect command
        let cmd = self.settings.keybindings.traffic.find_command(&key);
        if let Some(TrafficCommand::Disconnect) = cmd {
            self.traffic.quit_confirm = true;
            return true;
        }

        // Toggle config panel with 'c'
        if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
            self.traffic.config.visible = !self.traffic.config.visible;
            if self.traffic.config.visible {
                self.traffic.focus = TrafficFocus::Config;
            } else {
                self.traffic.focus = TrafficFocus::Traffic;
            }
            self.needs_full_clear = true;
            return true;
        }

        false
    }

    /// Handle key events for graph pane
    pub(super) fn handle_key_graph(&mut self, key: KeyEvent) {
        use crate::app::GraphFocus;

        match key.code {
            // Toggle config panel with Tab or 'c'
            KeyCode::Tab | KeyCode::Char('c') => {
                self.graph.focus = match self.graph.focus {
                    GraphFocus::Graph => GraphFocus::Config,
                    GraphFocus::Config => GraphFocus::Graph,
                };
            }
            // Config panel navigation when focused on config
            KeyCode::Char('j') | KeyCode::Down
                if matches!(self.graph.focus, GraphFocus::Config) =>
            {
                self.graph.config.next_field();
            }
            KeyCode::Char('k') | KeyCode::Up if matches!(self.graph.focus, GraphFocus::Config) => {
                self.graph.config.prev_field();
            }
            // Open dropdown or toggle for config fields
            KeyCode::Enter | KeyCode::Char(' ')
                if matches!(self.graph.focus, GraphFocus::Config) =>
            {
                self.confirm_graph_config_field();
            }
            // Open dropdown with l or right
            KeyCode::Char('l') | KeyCode::Right
                if matches!(self.graph.focus, GraphFocus::Config) =>
            {
                let field = self.graph.config.field;
                if !field.is_toggle() && !field.is_text_input() {
                    // Open dropdown
                    self.graph.open_dropdown();
                    self.input.mode = InputMode::GraphConfigDropdown;
                }
            }
            // Return to graph focus with h or left
            KeyCode::Char('h') | KeyCode::Left
                if matches!(self.graph.focus, GraphFocus::Config) =>
            {
                self.graph.focus = GraphFocus::Graph;
            }
            // Use shared placeholder handler for common functionality (tab switching, etc.)
            _ => {
                self.handle_key_placeholder_pane(key);
            }
        }
    }

    /// Handle key events for advanced send pane (placeholder)
    pub(super) fn handle_key_advanced_send(&mut self, key: KeyEvent) {
        // Use shared placeholder handler for common functionality
        if self.handle_key_placeholder_pane(key) {
            // Handled by shared handler
        }
        // Send-specific keybindings will go here
    }

    /// Handle command line input (after pressing :)
    pub(super) fn handle_key_command_line(&mut self, key: KeyEvent) {
        self.handle_simple_text_input(key, TextInputAction::ExecuteCommand, "Command cancelled");
    }

    /// Execute a command line command
    pub(super) fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "q" | "quit" => {
                if matches!(self.view, View::Connected) {
                    self.traffic.quit_confirm = true;
                } else {
                    self.should_quit = true;
                }
            }
            "connect" => {
                if parts.len() > 1 {
                    let port_path = parts[1..].join(" ");
                    self.connect_to_port(&port_path);
                } else {
                    self.status = "Usage: :connect <port_path>".to_string();
                }
            }
            "disconnect" => {
                if matches!(self.view, View::Connected) {
                    self.disconnect();
                    self.view = View::PortSelect;
                    self.needs_full_clear = true;
                    self.status = "Disconnected.".to_string();
                } else {
                    self.status = "Not connected".to_string();
                }
            }
            "vsplit" => {
                if !matches!(self.view, View::Connected) {
                    self.status = "Must be connected to use splits".to_string();
                    return;
                }
                if self.layout.is_split() {
                    self.status = "Already split - use :close first".to_string();
                    return;
                }
                if parts.len() > 1 {
                    if let Ok(tab_num) = parts[1].parse::<u8>() {
                        if let Some(content) = PaneContent::from_tab_number(tab_num) {
                            match self.layout.vsplit(content) {
                                Ok(()) => {
                                    self.needs_full_clear = true;
                                    self.status = format!("Split with {}", content.display_name());
                                }
                                Err(msg) => {
                                    self.status = msg.to_string();
                                }
                            }
                        } else {
                            self.status =
                                "Invalid pane number (1=Traffic, 2=Graph, 3=Send)".to_string();
                        }
                    } else {
                        self.status = "Usage: :vsplit [1|2|3]".to_string();
                    }
                } else {
                    // No argument: enter split selection mode
                    self.enter_split_select_mode();
                }
            }
            "close" => {
                if !matches!(self.view, View::Connected) {
                    self.status = "Must be connected".to_string();
                    return;
                }
                match self.layout.close_secondary() {
                    Ok(()) => {
                        self.needs_full_clear = true;
                        self.status = "Closed secondary pane".to_string();
                    }
                    Err(msg) => {
                        self.status = msg.to_string();
                    }
                }
            }
            "set" => {
                // Handle :set commands (encoding, baud, etc.) - placeholder for now
                self.status = "Set commands not yet implemented".to_string();
            }
            _ => {
                self.status = format!("Unknown command: {}", parts[0]);
            }
        }
    }

    pub(super) fn handle_key_quit_confirm(&mut self, key: KeyEvent) {
        match key.code {
            // Y/y confirms disconnect
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.traffic.quit_confirm = false;
                self.disconnect();
                self.view = View::PortSelect;
                self.needs_full_clear = true;
                self.status = "Disconnected.".to_string();
            }
            // n/N/q/Escape cancels
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                self.traffic.quit_confirm = false;
                self.status = "Disconnect cancelled.".to_string();
            }
            // Any other key is ignored
            _ => {}
        }
    }

    pub(super) fn handle_key_traffic_config_dropdown(&mut self, key: KeyEvent) {
        self.handle_dropdown(key, DropdownType::TrafficConfig);
    }

    pub(super) fn handle_key_graph_config_dropdown(&mut self, key: KeyEvent) {
        self.handle_dropdown(key, DropdownType::GraphConfig);
    }

    /// Generic handler for dropdown navigation and selection
    fn handle_dropdown(&mut self, key: KeyEvent, dropdown_type: DropdownType) {
        // Get options count and dropdown index reference based on type
        let (options_count, dropdown_index) = match dropdown_type {
            DropdownType::PortConfig => (
                self.port_select.get_options_count(),
                &mut self.port_select.config.dropdown_index,
            ),
            DropdownType::Settings => (
                2, // Regex, Normal
                &mut self.settings_panel.dropdown_index,
            ),
            DropdownType::TrafficConfig => (
                self.traffic.get_options_count(),
                &mut self.traffic.config.dropdown_index,
            ),
            DropdownType::GraphConfig => (
                self.graph.get_options_count(),
                &mut self.graph.config.dropdown_index,
            ),
        };

        match handle_dropdown_key(
            key,
            options_count,
            dropdown_index,
            &self.settings.keybindings.dropdown,
        ) {
            DropdownResult::Confirmed => {
                match dropdown_type {
                    DropdownType::PortConfig => {
                        self.port_select.apply_dropdown_selection();
                    }
                    DropdownType::Settings => {
                        self.apply_settings_dropdown_selection();
                    }
                    DropdownType::TrafficConfig => {
                        self.traffic.apply_dropdown_selection();
                        self.needs_full_clear = true;
                        self.status = self.traffic_config_status();
                    }
                    DropdownType::GraphConfig => {
                        self.graph.apply_dropdown_selection();
                        self.needs_full_clear = true;
                        self.status = format!(
                            "Graph: Mode={}, Parser={}",
                            self.graph
                                .engine
                                .as_ref()
                                .map(|e| e.mode().to_string())
                                .unwrap_or("N/A".to_string()),
                            self.graph
                                .engine
                                .as_ref()
                                .map(|e| e.parser_config().parser_type().to_string())
                                .unwrap_or("N/A".to_string())
                        );
                    }
                }
                self.input.mode = InputMode::Normal;
            }
            DropdownResult::Cancelled => {
                self.input.mode = InputMode::Normal;
            }
            DropdownResult::Navigated | DropdownResult::NotHandled => {}
        }
    }

    pub(super) fn handle_key_search_input(&mut self, key: KeyEvent) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(pattern) => {
                // Set the pattern using the SearchEngine
                if let Err(e) = self.search.set_pattern(&pattern, self.search.mode()) {
                    self.status = e;
                    return;
                }
                self.update_search_matches();
                self.goto_next_match();
            }
            TextInputResult::Cancel => {
                self.status = "Search cancelled.".to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    pub(super) fn handle_key_config_text_input(&mut self, key: KeyEvent) {
        // For numeric fields, filter out non-numeric characters
        if self.port_select.config.field.is_numeric_input()
            && let KeyCode::Char(c) = key.code
            && !c.is_ascii_digit()
        {
            return; // Ignore non-numeric characters
        }

        self.handle_simple_text_input(key, TextInputAction::ApplyPortConfig, "Input cancelled.");
    }

    pub(super) fn handle_key_traffic_config_text_input(&mut self, key: KeyEvent) {
        self.handle_simple_text_input(key, TextInputAction::ApplyTrafficConfig, "Input cancelled.");
    }

    pub(super) fn handle_key_graph_config_text_input(&mut self, key: KeyEvent) {
        self.handle_simple_text_input(key, TextInputAction::ApplyGraphConfig, "Input cancelled.");
    }

    /// Generic handler for simple text input modes
    /// Handles the common pattern of submit->action, cancel->message
    fn handle_simple_text_input(
        &mut self,
        key: KeyEvent,
        action: TextInputAction,
        cancel_msg: &str,
    ) {
        match self.input.handle_text_input(key) {
            TextInputResult::Submit(value) => match action {
                TextInputAction::ConnectToPort => {
                    self.connect_to_port(&value);
                }
                TextInputAction::ExecuteCommand => {
                    self.execute_command(&value);
                }
                TextInputAction::SendFile => {
                    self.start_file_send(&value);
                }
                TextInputAction::ApplyPortConfig => {
                    self.port_select.apply_text_input(value);
                    self.status = self.port_config_status();
                }
                TextInputAction::ApplyTrafficConfig => {
                    self.traffic.apply_text_input(value);
                    self.status = self.traffic_config_status();
                }
                TextInputAction::ApplyGraphConfig => {
                    self.graph.apply_text_input(value);
                    self.status = format!(
                        "{}: {}",
                        self.graph.config.field.label(),
                        self.graph.get_config_display(self.graph.config.field)
                    );
                }
            },
            TextInputResult::Cancel => {
                self.status = cancel_msg.to_string();
            }
            TextInputResult::Continue => {}
        }
    }

    /// Format status message for current port config field
    fn port_config_status(&self) -> String {
        format!(
            "{}: {}",
            self.port_select.config.field.label(),
            self.port_select
                .get_config_display(self.port_select.config.field)
        )
    }

    /// Format status message for current traffic config field  
    fn traffic_config_status(&self) -> String {
        format!(
            "{}: {}",
            self.traffic.config.field.label(),
            self.traffic.get_config_display(self.traffic.config.field)
        )
    }

    /// Enter an input mode with cleared buffer and appropriate status prompt
    fn enter_input_mode(&mut self, mode: InputMode) {
        self.input.mode = mode.clone();
        self.input.buffer.clear();
        self.status = mode.entry_prompt().to_string();
    }

    /// Handle confirm action on a port config field (toggle/text input/dropdown)
    fn confirm_port_config_field(&mut self) {
        if self.port_select.config.field.is_toggle() {
            self.port_select.toggle_setting();
            self.status = self.port_config_status();
        } else if self.port_select.config.field.is_text_input() {
            self.input.buffer = self.port_select.get_text_value();
            self.input.mode = InputMode::ConfigTextInput;
            self.status = InputMode::ConfigTextInput.entry_prompt().to_string();
        } else {
            self.port_select.open_dropdown();
            self.input.mode = InputMode::ConfigDropdown;
        }
    }

    /// Handle confirm action on a traffic config field (toggle/text input/dropdown)
    fn confirm_traffic_config_field(&mut self) {
        if self.traffic.config.field.is_toggle() {
            self.handle_traffic_toggle();
        } else if self.traffic.config.field.is_text_input() {
            self.input.buffer = self.traffic.get_text_value();
            self.input.mode = InputMode::TrafficConfigTextInput;
            self.status = InputMode::TrafficConfigTextInput.entry_prompt().to_string();
        } else {
            self.traffic.open_dropdown();
            self.input.mode = InputMode::TrafficConfigDropdown;
        }
    }

    /// Handle confirm action on a graph config field (toggle/text input/dropdown)
    fn confirm_graph_config_field(&mut self) {
        let field = self.graph.config.field;
        if field.is_toggle() {
            self.graph.toggle_setting();
            self.status = format!(
                "{}: {}",
                field.label(),
                self.graph.get_config_display(field)
            );
        } else if field.is_text_input() {
            self.input.buffer = self.graph.get_text_value();
            self.input.mode = InputMode::GraphConfigTextInput;
            self.status = InputMode::GraphConfigTextInput.entry_prompt().to_string();
        } else {
            self.graph.open_dropdown();
            self.input.mode = InputMode::GraphConfigDropdown;
        }
    }

    /// Perform a full search across all chunks using the SearchEngine
    pub(super) fn update_search_matches(&mut self) {
        use crate::ui::format_hex_grouped;

        if !self.search.has_pattern() {
            self.status = String::new();
            return;
        }

        // Build encoded chunks iterator for the search engine
        if let super::types::ConnectionState::Connected(ref handle) = self.connection {
            let buffer = handle.buffer();
            let encoding = self.traffic.encoding;
            let hex_grouping = self.traffic.hex_grouping;

            // Create an iterator that encodes each chunk
            let encoded_chunks = buffer.chunks().map(|chunk| {
                let encoded = encode(&chunk.data, encoding);
                // Apply hex grouping if in hex mode (same as rendering)
                if encoding == serial_core::Encoding::Hex {
                    format_hex_grouped(&encoded, hex_grouping)
                } else {
                    encoded
                }
            });

            // Perform the search
            self.search.search_all(encoded_chunks);
        }

        // Update status based on results
        self.status = self.search.status_message();
    }

    pub(super) fn goto_next_match(&mut self) {
        if let Some(chunk_index) = self.search.goto_next_match() {
            self.traffic.scroll_to_chunk = Some(chunk_index);
            self.status = self.search.status_message();
        } else {
            self.status = "No matches".to_string();
        }
    }

    pub(super) fn goto_prev_match(&mut self) {
        if let Some(chunk_index) = self.search.goto_prev_match() {
            self.traffic.scroll_to_chunk = Some(chunk_index);
            self.status = self.search.status_message();
        } else {
            self.status = "No matches".to_string();
        }
    }

    pub(super) fn handle_key_file_path_input(&mut self, key: KeyEvent) {
        self.handle_simple_text_input(key, TextInputAction::SendFile, "File send cancelled.");
    }

    pub(super) fn handle_key_send_input(&mut self, key: KeyEvent) {
        // Send input is special: Enter sends with newline but stays in input mode,
        // Ctrl+J sends without newline, Esc exits
        match key.code {
            KeyCode::Enter => {
                if !self.input.buffer.is_empty() {
                    let mut data = self.input.buffer.clone();
                    data.push('\n');
                    self.send_data(data.into_bytes());
                    self.input.buffer.clear();
                }
            }
            KeyCode::Esc => {
                self.input.mode = InputMode::Normal;
                self.input.buffer.clear();
                self.status = "Send cancelled.".to_string();
            }
            KeyCode::Backspace => {
                self.input.buffer.pop();
            }
            KeyCode::Char(c) if c == 'j' && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+J: send without newline
                if !self.input.buffer.is_empty() {
                    let data = self.input.buffer.clone();
                    self.send_data(data.into_bytes());
                    self.input.buffer.clear();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.buffer.push(c);
            }
            _ => {}
        }
    }

    /// Handle toggling a traffic config setting
    /// This is separate from TrafficState::toggle_setting to handle side effects like file saving
    pub(super) fn handle_traffic_toggle(&mut self) {
        let field = self.traffic.config.field;

        // Handle SaveEnabled specially - toggling during a session starts/stops file saving
        if field == TrafficConfigField::SaveEnabled {
            self.traffic.file_save.enabled = !self.traffic.file_save.enabled;
            if self.traffic.file_save.enabled {
                // Start file saving when enabled during a session
                self.start_file_saving();
            } else {
                // Stop file saving when disabled during a session
                self.stop_file_saving();
            }
        } else {
            // For other toggles, use the TrafficState method
            self.traffic.toggle_setting();
        }

        self.status = self.traffic_config_status();
        self.needs_full_clear = true;
    }
}
