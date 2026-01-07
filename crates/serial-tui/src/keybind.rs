//! Keybinding definitions and help text.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A keybinding definition with description.
#[derive(Debug, Clone)]
pub struct Keybind {
    pub key: KeyEvent,
    pub description: &'static str,
    pub context: KeyContext,
}

/// Context where a keybinding is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    /// Always active (global)
    Global,
    /// Active when not in input mode
    Navigation,
    /// Active when in input mode
    Input,
    /// Active only in pre-connection view
    PreConnect,
    /// Active only when connected
    Connected,
    /// Active in traffic view
    Traffic,
    /// Active in graph view
    Graph,
    /// Active in file sender view
    FileSender,
}

impl Keybind {
    pub const fn new(key: KeyEvent, description: &'static str, context: KeyContext) -> Self {
        Self {
            key,
            description,
            context,
        }
    }

    /// Format the key as a display string.
    pub fn key_display(&self) -> String {
        format_key(&self.key)
    }
}

/// Format a key event as a human-readable string.
pub fn format_key(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }

    let key_name = match key.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Shift+Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".to_string(),
    };

    if parts.is_empty() {
        key_name
    } else {
        parts.push(&key_name);
        parts.join("+")
    }
}

/// Helper to create key events.
pub const fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub const fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

pub const fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

/// All keybindings in the application.
pub fn all_keybinds() -> Vec<Keybind> {
    vec![
        // Global
        Keybind::new(key(KeyCode::Char('q')), "Quit / Disconnect prompt", KeyContext::Global),
        Keybind::new(key(KeyCode::Char('?')), "Help/Settings", KeyContext::Global),
        Keybind::new(
            key(KeyCode::Char('c')),
            "Toggle config panel",
            KeyContext::Global,
        ),
        Keybind::new(key(KeyCode::Esc), "Close overlay/Cancel", KeyContext::Global),
        Keybind::new(
            ctrl(KeyCode::Char('h')),
            "Focus main panel",
            KeyContext::Global,
        ),
        Keybind::new(
            ctrl(KeyCode::Char('l')),
            "Focus config panel",
            KeyContext::Global,
        ),
        // Navigation
        Keybind::new(key(KeyCode::Char('j')), "Down", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char('k')), "Up", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char('h')), "Left / prev option", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char('l')), "Right / next option", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char('g')), "Go to top", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char('G')), "Go to bottom", KeyContext::Navigation),
        Keybind::new(ctrl(KeyCode::Char('d')), "Page down", KeyContext::Navigation),
        Keybind::new(ctrl(KeyCode::Char('u')), "Page up", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Enter), "Select / toggle", KeyContext::Navigation),
        Keybind::new(key(KeyCode::Char(' ')), "Select / toggle", KeyContext::Navigation),
        // Pre-connect
        Keybind::new(
            key(KeyCode::Enter),
            "Connect to port",
            KeyContext::PreConnect,
        ),
        Keybind::new(
            key(KeyCode::Char('r')),
            "Refresh port list",
            KeyContext::PreConnect,
        ),
        Keybind::new(
            key(KeyCode::Char('/')),
            "Search ports",
            KeyContext::PreConnect,
        ),
        Keybind::new(
            key(KeyCode::Char('n')),
            "Next search match",
            KeyContext::PreConnect,
        ),
        Keybind::new(
            key(KeyCode::Char('N')),
            "Previous search match",
            KeyContext::PreConnect,
        ),
        // Connected
        Keybind::new(
            key(KeyCode::Char('1')),
            "Traffic view",
            KeyContext::Connected,
        ),
        Keybind::new(key(KeyCode::Char('2')), "Graph view", KeyContext::Connected),
        Keybind::new(
            key(KeyCode::Char('3')),
            "File sender",
            KeyContext::Connected,
        ),
        Keybind::new(
            key(KeyCode::Char('d')),
            "Disconnect",
            KeyContext::Connected,
        ),
        // Traffic view
        Keybind::new(key(KeyCode::Char('s')), "Send data", KeyContext::Traffic),
        Keybind::new(key(KeyCode::Char('/')), "Search", KeyContext::Traffic),
        Keybind::new(key(KeyCode::Char('n')), "Next match", KeyContext::Traffic),
        Keybind::new(key(KeyCode::Char('N')), "Previous match", KeyContext::Traffic),
        Keybind::new(key(KeyCode::Char('f')), "Filter", KeyContext::Traffic),
        Keybind::new(
            ctrl(KeyCode::Char('b')),
            "Toggle lock to bottom",
            KeyContext::Traffic,
        ),
        // Graph view
        Keybind::new(
            key(KeyCode::Char('g')),
            "Toggle graph parsing",
            KeyContext::Graph,
        ),
        Keybind::new(
            key(KeyCode::Tab),
            "Switch Settings/Series",
            KeyContext::Graph,
        ),
        Keybind::new(
            key(KeyCode::Char('t')),
            "Toggle series visibility",
            KeyContext::Graph,
        ),
        // File sender
        Keybind::new(
            key(KeyCode::Char('o')),
            "Open file",
            KeyContext::FileSender,
        ),
        Keybind::new(
            key(KeyCode::Enter),
            "Start sending",
            KeyContext::FileSender,
        ),
        Keybind::new(
            key(KeyCode::Char('x')),
            "Cancel sending",
            KeyContext::FileSender,
        ),
        // Input mode
        Keybind::new(key(KeyCode::Enter), "Confirm input", KeyContext::Input),
        Keybind::new(key(KeyCode::Esc), "Cancel input", KeyContext::Input),
    ]
}

/// Get keybindings for a specific context.
pub fn keybinds_for_context(context: KeyContext) -> Vec<&'static Keybind> {
    // Use a static to avoid recreating the vec each time
    static KEYBINDS: std::sync::OnceLock<Vec<Keybind>> = std::sync::OnceLock::new();
    let all = KEYBINDS.get_or_init(all_keybinds);
    all.iter()
        .filter(|k| k.context == context || k.context == KeyContext::Global)
        .collect()
}

/// A condensed keybind hint for status bars.
///
/// Use this to ensure consistency between inline help and the help overlay.
pub struct KeyHint {
    pub key: &'static str,
    pub description: &'static str,
}

impl KeyHint {
    pub const fn new(key: &'static str, description: &'static str) -> Self {
        Self { key, description }
    }
}

/// Common key hints for the pre-connect view status bar.
pub const PRECONNECT_HINTS: &[KeyHint] = &[
    KeyHint::new("Enter", "connect"),
    KeyHint::new("r", "refresh"),
    KeyHint::new("/", "search"),
    KeyHint::new("Ctrl+h/l", "panels"),
    KeyHint::new("?", "help"),
];

/// Common key hints for the traffic view status bar.
pub const TRAFFIC_HINTS: &[KeyHint] = &[
    KeyHint::new("s", "send"),
    KeyHint::new("/", "search"),
    KeyHint::new("f", "filter"),
    KeyHint::new("Ctrl+b", "lock bottom"),
    KeyHint::new("?", "help"),
];

/// Common key hints for the graph view status bar.
pub const GRAPH_HINTS: &[KeyHint] = &[
    KeyHint::new("g", "toggle graph"),
    KeyHint::new("Tab", "settings/series"),
    KeyHint::new("t", "toggle series"),
    KeyHint::new("?", "help"),
];

/// Common key hints for the file sender view status bar.
pub const FILE_SENDER_HINTS: &[KeyHint] = &[
    KeyHint::new("o", "open file"),
    KeyHint::new("Enter", "start sending"),
    KeyHint::new("x", "cancel"),
    KeyHint::new("?", "help"),
];
