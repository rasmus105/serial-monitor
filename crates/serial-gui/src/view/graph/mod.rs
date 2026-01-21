//! Graph view for visualizing parsed data series.

mod chart;
mod config;
mod legend;
mod state;

pub use state::{GraphMsg, GraphView};

use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill, Length};

use crate::app::{ConnectedMsg, ConnectedState, Message};
use crate::theme::Theme;

/// Render the graph view.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    // Main layout: chart area + config panel
    let main_content: Element<'_, Message> = if state.show_config_panel {
        row![chart_area(state), config::view(state),]
            .spacing(5)
            .into()
    } else {
        // When panel is hidden, show a small button to reveal it
        let show_panel_btn = button(text(">").size(14))
            .on_press(Message::Connected(ConnectedMsg::ToggleConfigPanel))
            .padding([10, 5]);

        row![
            chart_area(state),
            container(show_panel_btn)
                .height(Fill)
                .align_y(iced::Alignment::Center),
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

/// Render the main chart area with legend.
fn chart_area(state: &ConnectedState) -> Element<'_, Message> {
    let buffer = state.handle.buffer();

    // Check if graph is enabled
    if !buffer.graph_enabled() {
        return container(
            column![
                text("Graph not enabled")
                    .size(16)
                    .color(Theme::TEXT_SECONDARY),
                text("Switch to Graph tab to enable parsing")
                    .size(13)
                    .color(Theme::MUTED),
            ]
            .spacing(8)
            .align_x(iced::Alignment::Center),
        )
        .width(Fill)
        .height(Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .style(Theme::bordered_container)
        .into();
    }

    // Layout: chart on top, legend below (or side by side on wide screens)
    let chart_widget = chart::view(state);
    let legend_widget = legend::view(state);

    column![
        container(chart_widget)
            .width(Fill)
            .height(Fill)
            .style(Theme::bordered_container),
        container(legend_widget)
            .width(Fill)
            .height(Length::Shrink)
            .padding(5),
    ]
    .spacing(5)
    .width(Fill)
    .height(Fill)
    .into()
}
