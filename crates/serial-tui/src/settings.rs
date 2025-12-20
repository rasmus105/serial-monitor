//! Application settings with configurable keybindings
//!
//! This module provides:
//! - `KeyBindings`: Configurable key mappings for all commands
//! - `Settings`: Application settings container
//! - Persistence to/from config files (future)

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::command::{
    Command, DropdownCommand, GlobalNavCommand, KeyBinding, PortSelectCommand, TrafficCommand,
};

// =============================================================================
// KeyBindings - Configurable key mappings
// =============================================================================

/// Configurable key bindings for a specific command type
#[derive(Debug, Clone)]
pub struct CommandBindings<C> {
    bindings: HashMap<C, Vec<KeyBinding>>,
}

impl<C: Command + std::hash::Hash + Eq> CommandBindings<C> {
    /// Create bindings initialized with defaults for all commands
    pub fn with_defaults(commands: &[C]) -> Self {
        let mut bindings = HashMap::new();
        for &cmd in commands {
            bindings.insert(cmd, cmd.default_keys().to_vec());
        }
        Self { bindings }
    }

    /// Get the key bindings for a command
    pub fn get(&self, cmd: C) -> &[KeyBinding] {
        self.bindings.get(&cmd).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Set the key bindings for a command
    pub fn set(&mut self, cmd: C, keys: Vec<KeyBinding>) {
        self.bindings.insert(cmd, keys);
    }

    /// Add a key binding to a command
    pub fn add(&mut self, cmd: C, key: KeyBinding) {
        self.bindings.entry(cmd).or_default().push(key);
    }

    /// Remove a specific key binding from a command
    pub fn remove(&mut self, cmd: C, key: &KeyBinding) {
        if let Some(keys) = self.bindings.get_mut(&cmd) {
            keys.retain(|k| k != key);
        }
    }

    /// Clear all bindings for a command
    pub fn clear(&mut self, cmd: C) {
        self.bindings.insert(cmd, Vec::new());
    }

    /// Reset a command to its default bindings
    pub fn reset_to_default(&mut self, cmd: C) {
        self.bindings.insert(cmd, cmd.default_keys().to_vec());
    }

    /// Find the command that matches a key event
    pub fn find_command(&self, event: &KeyEvent) -> Option<C>
    where
        C: Copy,
    {
        for (&cmd, keys) in &self.bindings {
            if keys.iter().any(|k| k.matches(event)) {
                return Some(cmd);
            }
        }
        None
    }

    /// Get shortcut hint for a command (for UI display)
    pub fn shortcut_hint(&self, cmd: C) -> Option<String> {
        let keys = self.get(cmd);
        if keys.is_empty() {
            return None;
        }
        Some(
            keys.iter()
                .map(|k| k.display())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
}

// =============================================================================
// All Commands Lists (for iteration)
// =============================================================================

/// All global navigation commands
pub const GLOBAL_NAV_COMMANDS: &[GlobalNavCommand] = &[
    GlobalNavCommand::Up,
    GlobalNavCommand::Down,
    GlobalNavCommand::Top,
    GlobalNavCommand::Bottom,
    GlobalNavCommand::PageUp,
    GlobalNavCommand::PageDown,
    GlobalNavCommand::Confirm,
    GlobalNavCommand::Cancel,
];

/// All port selection commands
pub const PORT_SELECT_COMMANDS: &[PortSelectCommand] = &[
    PortSelectCommand::Quit,
    PortSelectCommand::RefreshPorts,
    PortSelectCommand::EnterPortPath,
    PortSelectCommand::ToggleConfigPanel,
    PortSelectCommand::FocusPortList,
    PortSelectCommand::FocusConfig,
    PortSelectCommand::Confirm,
];

/// All traffic commands (excluding navigation which is global)
pub const TRAFFIC_COMMANDS: &[TrafficCommand] = &[
    TrafficCommand::Disconnect,
    TrafficCommand::CycleEncoding,
    TrafficCommand::EnterSendMode,
    TrafficCommand::EnterSearchMode,
    TrafficCommand::NextMatch,
    TrafficCommand::PrevMatch,
    TrafficCommand::ToggleFileSend,
    TrafficCommand::ToggleLineNumbers,
    TrafficCommand::ToggleTimestamps,
    TrafficCommand::ToggleConfigPanel,
    TrafficCommand::FocusTraffic,
    TrafficCommand::FocusConfig,
];

/// All dropdown commands
pub const DROPDOWN_COMMANDS: &[DropdownCommand] = &[
    DropdownCommand::Confirm,
    DropdownCommand::Cancel,
];

// =============================================================================
// KeyBindings - All application keybindings
// =============================================================================

/// All configurable key bindings for the application
#[derive(Debug, Clone)]
pub struct KeyBindings {
    /// Global navigation bindings (shared across all views)
    pub global_nav: CommandBindings<GlobalNavCommand>,
    pub port_select: CommandBindings<PortSelectCommand>,
    pub traffic: CommandBindings<TrafficCommand>,
    pub dropdown: CommandBindings<DropdownCommand>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            global_nav: CommandBindings::with_defaults(GLOBAL_NAV_COMMANDS),
            port_select: CommandBindings::with_defaults(PORT_SELECT_COMMANDS),
            traffic: CommandBindings::with_defaults(TRAFFIC_COMMANDS),
            dropdown: CommandBindings::with_defaults(DROPDOWN_COMMANDS),
        }
    }
}

impl KeyBindings {
    /// Create new keybindings with all defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all keybindings to defaults
    pub fn reset_all(&mut self) {
        *self = Self::default();
    }
}

// =============================================================================
// Global Command Enum (for settings UI)
// =============================================================================

/// A command from any context, used for the settings UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnyCommand {
    GlobalNav(GlobalNavCommand),
    PortSelect(PortSelectCommand),
    Traffic(TrafficCommand),
    Dropdown(DropdownCommand),
}

