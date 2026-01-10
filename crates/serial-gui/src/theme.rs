//! GUI color theme matching the TUI's color scheme.

use iced::widget::container;
use iced::{Background, Border, Color};

/// Application color palette and style helpers.
///
/// Colors are defined for use across the application; some may be reserved
/// for future features (e.g., button styling, focused states).
#[allow(dead_code)]
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    // =========================================================================
    // Base colors
    // =========================================================================
    pub const BG: Color = Color::from_rgb(0.1, 0.1, 0.1);
    pub const FG: Color = Color::from_rgb(0.9, 0.9, 0.9);
    pub const TEXT: Color = Color::from_rgb(0.9, 0.9, 0.9);

    // =========================================================================
    // Accent colors
    // =========================================================================
    pub const PRIMARY: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan
    pub const SECONDARY: Color = Color::from_rgb(0.2, 0.4, 0.8); // Blue
    pub const ACCENT: Color = Color::from_rgb(0.8, 0.2, 0.8); // Magenta

    // Disconnected/pre-connect accent
    pub const DISCONNECTED: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow

    // =========================================================================
    // Status colors
    // =========================================================================
    pub const SUCCESS: Color = Color::from_rgb(0.2, 0.8, 0.2); // Green
    pub const WARNING: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow
    pub const ERROR: Color = Color::from_rgb(0.9, 0.2, 0.2); // Red
    pub const INFO: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan

    // =========================================================================
    // Direction colors
    // =========================================================================
    pub const TX: Color = Color::from_rgb(0.9, 0.8, 0.2); // Yellow
    pub const RX: Color = Color::from_rgb(0.2, 0.8, 0.2); // Green

    // =========================================================================
    // UI element colors
    // =========================================================================
    pub const BORDER: Color = Color::from_rgb(0.3, 0.3, 0.3);
    pub const BORDER_FOCUSED: Color = Color::from_rgb(0.0, 0.8, 0.8); // Cyan
    pub const SELECTION: Color = Color::from_rgb(0.2, 0.2, 0.25);
    pub const MUTED: Color = Color::from_rgb(0.5, 0.5, 0.5);

    // =========================================================================
    // Button colors
    // =========================================================================
    pub const BUTTON_BG: Color = Color::from_rgb(0.2, 0.2, 0.25);
    pub const BUTTON_HOVER: Color = Color::from_rgb(0.25, 0.25, 0.3);
    pub const BUTTON_ACTIVE: Color = Color::from_rgb(0.0, 0.6, 0.6);

    // =========================================================================
    // Style helpers
    // =========================================================================

    /// Standard bordered container style (dark background with subtle border)
    pub fn bordered_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG)),
            border: Border {
                color: Self::BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }

    /// Tooltip container style
    pub fn tooltip_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG)),
            border: Border {
                color: Self::BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }
}
