//! Reusable widget builders for the traffic view.

use iced::widget::{Space, button, column, container, row, text, tooltip};
use iced::{Alignment, Element, Fill};

use crate::app::{ConnectedMsg, Message};
use crate::theme::Theme;

// Re-export widget options from centralized module
pub use crate::widget_options::{
    ENCODING_OPTIONS, EncodingOption, LINE_ENDING_OPTIONS, LineEndingOption, SCROLL_MODE_OPTIONS,
    ScrollModeOption, TIMESTAMP_FORMAT_OPTIONS, TimestampFormatOption,
};

// =============================================================================
// Widget builders
// =============================================================================

/// Create a horizontal divider line
pub fn divider<'a>() -> Element<'a, Message> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Theme::BORDER)),
            ..Default::default()
        })
        .into()
}

/// Create a grid cell for connection info (label above, value below)
pub fn config_grid_cell<'a>(label: &'a str, value: &'a str) -> Element<'a, Message> {
    column![
        text(label).size(10).color(Theme::MUTED),
        text(value).size(12),
    ]
    .spacing(1)
    .width(Fill)
    .into()
}

/// Create a grid cell for connection info with owned value
pub fn config_grid_cell_owned<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    column![
        text(label).size(10).color(Theme::MUTED),
        text(value).size(12),
    ]
    .spacing(1)
    .width(Fill)
    .into()
}

/// Create a section header
pub fn section_header<'a>(title: &'a str) -> Element<'a, Message> {
    column![
        text(title).size(12).color(Theme::MUTED),
        Space::new().height(2),
    ]
    .into()
}

/// Create a collapsible section header that can be clicked to toggle
pub fn collapsible_section_header<'a>(
    title: &'static str,
    collapsed: bool,
) -> Element<'a, Message> {
    let icon = if collapsed { "+" } else { "-" };

    button(
        row![
            text(icon).size(12).color(Theme::MUTED),
            Space::new().width(4),
            text(title).size(12).color(Theme::MUTED),
        ]
        .align_y(Alignment::Center),
    )
    .on_press(Message::Connected(ConnectedMsg::ToggleSectionCollapse(
        title.to_string(),
    )))
    .padding([2, 4])
    .style(|_theme, _status| button::Style {
        background: None,
        text_color: Theme::MUTED,
        ..Default::default()
    })
    .into()
}

/// Create a config row with tooltip on the label
pub fn config_row_with_tooltip<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    tooltip_text: &'a str,
) -> Element<'a, Message> {
    let label_with_tooltip = tooltip(
        text(label).size(12),
        container(text(tooltip_text).size(11))
            .padding(8)
            .style(Theme::tooltip_container),
        tooltip::Position::Left,
    )
    .gap(5);

    row![label_with_tooltip, Space::new().width(Fill), widget.into(),]
        .align_y(Alignment::Center)
        .into()
}

/// Create an indented config row (for sub-options)
pub fn config_row_indented<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    disabled: bool,
) -> Element<'a, Message> {
    let label_color = if disabled { Theme::MUTED } else { Theme::TEXT };
    row![
        Space::new().width(16), // Indent
        text(label).size(12).color(label_color),
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create an indented config row with tooltip
pub fn config_row_indented_with_tooltip<'a>(
    label: &'a str,
    widget: impl Into<Element<'a, Message>>,
    disabled: bool,
    tooltip_text: &'a str,
) -> Element<'a, Message> {
    let label_color = if disabled { Theme::MUTED } else { Theme::TEXT };
    let label_with_tooltip = tooltip(
        text(label).size(12).color(label_color),
        container(text(tooltip_text).size(11))
            .padding(8)
            .style(Theme::tooltip_container),
        tooltip::Position::Left,
    )
    .gap(5);

    row![
        Space::new().width(16), // Indent
        label_with_tooltip,
        Space::new().width(Fill),
        widget.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Create an info row (read-only, right-aligned value) with owned string
pub fn config_row_info_owned<'a>(
    label: &'a str,
    value: String,
    color: Option<iced::Color>,
) -> Element<'a, Message> {
    let value_text = text(value).size(12);
    let value_text = if let Some(c) = color {
        value_text.color(c)
    } else {
        value_text
    };

    row![
        text(label).size(12).color(Theme::MUTED),
        Space::new().width(Fill),
        value_text,
    ]
    .align_y(Alignment::Center)
    .into()
}
