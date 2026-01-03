//! Terminal color theme using system colors.

use ratatui::style::{Color, Modifier, Style};

/// Application theme using terminal system colors.
pub struct Theme;

impl Theme {
    // Base colors - using terminal defaults
    pub const BG: Color = Color::Reset;
    pub const FG: Color = Color::Reset;

    // Accent colors
    pub const PRIMARY: Color = Color::Cyan;
    pub const SECONDARY: Color = Color::Blue;
    pub const ACCENT: Color = Color::Magenta;
    
    // Disconnected/pre-connect accent (yellow to indicate "not connected")
    pub const DISCONNECTED: Color = Color::Yellow;

    // Status colors
    pub const SUCCESS: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const ERROR: Color = Color::Red;
    pub const INFO: Color = Color::Cyan;

    // Direction colors
    pub const TX: Color = Color::Yellow;
    pub const RX: Color = Color::Green;

    // UI element colors
    pub const BORDER: Color = Color::DarkGray;
    pub const BORDER_FOCUSED: Color = Color::Cyan;
    pub const BORDER_DISCONNECTED: Color = Color::Yellow;
    /// Selection background - using indexed color 236 (very dark gray) for subtle highlighting
    /// that doesn't wash out foreground colors. Falls back gracefully on 16-color terminals.
    pub const SELECTION: Color = Color::Indexed(236);
    pub const MUTED: Color = Color::DarkGray;
    pub const HIGHLIGHT: Color = Color::White;
    /// Gauge/progress bar unfilled background - slightly visible against terminal background
    pub const GAUGE_BG: Color = Color::Indexed(238);

    // Styles
    pub fn base() -> Style {
        Style::default()
    }

    pub fn title() -> Style {
        Style::default().fg(Self::PRIMARY).add_modifier(Modifier::BOLD)
    }
    
    pub fn title_disconnected() -> Style {
        Style::default().fg(Self::DISCONNECTED).add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::BORDER_FOCUSED)
    }
    
    pub fn border_disconnected() -> Style {
        Style::default().fg(Self::BORDER_DISCONNECTED)
    }

    pub fn selected() -> Style {
        Style::default().bg(Self::SELECTION)
    }

    pub fn highlight() -> Style {
        Style::default().fg(Self::HIGHLIGHT).add_modifier(Modifier::BOLD)
    }

    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }

    pub fn tx() -> Style {
        Style::default().fg(Self::TX)
    }

    pub fn rx() -> Style {
        Style::default().fg(Self::RX)
    }

    pub fn success() -> Style {
        Style::default().fg(Self::SUCCESS)
    }

    pub fn warning() -> Style {
        Style::default().fg(Self::WARNING)
    }

    pub fn error() -> Style {
        Style::default().fg(Self::ERROR)
    }

    pub fn info() -> Style {
        Style::default().fg(Self::INFO)
    }

    pub fn keybind() -> Style {
        Style::default().fg(Self::PRIMARY).add_modifier(Modifier::BOLD)
    }
    
    pub fn keybind_disconnected() -> Style {
        Style::default().fg(Self::DISCONNECTED).add_modifier(Modifier::BOLD)
    }

    pub fn keybind_desc() -> Style {
        Style::default().fg(Self::FG)
    }

    pub fn tab_active() -> Style {
        Style::default()
            .fg(Self::PRIMARY)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::default().fg(Self::MUTED)
    }

    pub fn search_match() -> Style {
        Style::default().fg(Self::WARNING).add_modifier(Modifier::BOLD)
    }

    pub fn search_match_current() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Self::WARNING)
            .add_modifier(Modifier::BOLD)
    }
}
