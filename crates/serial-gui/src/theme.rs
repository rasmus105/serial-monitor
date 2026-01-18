//! GUI theme with modern dark color scheme and typography.
//!
//! This module provides a cohesive visual design system including:
//! - Layered background colors for visual hierarchy
//! - Muted, desaturated accent colors that feel modern
//! - Typography constants for consistent sizing
//! - Widget style functions for buttons, inputs, etc.

use iced::widget::{button, container, pick_list, scrollable, text_input};
use iced::{Background, Border, Color, Shadow};

// =============================================================================
// Typography
// =============================================================================

/// Font size constants for consistent typography throughout the application.
#[allow(dead_code)]
pub mod font_size {
    /// Title text (e.g., "Serial Monitor" on pre-connect screen)
    pub const TITLE: f32 = 24.0;
    /// Section headers in panels
    pub const HEADER: f32 = 14.0;
    /// Primary body text
    pub const BODY: f32 = 13.0;
    /// Secondary/smaller text
    pub const SMALL: f32 = 11.0;
    /// Caption text (labels, hints)
    pub const CAPTION: f32 = 10.0;
}

/// Standard spacing values for consistent layout.
#[allow(dead_code)]
pub mod spacing {
    /// Standard padding inside containers
    pub const CONTAINER: u16 = 10;
    /// Padding inside sections
    pub const SECTION: u16 = 8;
    /// Gap between elements in a row
    pub const ROW_GAP: u16 = 8;
    /// Gap between sections
    pub const SECTION_GAP: u16 = 4;
    /// Vertical padding for config rows
    pub const ROW_PADDING: u16 = 6;
}

// =============================================================================
// Color Palette
// =============================================================================