impl AnyCommand {
    /// Get the category name for this command
    pub fn category(&self) -> &'static str {
        match self {
            AnyCommand::GlobalNav(_) => "Global Navigation",
            AnyCommand::PortSelect(_) => "Port Selection",
            AnyCommand::Traffic(_) => "Traffic View",
            AnyCommand::Dropdown(_) => "Dropdown",
        }
    }

    /// Get the command name for display
    pub fn name(&self) -> &'static str {
        match self {
            AnyCommand::GlobalNav(cmd) => cmd.name(),
            AnyCommand::PortSelect(cmd) => cmd.name(),
            AnyCommand::Traffic(cmd) => cmd.name(),
            AnyCommand::Dropdown(cmd) => cmd.name(),
        }
    }

    /// Get the command description
    pub fn description(&self) -> &'static str {
        match self {
            AnyCommand::GlobalNav(cmd) => cmd.description(),
            AnyCommand::PortSelect(cmd) => cmd.description(),
            AnyCommand::Traffic(cmd) => cmd.description(),
            AnyCommand::Dropdown(cmd) => cmd.description(),
        }
    }

    /// Get all commands in display order
    pub fn all() -> Vec<AnyCommand> {
        let mut all = Vec::new();

        // Global navigation commands first (most important)
        for &cmd in GLOBAL_NAV_COMMANDS {
            all.push(AnyCommand::GlobalNav(cmd));
        }

        // Port selection commands
        for &cmd in PORT_SELECT_COMMANDS {
            all.push(AnyCommand::PortSelect(cmd));
        }

        // Traffic commands
        for &cmd in TRAFFIC_COMMANDS {
            all.push(AnyCommand::Traffic(cmd));
        }

        // Dropdown commands
        for &cmd in DROPDOWN_COMMANDS {
            all.push(AnyCommand::Dropdown(cmd));
        }

        all
    }
}

// =============================================================================
// Command metadata (name, description)
// =============================================================================

impl PortSelectCommand {
    /// Get the display name for this command
    pub fn name(&self) -> &'static str {
        match self {
            PortSelectCommand::Quit => "Quit",
            PortSelectCommand::RefreshPorts => "Refresh Ports",
            PortSelectCommand::EnterPortPath => "Enter Port Path",
            PortSelectCommand::ToggleConfigPanel => "Toggle Config Panel",
            PortSelectCommand::FocusPortList => "Focus Port List",
            PortSelectCommand::FocusConfig => "Focus Config",
            PortSelectCommand::Confirm => "Confirm",
        }
    }

    /// Get the description for this command
    pub fn description(&self) -> &'static str {
        match self {
            PortSelectCommand::Quit => "Exit the application",
            PortSelectCommand::RefreshPorts => "Refresh the list of available ports",
            PortSelectCommand::EnterPortPath => "Manually enter a port path",
            PortSelectCommand::ToggleConfigPanel => "Show/hide the configuration panel",
            PortSelectCommand::FocusPortList => "Move focus to port list",
            PortSelectCommand::FocusConfig => "Move focus to configuration panel",
            PortSelectCommand::Confirm => "Confirm selection / connect",
        }
    }
}

