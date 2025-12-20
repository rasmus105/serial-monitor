//! Command pattern for key actions
//!
//! This module defines application commands and maps key events to commands.
//! This decouples key bindings from action logic, making both more testable
//! and potentially allowing for configurable keybindings in the future.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// =============================================================================
// Key Bindings
// =============================================================================

/// Represents a single key binding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBinding {
    /// A simple character key (e.g., 'j', 'k')
    Char(char),
    /// A special key (e.g., Arrow keys, Enter, Esc)
    Key(KeyCode),
    /// A key with modifiers (e.g., Ctrl+U)
    Modified(KeyCode, KeyModifiers),
}

impl KeyBinding {
    /// Returns a human-readable display string for this binding
    pub fn display(&self) -> String {
        match self {
            KeyBinding::Char(c) => match c {
                ' ' => "Space".to_string(),
                c => c.to_string(),
            },
            KeyBinding::Key(code) => Self::key_code_display(*code),
            KeyBinding::Modified(code, mods) => {
                let mut parts = Vec::new();
                if mods.contains(KeyModifiers::CONTROL) {
                    parts.push("Ctrl".to_string());
                }
                if mods.contains(KeyModifiers::ALT) {
                    parts.push("Alt".to_string());
                }
                if mods.contains(KeyModifiers::SHIFT) {
                    parts.push("Shift".to_string());
                }
                parts.push(Self::key_code_display(*code));
                parts.join("+")
            }
        }
    }

    fn key_code_display(code: KeyCode) -> String {
        match code {
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::F(n) => format!("F{}", n),
            _ => "?".to_string(),
        }
    }

    /// Check if this binding matches a key event
    pub fn matches(&self, event: &KeyEvent) -> bool {
        match self {
            KeyBinding::Char(c) => {
                event.code == KeyCode::Char(*c)
                    && !event.modifiers.contains(KeyModifiers::CONTROL)
                    && !event.modifiers.contains(KeyModifiers::ALT)
            }
            KeyBinding::Key(code) => event.code == *code,
            KeyBinding::Modified(code, mods) => {
                event.code == *code && event.modifiers.contains(*mods)
            }
        }
    }
}

/// Trait for commands that have associated key bindings
pub trait Command: Copy {
    /// Returns the default key bindings for this command
    fn default_keys(&self) -> &'static [KeyBinding];

    /// Returns a human-readable shortcut hint string (e.g., "j/↓")
    /// Returns None if there are no bindings
    fn shortcut_hint(&self) -> Option<String> {
        let keys = self.default_keys();
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
// Commands
// =============================================================================

/// Commands available in port selection view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSelectCommand {
    /// Quit the application
    Quit,
    /// Refresh the list of available ports
    RefreshPorts,
    /// Enter manual port path input mode
    EnterPortPath,
    /// Toggle config panel visibility
    ToggleConfigPanel,
    /// Focus the port list panel
    FocusPortList,
    /// Focus the config panel
    FocusConfig,
    /// Move selection up (in current focus)
    MoveUp,
    /// Move selection down (in current focus)
    MoveDown,
    /// Confirm selection (connect or open dropdown)
    Confirm,
}

/// Commands available in traffic view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficCommand {
    /// Disconnect and return to port selection
    Disconnect,
    /// Scroll up one line
    ScrollUp,
    /// Scroll down one line
    ScrollDown,
    /// Scroll to top
    ScrollToTop,
    /// Scroll to bottom
    ScrollToBottom,
    /// Scroll up half page
    PageUp,
    /// Scroll down half page
    PageDown,
    /// Cycle to next encoding
    CycleEncoding,
    /// Enter send input mode
    EnterSendMode,
    /// Enter search input mode
    EnterSearchMode,
    /// Go to next search match
    NextMatch,
    /// Go to previous search match
    PrevMatch,
    /// Toggle file send (start or cancel)
    ToggleFileSend,
    /// Toggle line numbers display
    ToggleLineNumbers,
    /// Toggle timestamps display
    ToggleTimestamps,
    /// Toggle config panel visibility
    ToggleConfigPanel,
    /// Focus the traffic panel
    FocusTraffic,
    /// Focus the config panel
    FocusConfig,
    /// Move selection up (in config panel)
    MoveUp,
    /// Move selection down (in config panel)
    MoveDown,
    /// Confirm selection (toggle or open dropdown in config panel)
    Confirm,
    /// Clear search or disconnect (context-dependent)
    EscapeOrClear,
}

/// Commands available in config dropdown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropdownCommand {
    /// Move selection up
    MoveUp,
    /// Move selection down
    MoveDown,
    /// Confirm selection
    Confirm,
    /// Cancel and close dropdown
    Cancel,
}

// =============================================================================
// Command Trait Implementations
// =============================================================================

impl Command for PortSelectCommand {
    fn default_keys(&self) -> &'static [KeyBinding] {
        use KeyBinding::*;
        use PortSelectCommand::*;

        match self {
            Quit => &[Char('q'), Modified(KeyCode::Char('c'), KeyModifiers::CONTROL)],
            RefreshPorts => &[Char('r')],
            EnterPortPath => &[Char(':')],
            ToggleConfigPanel => &[Char('t')],
            FocusPortList => &[Char('h'), Key(KeyCode::Left)],
            FocusConfig => &[Char('l'), Key(KeyCode::Right)],
            MoveUp => &[Char('k'), Key(KeyCode::Up)],
            MoveDown => &[Char('j'), Key(KeyCode::Down)],
            Confirm => &[Key(KeyCode::Enter)],
        }
    }
}

