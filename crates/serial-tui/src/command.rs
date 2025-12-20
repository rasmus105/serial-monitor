//! Command pattern for key actions
//!
//! This module defines application commands and maps key events to commands.
//! This decouples key bindings from action logic, making both more testable
//! and potentially allowing for configurable keybindings in the future.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