impl TrafficCommand {
    /// Get the display name for this command
    pub fn name(&self) -> &'static str {
        match self {
            TrafficCommand::Disconnect => "Disconnect",
            TrafficCommand::CycleEncoding => "Cycle Encoding",
            TrafficCommand::EnterSendMode => "Enter Send Mode",
            TrafficCommand::EnterSearchMode => "Enter Search Mode",
            TrafficCommand::NextMatch => "Next Match",
            TrafficCommand::PrevMatch => "Previous Match",
            TrafficCommand::ToggleFileSend => "Toggle File Send",
            TrafficCommand::ToggleLineNumbers => "Toggle Line Numbers",
            TrafficCommand::ToggleTimestamps => "Toggle Timestamps",
            TrafficCommand::ToggleConfigPanel => "Toggle Config Panel",
            TrafficCommand::FocusTraffic => "Focus Traffic",
            TrafficCommand::FocusConfig => "Focus Config",
        }
    }

    /// Get the description for this command
    pub fn description(&self) -> &'static str {
        match self {
            TrafficCommand::Disconnect => "Disconnect from port and return to selection",
            TrafficCommand::CycleEncoding => "Cycle through display encodings",
            TrafficCommand::EnterSendMode => "Enter text input mode to send data",
            TrafficCommand::EnterSearchMode => "Start searching in the traffic",
            TrafficCommand::NextMatch => "Go to next search match",
            TrafficCommand::PrevMatch => "Go to previous search match",
            TrafficCommand::ToggleFileSend => "Start or cancel file sending",
            TrafficCommand::ToggleLineNumbers => "Show/hide line numbers",
            TrafficCommand::ToggleTimestamps => "Show/hide timestamps",
            TrafficCommand::ToggleConfigPanel => "Show/hide the configuration panel",
            TrafficCommand::FocusTraffic => "Move focus to traffic view",
            TrafficCommand::FocusConfig => "Move focus to configuration panel",
        }
    }
}

impl DropdownCommand {
    /// Get the display name for this command
    pub fn name(&self) -> &'static str {
        match self {
            DropdownCommand::Confirm => "Confirm",
            DropdownCommand::Cancel => "Cancel",
        }
    }

    /// Get the description for this command
    pub fn description(&self) -> &'static str {
        match self {
            DropdownCommand::Confirm => "Confirm selection",
            DropdownCommand::Cancel => "Cancel and close dropdown",
        }
    }
}

// =============================================================================
// Settings Panel State
// =============================================================================

/// Which tab is selected in the settings panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Keybindings,
    // Future tabs: General, Display, etc.
}

impl SettingsTab {
    pub fn name(&self) -> &'static str {
        match self {
            SettingsTab::Keybindings => "Keybindings",
        }
    }

    pub fn all() -> &'static [SettingsTab] {
        &[SettingsTab::Keybindings]
    }

    pub fn next(self) -> Self {
        let tabs = Self::all();
        let idx = tabs.iter().position(|&t| t == self).unwrap_or(0);
        tabs[(idx + 1) % tabs.len()]
    }

    pub fn prev(self) -> Self {
        let tabs = Self::all();
        let idx = tabs.iter().position(|&t| t == self).unwrap_or(0);
        tabs[(idx + tabs.len() - 1) % tabs.len()]
    }
}

/// State for the settings panel
#[derive(Debug, Clone, Default)]
pub struct SettingsPanelState {
    /// Whether the settings panel is open
    pub open: bool,
    /// Current tab
    pub tab: SettingsTab,
    /// Selected command index in keybindings tab
    pub selected_command: usize,
    /// Scroll offset for the command list
    pub scroll_offset: usize,
    /// Whether we're in "record key" mode for adding a binding
    pub recording_key: bool,
    /// Index of the binding being edited (None = adding new)
    pub editing_binding_index: Option<usize>,
}

