//! Traffic view for connected state - displays sent/received data.

mod config_panel;
mod data_view;
mod widgets;

use iced::widget::{button, container, row, text};
use iced::{Alignment, Element, Fill};

use crate::app::{ConnectedMsg, ConnectedState, Message};

/// Render the traffic view.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    // Main content area with optional config panel
    let main_content: Element<'_, Message> = if state.show_config_panel {
        row![data_view::traffic_area(state), config_panel::view(state),]
            .spacing(5)
            .into()
    } else {
        // When panel is hidden, show a small button to reveal it
        let show_panel_btn = button(text(">").size(14))
            .on_press(Message::Connected(ConnectedMsg::ToggleConfigPanel))
            .padding([10, 5]);

        row![
            data_view::traffic_area(state),
            container(show_panel_btn)
                .height(Fill)
                .align_y(Alignment::Center),
        ]
        .spacing(5)
        .into()
    };

    container(main_content)
        .width(Fill)
        .height(Fill)
        .padding(10)
        .into()
}
