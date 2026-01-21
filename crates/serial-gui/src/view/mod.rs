//! View modules for different application states.

pub mod graph;
pub mod pre_connect;
pub mod traffic;

use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill};

use crate::app::{ConnectedMsg, ConnectedState, Message, ViewTab};
use crate::theme::Theme;

/// Render the connected state view with tab bar.
pub fn connected_view(state: &ConnectedState) -> Element<'_, Message> {
    // Tab bar
    let tab_bar = row(ViewTab::ALL.iter().map(|&tab| {
        let is_active = state.active_tab == tab;
        let label = text(tab.label()).size(13);

        let btn = button(label)
            .padding([6, 16])
            .style(move |theme, status| {
                if is_active {
                    tab_button_active(theme, status)
                } else {
                    tab_button_inactive(theme, status)
                }
            })
            .on_press(Message::Connected(ConnectedMsg::SwitchTab(tab)));

        btn.into()
    }))
    .spacing(2)
    .padding(10);

    // Tab content
    let content: Element<'_, Message> = match state.active_tab {
        ViewTab::Traffic => traffic::view(state),
        ViewTab::Graph => graph::view(state),
    };

    column![
        container(tab_bar)
            .width(Fill)
            .padding(10)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Theme::BG_BASE)),
                ..Default::default()
            }),
        content,
    ]
    .into()
}

// Tab button styles
fn tab_button_active(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(iced::Background::Color(Theme::BG_SURFACE)),
        text_color: Theme::TEXT_PRIMARY,
        border: iced::Border {
            color: Theme::ACCENT_PRIMARY,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn tab_button_inactive(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Theme::BG_HOVER,
        _ => Theme::BG_BASE,
    };
    button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color: Theme::TEXT_SECONDARY,
        border: iced::Border {
            color: Theme::BORDER,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
