//! GUI color theme matching the TUI's color scheme.

use iced::Color;

/// Application color palette.
pub struct Theme;

impl Theme {
    // Base colors
    pub const BG: Color = Color::from_rgb(0.1, 0.1, 0.1);
    pub const FG: Color = Color::from_rgb(0.9, 0.9, 0.9);

    // Accent colors
    pub const PRIMARY: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan
    pub const SECONDARY: Color = Color::from_rgb(0.2, 0.4, 0.8); // Blue
    pub const ACCENT: Color = Color::from_rgb(0.8, 0.2, 0.8); // Magenta

    // Disconnected/pre-connect accent
    pub const DISCONNECTED: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow

    // Status colors
    pub const SUCCESS: Color = Color::from_rgb(0.2, 0.8, 0.2); // Green
    pub const WARNING: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow
    pub const ERROR: Color = Color::from_rgb(0.9, 0.2, 0.2); // Red
    pub const INFO: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan

    // Direction colors
    pub const TX: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow
    pub const RX: Color = Color::from_rgb(0.2, 0.8, 0.2); // Green

    // UI element colors
    pub const BORDER: Color = Color::from_rgb(0.3, 0.3, 0.3);
    pub const BORDER_FOCUSED: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan
    pub const SELECTION: Color = Color::from_rgb(0.2, 0.2, 0.25);
    pub const MUTED: Color = Color::from_rgb(0.5, 0.5, 0.5);

    // Button colors
    pub const BUTTON_BG: Color = Color::from_rgb(0.2, 0.2, 0.25);
    pub const BUTTON_HOVER: Color = Color::from_rgb(0.25, 0.25, 0.3);
    pub const BUTTON_ACTIVE: Color = Color::from_rgb(0.0, 0.6, 0.6);
}
