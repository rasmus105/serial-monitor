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
    pub const SELECTION: Color = Color::DarkGray;
    pub const MUTED: Color = Color::DarkGray;
    pub const HIGHLIGHT: Color = Color::White;

    // Styles
    pub fn default() -> Style {
        Style::default()
    }

    pub fn title() -> Style {
        Style::default().fg(Self::PRIMARY).add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::BORDER_FOCUSED)
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
}