impl SettingsPanelState {
    pub fn open(&mut self) {
        self.open = true;
        self.recording_key = false;
        self.editing_binding_index = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.recording_key = false;
        self.editing_binding_index = None;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Get the currently selected command
    pub fn selected_any_command(&self) -> Option<AnyCommand> {
        AnyCommand::all().get(self.selected_command).copied()
    }

    /// Move selection up
    pub fn move_up(&mut self, visible_height: usize) {
        self.move_up_with_height(visible_height);
    }

    /// Move selection down
    pub fn move_down(&mut self, visible_height: usize) {
        let max = AnyCommand::all().len().saturating_sub(1);
        if self.selected_command < max {
            self.selected_command += 1;
            // Adjust scroll if selection moved past visible area
            // Keep a few lines of context visible
            let scroll_margin = 2;
            if self.selected_command >= self.scroll_offset + visible_height.saturating_sub(scroll_margin) {
                self.scroll_offset = self.selected_command.saturating_sub(visible_height.saturating_sub(scroll_margin + 1));
            }
        }
    }

    /// Move selection up with visible height for scroll adjustment
    pub fn move_up_with_height(&mut self, _visible_height: usize) {
        if self.selected_command > 0 {
            self.selected_command -= 1;
            // Adjust scroll if needed - keep a margin at top
            let scroll_margin = 2;
            if self.selected_command < self.scroll_offset + scroll_margin {
                self.scroll_offset = self.selected_command.saturating_sub(scroll_margin);
            }
        }
    }

    /// Page up (move half page up)
    pub fn page_up(&mut self, visible_height: usize) {
        let half_page = visible_height / 2;
        self.selected_command = self.selected_command.saturating_sub(half_page);
        // Adjust scroll to keep selection visible
        if self.selected_command < self.scroll_offset {
            self.scroll_offset = self.selected_command;
        }
    }

    /// Page down (move half page down)
    pub fn page_down(&mut self, visible_height: usize) {
        let half_page = visible_height / 2;
        let max = AnyCommand::all().len().saturating_sub(1);
        self.selected_command = (self.selected_command + half_page).min(max);
        // Adjust scroll to keep selection visible
        if self.selected_command >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_command.saturating_sub(visible_height - 1);
        }
    }

    /// Go to top (first command)
    pub fn go_to_top(&mut self) {
        self.selected_command = 0;
        self.scroll_offset = 0;
    }

    /// Go to bottom (last command)
    pub fn go_to_bottom(&mut self, visible_height: usize) {
        let max = AnyCommand::all().len().saturating_sub(1);
        self.selected_command = max;
        // Adjust scroll to show bottom
        self.scroll_offset = max.saturating_sub(visible_height.saturating_sub(1));
    }

    /// Start recording a new key binding
    pub fn start_recording(&mut self) {
        self.recording_key = true;
        self.editing_binding_index = None;
    }

    /// Start editing an existing binding at the given index
    pub fn start_editing(&mut self, index: usize) {
        self.recording_key = true;
        self.editing_binding_index = Some(index);
    }

    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.recording_key = false;
        self.editing_binding_index = None;
    }
}

// =============================================================================
// Settings Container
// =============================================================================

/// Application settings
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub keybindings: KeyBindings,
}