/// Application color palette - modern muted dark theme.
///
/// Inspired by VS Code dark and similar modern IDEs.
/// Colors are intentionally desaturated for a professional look.
#[allow(dead_code)]
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    // =========================================================================
    // Background colors (layered for visual hierarchy)
    // =========================================================================

    /// Base background - darkest layer
    pub const BG_BASE: Color = Color::from_rgb(0.118, 0.118, 0.118); // #1e1e1e

    /// Surface background - slightly elevated (panels, cards)
    pub const BG_SURFACE: Color = Color::from_rgb(0.145, 0.145, 0.145); // #252525

    /// Elevated background - for raised elements
    pub const BG_ELEVATED: Color = Color::from_rgb(0.176, 0.176, 0.176); // #2d2d2d

    /// Alternating row background (slightly different from surface)
    pub const BG_ROW_ALT: Color = Color::from_rgb(0.157, 0.157, 0.165); // #282829

    /// Hover state background
    pub const BG_HOVER: Color = Color::from_rgb(0.20, 0.20, 0.22); // #333338

    /// Active/pressed state background
    pub const BG_ACTIVE: Color = Color::from_rgb(0.22, 0.22, 0.25); // #383840

    // =========================================================================
    // Text colors
    // =========================================================================

    /// Primary text color - high contrast but not pure white
    pub const TEXT_PRIMARY: Color = Color::from_rgb(0.80, 0.80, 0.80); // #cccccc

    /// Secondary/muted text color (alias: MUTED)
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.52, 0.52, 0.52); // #858585

    /// Disabled text color
    pub const TEXT_DISABLED: Color = Color::from_rgb(0.40, 0.40, 0.40); // #666666

    // =========================================================================
    // Legacy aliases for compatibility
    // =========================================================================

    /// Alias for TEXT_SECONDARY - used for muted/secondary text
    pub const MUTED: Color = Self::TEXT_SECONDARY;

    /// Alias for TEXT_PRIMARY - primary text color
    pub const TEXT: Color = Self::TEXT_PRIMARY;

    /// Alias for STATUS_ERROR
    pub const ERROR: Color = Self::STATUS_ERROR;

    /// Alias for STATUS_SUCCESS
    pub const SUCCESS: Color = Self::STATUS_SUCCESS;

    /// Alias for ACCENT_PRIMARY - used for headers
    pub const PRIMARY: Color = Self::ACCENT_PRIMARY;

    // =========================================================================
    // Border colors
    // =========================================================================

    /// Standard border color
    pub const BORDER: Color = Color::from_rgb(0.24, 0.24, 0.24); // #3c3c3c

    /// Focused/highlighted border
    pub const BORDER_FOCUSED: Color = Color::from_rgb(0.34, 0.61, 0.84); // #569cd6

    // =========================================================================
    // Accent colors (desaturated for modern look)
    // =========================================================================

    /// Primary accent - soft blue
    pub const ACCENT_PRIMARY: Color = Color::from_rgb(0.34, 0.61, 0.84); // #569cd6

    /// Success/positive - teal green
    pub const STATUS_SUCCESS: Color = Color::from_rgb(0.31, 0.79, 0.69); // #4ec9b0

    /// Warning - soft yellow
    pub const STATUS_WARNING: Color = Color::from_rgb(0.86, 0.86, 0.67); // #dcdcaa

    /// Error - softer red
    pub const STATUS_ERROR: Color = Color::from_rgb(0.95, 0.30, 0.30); // #f14c4c

    // =========================================================================
    // Semantic colors
    // =========================================================================

    /// TX (transmitted data) color - soft yellow
    pub const TX: Color = Self::STATUS_WARNING;

    /// RX (received data) color - teal green
    pub const RX: Color = Self::STATUS_SUCCESS;

    /// Connected/active status indicator - vivid green (not teal)
    pub const STATUS_CONNECTED: Color = Color::from_rgb(0.30, 0.75, 0.35); // #4dbf59

    // =========================================================================
    // Container Style Functions
    // =========================================================================

    /// Standard bordered container style (dark background with subtle border)
    pub fn bordered_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG_SURFACE)),
            border: Border {
                color: Self::BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }

    /// Tooltip container style
    pub fn tooltip_container(theme: &iced::Theme) -> container::Style {
        Self::bordered_container(theme)
    }

    /// Section header background style
    pub fn section_header_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG_ELEVATED)),
            border: Border {
                color: Self::BORDER,
                width: 0.0,
                radius: 2.0.into(),
            },
            ..Default::default()
        }
    }

    /// Alternating row background (for even rows)
    pub fn row_even_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG_SURFACE)),
            ..Default::default()
        }
    }

    /// Alternating row background (for odd rows)
    pub fn row_odd_container(_theme: &iced::Theme) -> container::Style {
        container::Style {
            background: Some(Background::Color(Self::BG_ROW_ALT)),
            ..Default::default()
        }
    }

    // =========================================================================
    // Button Styles
    // =========================================================================

    /// Primary button style (accent colored)
    pub fn button_primary(_theme: &iced::Theme, status: button::Status) -> button::Style {
        let (background, text_color) = match status {
            button::Status::Active => (Self::ACCENT_PRIMARY, Self::BG_BASE),
            button::Status::Hovered => {
                // Slightly lighter on hover
                (Color::from_rgb(0.40, 0.65, 0.88), Self::BG_BASE)
            }
            button::Status::Pressed => {
                // Slightly darker on press
                (Color::from_rgb(0.28, 0.55, 0.78), Self::BG_BASE)
            }
            button::Status::Disabled => (Self::BG_ELEVATED, Self::TEXT_DISABLED),
        };

        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }

    /// Secondary button style (subtle, for less important actions)
    pub fn button_secondary(_theme: &iced::Theme, status: button::Status) -> button::Style {
        let (background, text_color, border_color) = match status {
            button::Status::Active => (Self::BG_ELEVATED, Self::TEXT_PRIMARY, Self::BORDER),
            button::Status::Hovered => (Self::BG_HOVER, Self::TEXT_PRIMARY, Self::BORDER_FOCUSED),
            button::Status::Pressed => (Self::BG_ACTIVE, Self::TEXT_PRIMARY, Self::BORDER_FOCUSED),
            button::Status::Disabled => (Self::BG_SURFACE, Self::TEXT_DISABLED, Self::BORDER),
        };

        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }

    /// Ghost button style (no background, just text)
    pub fn button_ghost(_theme: &iced::Theme, status: button::Status) -> button::Style {
        let (background, text_color) = match status {
            button::Status::Active => (Color::TRANSPARENT, Self::TEXT_SECONDARY),
            button::Status::Hovered => (Self::BG_HOVER, Self::TEXT_PRIMARY),
            button::Status::Pressed => (Self::BG_ACTIVE, Self::TEXT_PRIMARY),
            button::Status::Disabled => (Color::TRANSPARENT, Self::TEXT_DISABLED),
        };

        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    }

    /// Section header button style (transparent, full width clickable)
    pub fn button_section_header(_theme: &iced::Theme, status: button::Status) -> button::Style {
        // The inner container handles the background, so we just need hover effects
        let text_color = match status {
            button::Status::Active => Self::TEXT_PRIMARY,
            button::Status::Hovered => Self::TEXT_PRIMARY,
            button::Status::Pressed => Self::TEXT_PRIMARY,
            button::Status::Disabled => Self::TEXT_DISABLED,
        };

        button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
            ..Default::default()
        }
    }

    // =========================================================================
    // Text Input Styles
    // =========================================================================

    /// Standard text input style
    pub fn text_input(_theme: &iced::Theme, status: text_input::Status) -> text_input::Style {
        let (background, border_color) = match status {
            text_input::Status::Active => (Self::BG_BASE, Self::BORDER),
            text_input::Status::Hovered => (Self::BG_BASE, Self::BORDER_FOCUSED),
            text_input::Status::Focused { .. } => (Self::BG_BASE, Self::ACCENT_PRIMARY),
            text_input::Status::Disabled => (Self::BG_SURFACE, Self::BORDER),
        };

        text_input::Style {
            background: Background::Color(background),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            icon: Self::TEXT_SECONDARY,
            placeholder: Self::TEXT_DISABLED,
            value: Self::TEXT_PRIMARY,
            selection: Self::ACCENT_PRIMARY,
        }
    }

    // =========================================================================
    // Pick List Styles
    // =========================================================================

    /// Standard pick list (dropdown) style
    pub fn pick_list(_theme: &iced::Theme, status: pick_list::Status) -> pick_list::Style {
        let (background, border_color, text_color) = match status {
            pick_list::Status::Active => (Self::BG_ELEVATED, Self::BORDER, Self::TEXT_PRIMARY),
            pick_list::Status::Hovered => {
                (Self::BG_HOVER, Self::BORDER_FOCUSED, Self::TEXT_PRIMARY)
            }
            pick_list::Status::Opened { .. } => {
                (Self::BG_ACTIVE, Self::ACCENT_PRIMARY, Self::TEXT_PRIMARY)
            }
        };

        pick_list::Style {
            background: Background::Color(background),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color,
            placeholder_color: Self::TEXT_DISABLED,
            handle_color: Self::TEXT_SECONDARY,
        }
    }

    // =========================================================================
    // Scrollbar Styles
    // =========================================================================

    /// Standard scrollbar style
    pub fn scrollbar(_theme: &iced::Theme, status: scrollable::Status) -> scrollable::Style {
        let scroller_color = match status {
            scrollable::Status::Active { .. } => Self::TEXT_DISABLED,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. } => {
                Self::TEXT_SECONDARY
            }
        };

        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: scrollable::Rail {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border::default(),
                scroller: scrollable::Scroller {
                    background: Background::Color(scroller_color),
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: 4.0.into(),
                    },
                },
            },
            horizontal_rail: scrollable::Rail {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border::default(),
                scroller: scrollable::Scroller {
                    background: Background::Color(scroller_color),
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: 4.0.into(),
                    },
                },
            },
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: Background::Color(Self::BG_ELEVATED),
                border: Border {
                    color: Self::BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
                icon: Self::TEXT_SECONDARY,
            },
        }
    }
}