impl Command for TrafficCommand {
    fn default_keys(&self) -> &'static [KeyBinding] {
        use KeyBinding::*;
        use TrafficCommand::*;

        match self {
            Disconnect => &[Char('q')],
            ScrollUp => &[Char('k'), Key(KeyCode::Up)],
            ScrollDown => &[Char('j'), Key(KeyCode::Down)],
            ScrollToTop => &[Char('g')],
            ScrollToBottom => &[Char('G')],
            PageUp => &[Modified(KeyCode::Char('u'), KeyModifiers::CONTROL)],
            PageDown => &[Modified(KeyCode::Char('d'), KeyModifiers::CONTROL)],
            CycleEncoding => &[Char('e')],
            EnterSendMode => &[Char('i')],
            EnterSearchMode => &[Char('/')],
            NextMatch => &[Char('n')],
            PrevMatch => &[Char('N')],
            ToggleFileSend => &[Char('f')],
            // No default bindings - user can configure these
            ToggleLineNumbers => &[],
            ToggleTimestamps => &[],
            ToggleConfigPanel => &[Char('c')],
            FocusTraffic => &[Char('h'), Key(KeyCode::Left)],
            FocusConfig => &[Char('l'), Key(KeyCode::Right)],
            MoveUp => &[Char('k'), Key(KeyCode::Up)],
            MoveDown => &[Char('j'), Key(KeyCode::Down)],
            Confirm => &[Key(KeyCode::Enter), Char(' ')],
            EscapeOrClear => &[Key(KeyCode::Esc)],
        }
    }
}

impl Command for DropdownCommand {
    fn default_keys(&self) -> &'static [KeyBinding] {
        use DropdownCommand::*;
        use KeyBinding::*;

        match self {
            MoveUp => &[Char('k'), Key(KeyCode::Up)],
            MoveDown => &[Char('j'), Key(KeyCode::Down)],
            Confirm => &[Key(KeyCode::Enter)],
            Cancel => &[Key(KeyCode::Esc)],
        }
    }
}

// =============================================================================
// Key Mapping
// =============================================================================

/// Maps a key event to a port selection command
pub fn map_port_select_key(key: KeyEvent, config_visible: bool) -> Option<PortSelectCommand> {
    use PortSelectCommand::*;

    match key.code {
        KeyCode::Char('q') => Some(Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Quit),
        KeyCode::Char('r') => Some(RefreshPorts),
        KeyCode::Char(':') => Some(EnterPortPath),
        KeyCode::Char('t') => Some(ToggleConfigPanel),
        KeyCode::Left | KeyCode::Char('h') if config_visible => Some(FocusPortList),
        KeyCode::Right | KeyCode::Char('l') if config_visible => Some(FocusConfig),
        KeyCode::Up | KeyCode::Char('k') => Some(MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(MoveDown),
        KeyCode::Enter => Some(Confirm),
        _ => None,
    }
}

/// Maps a key event to a traffic view command
pub fn map_traffic_key(key: KeyEvent, config_visible: bool, config_focused: bool) -> Option<TrafficCommand> {
    use TrafficCommand::*;

    // When config panel is focused, handle navigation differently
    if config_focused {
        return match key.code {
            KeyCode::Char('q') => Some(Disconnect),
            KeyCode::Char('c') => Some(ToggleConfigPanel),
            KeyCode::Left | KeyCode::Char('h') => Some(FocusTraffic),
            KeyCode::Up | KeyCode::Char('k') => Some(MoveUp),
            KeyCode::Down | KeyCode::Char('j') => Some(MoveDown),
            KeyCode::Enter | KeyCode::Char(' ') => Some(Confirm),
            KeyCode::Esc => Some(EscapeOrClear),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Disconnect),
        KeyCode::Char('c') => Some(ToggleConfigPanel),
        KeyCode::Right | KeyCode::Char('l') if config_visible => Some(FocusConfig),
        KeyCode::Up | KeyCode::Char('k') => Some(ScrollUp),
        KeyCode::Down | KeyCode::Char('j') => Some(ScrollDown),
        KeyCode::Char('g') => Some(ScrollToTop),
        KeyCode::Char('G') => Some(ScrollToBottom),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(PageUp),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(PageDown),
        KeyCode::Char('e') => Some(CycleEncoding),
        KeyCode::Char('i') => Some(EnterSendMode),
        KeyCode::Char('/') => Some(EnterSearchMode),
        KeyCode::Char('n') => Some(NextMatch),
        KeyCode::Char('N') => Some(PrevMatch),
        KeyCode::Char('f') => Some(ToggleFileSend),
        KeyCode::Esc => Some(EscapeOrClear),
        _ => None,
    }
}

/// Maps a key event to a dropdown command
pub fn map_dropdown_key(key: KeyEvent) -> Option<DropdownCommand> {
    use DropdownCommand::*;

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(MoveDown),
        KeyCode::Enter => Some(Confirm),
        KeyCode::Esc => Some(Cancel),
        _ => None,
    }
}