impl Settings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the bindings for an AnyCommand
    pub fn get_bindings(&self, cmd: AnyCommand) -> Vec<KeyBinding> {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.get(c).to_vec(),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.get(c).to_vec(),
            AnyCommand::Traffic(c) => self.keybindings.traffic.get(c).to_vec(),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.get(c).to_vec(),
        }
    }

    /// Set the bindings for an AnyCommand
    pub fn set_bindings(&mut self, cmd: AnyCommand, bindings: Vec<KeyBinding>) {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.set(c, bindings),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.set(c, bindings),
            AnyCommand::Traffic(c) => self.keybindings.traffic.set(c, bindings),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.set(c, bindings),
        }
    }

    /// Add a binding to an AnyCommand
    pub fn add_binding(&mut self, cmd: AnyCommand, binding: KeyBinding) {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.add(c, binding),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.add(c, binding),
            AnyCommand::Traffic(c) => self.keybindings.traffic.add(c, binding),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.add(c, binding),
        }
    }

    /// Remove a binding from an AnyCommand
    pub fn remove_binding(&mut self, cmd: AnyCommand, binding: &KeyBinding) {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.remove(c, binding),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.remove(c, binding),
            AnyCommand::Traffic(c) => self.keybindings.traffic.remove(c, binding),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.remove(c, binding),
        }
    }

    /// Reset a command to default bindings
    pub fn reset_command(&mut self, cmd: AnyCommand) {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.reset_to_default(c),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.reset_to_default(c),
            AnyCommand::Traffic(c) => self.keybindings.traffic.reset_to_default(c),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.reset_to_default(c),
        }
    }

    /// Get shortcut hint for an AnyCommand
    pub fn shortcut_hint(&self, cmd: AnyCommand) -> Option<String> {
        match cmd {
            AnyCommand::GlobalNav(c) => self.keybindings.global_nav.shortcut_hint(c),
            AnyCommand::PortSelect(c) => self.keybindings.port_select.shortcut_hint(c),
            AnyCommand::Traffic(c) => self.keybindings.traffic.shortcut_hint(c),
            AnyCommand::Dropdown(c) => self.keybindings.dropdown.shortcut_hint(c),
        }
    }
}

// =============================================================================
// Helper: Convert KeyEvent to KeyBinding
// =============================================================================

/// Convert a KeyEvent into a KeyBinding
pub fn key_event_to_binding(event: &KeyEvent) -> KeyBinding {
    let has_ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = event.modifiers.contains(KeyModifiers::ALT);
    let has_shift = event.modifiers.contains(KeyModifiers::SHIFT);

    // Build modifiers (excluding shift for regular characters, as it's implicit)
    let mods = if has_ctrl || has_alt {
        let mut m = KeyModifiers::empty();
        if has_ctrl {
            m |= KeyModifiers::CONTROL;
        }
        if has_alt {
            m |= KeyModifiers::ALT;
        }
        // Only include shift for special keys, not for chars (where it's implicit in the char)
        if has_shift && !matches!(event.code, KeyCode::Char(_)) {
            m |= KeyModifiers::SHIFT;
        }
        Some(m)
    } else {
        None
    };

    match event.code {
        KeyCode::Char(c) => {
            if let Some(m) = mods {
                KeyBinding::Modified(KeyCode::Char(c), m)
            } else {
                KeyBinding::Char(c)
            }
        }
        code => {
            if let Some(m) = mods {
                KeyBinding::Modified(code, m)
            } else {
                KeyBinding::Key(code)
            }
        }
    }
}

// =============================================================================
// Commands for Settings Panel
// =============================================================================

/// Commands for navigating the settings panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCommand {
    /// Close the settings panel
    Close,
    /// Move to next tab
    NextTab,
    /// Move to previous tab
    PrevTab,
    /// Move selection up
    MoveUp,
    /// Move selection down
    MoveDown,
    /// Add a new keybinding
    AddBinding,
    /// Delete selected keybinding
    DeleteBinding,
    /// Reset command to defaults
    ResetToDefault,
    /// Confirm / select
    Confirm,
}

/// Map a key event to a settings command (hardcoded, these are not configurable)
/// Note: Navigation (j/k, Ctrl+u/d) is handled by GlobalNavCommand first
pub fn map_settings_key(event: &KeyEvent) -> Option<SettingsCommand> {
    match event.code {
        // Esc, q, and Enter are also handled by GlobalNavCommand, but kept here as fallback
        KeyCode::Esc | KeyCode::Char('q') => Some(SettingsCommand::Close),
        KeyCode::Tab if event.modifiers.contains(KeyModifiers::SHIFT) => {
            Some(SettingsCommand::PrevTab)
        }
        KeyCode::Tab => Some(SettingsCommand::NextTab),
        // Navigation handled by GlobalNavCommand, these are fallbacks
        KeyCode::Up | KeyCode::Char('k') => Some(SettingsCommand::MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(SettingsCommand::MoveDown),
        KeyCode::Char('a') => Some(SettingsCommand::AddBinding),
        KeyCode::Char('d') | KeyCode::Delete => Some(SettingsCommand::DeleteBinding),
        KeyCode::Char('r') => Some(SettingsCommand::ResetToDefault),
        KeyCode::Enter => Some(SettingsCommand::Confirm),
        _ => None,
    }
}
