//! Legend panel showing series names, colors, and latest values.

use iced::widget::{Row, button, container, row, text};
use iced::{Alignment, Color, Element};

use serial_core::buffer::graph::GraphMode;

use crate::app::{ConnectedMsg, ConnectedState, Message};
use crate::theme::Theme;

use super::chart::SERIES_COLORS;
use super::state::GraphMsg;

/// Data extracted from series for legend display.
struct SeriesInfo {
    name: String,
    visible: bool,
    latest_value: Option<f64>,
}

/// Data extracted from packet rate for legend display.
struct RateInfo {
    rx_rate: f64,
    tx_rate: f64,
}

/// Render the legend panel.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    let buffer = state.handle.buffer();
    let graph_view = &state.graph_view;
    let mode = graph_view.config.mode();

    let Some(graph) = buffer.graph() else {
        return text("No graph data").size(12).color(Theme::MUTED).into();
    };

    match mode {
        GraphMode::ParsedData => {
            // Extract data while holding the borrow
            let series_infos: Vec<SeriesInfo> = graph
                .series
                .iter()
                .map(|(name, series)| SeriesInfo {
                    name: name.clone(),
                    visible: series.visible,
                    latest_value: series.points.back().map(|p| p.value),
                })
                .collect();
            drop(buffer); // Release the borrow before building UI
            parsed_data_legend(state, series_infos)
        }
        GraphMode::PacketRate => {
            // Extract rate data
            let rate_info = {
                let packet_rate = &graph.config.packet_rate;
                packet_rate
                    .samples
                    .back()
                    .map(|sample| {
                        let window_secs = packet_rate.window_size.as_secs_f64();
                        RateInfo {
                            rx_rate: sample.rx_count as f64 / window_secs,
                            tx_rate: sample.tx_count as f64 / window_secs,
                        }
                    })
                    .unwrap_or(RateInfo {
                        rx_rate: 0.0,
                        tx_rate: 0.0,
                    })
            };
            drop(buffer); // Release the borrow before building UI
            packet_rate_legend(state, rate_info)
        }
    }
}

/// Legend for parsed data mode.
fn parsed_data_legend(
    _state: &ConnectedState,
    series_infos: Vec<SeriesInfo>,
) -> Element<'_, Message> {
    if series_infos.is_empty() {
        return text("No series found").size(12).color(Theme::MUTED).into();
    }

    let items: Vec<Element<'_, Message>> = series_infos
        .into_iter()
        .enumerate()
        .map(|(idx, info)| {
            let color = SERIES_COLORS[idx % SERIES_COLORS.len()];
            let latest_value = info
                .latest_value
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "-".to_string());

            legend_item(info.name, color, info.visible, latest_value)
        })
        .collect();

    // Horizontal layout for legend items (wrap if needed)
    Row::with_children(items)
        .spacing(16)
        .align_y(Alignment::Center)
        .into()
}

/// Legend for packet rate mode.
fn packet_rate_legend(state: &ConnectedState, rate_info: RateInfo) -> Element<'_, Message> {
    let graph_view = &state.graph_view;

    let rx_item = legend_item_rate(
        "RX",
        Theme::RX,
        graph_view.config.show_rx_rate,
        format!("{:.1}/s", rate_info.rx_rate),
        GraphMsg::ToggleParseRx, // Reuse this for rate toggle
    );

    let tx_item = legend_item_rate(
        "TX",
        Theme::TX,
        graph_view.config.show_tx_rate,
        format!("{:.1}/s", rate_info.tx_rate),
        GraphMsg::ToggleParseTx, // Reuse this for rate toggle
    );

    row![rx_item, tx_item]
        .spacing(24)
        .align_y(Alignment::Center)
        .into()
}

/// A single legend item for parsed data series.
fn legend_item(
    name: String,
    color: Color,
    visible: bool,
    value: String,
) -> Element<'static, Message> {
    let color_indicator =
        container(text(""))
            .width(12)
            .height(12)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(if visible {
                    color
                } else {
                    Theme::TEXT_DISABLED
                })),
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

    let label = text(name.clone()).size(12).color(if visible {
        Theme::TEXT_PRIMARY
    } else {
        Theme::TEXT_DISABLED
    });

    let value_text = text(value).size(11).color(Theme::TEXT_SECONDARY);

    let content = row![color_indicator, label, value_text]
        .spacing(6)
        .align_y(Alignment::Center);

    button(content)
        .padding([4, 8])
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered => Theme::BG_HOVER,
                button::Status::Pressed => Theme::BG_ACTIVE,
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: Theme::TEXT_PRIMARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::Connected(ConnectedMsg::Graph(
            GraphMsg::ToggleSeriesVisibility(name),
        )))
        .into()
}

/// A single legend item for packet rate series with explicit toggle.
fn legend_item_rate(
    name: &'static str,
    color: Color,
    visible: bool,
    value: String,
    toggle_msg: GraphMsg,
) -> Element<'static, Message> {
    let color_indicator =
        container(text(""))
            .width(12)
            .height(12)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(if visible {
                    color
                } else {
                    Theme::TEXT_DISABLED
                })),
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

    let label = text(name).size(12).color(if visible {
        Theme::TEXT_PRIMARY
    } else {
        Theme::TEXT_DISABLED
    });

    let value_text = text(value).size(11).color(Theme::TEXT_SECONDARY);

    let content = row![color_indicator, label, value_text]
        .spacing(6)
        .align_y(Alignment::Center);

    button(content)
        .padding([4, 8])
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered => Theme::BG_HOVER,
                button::Status::Pressed => Theme::BG_ACTIVE,
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: Theme::TEXT_PRIMARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::Connected(ConnectedMsg::Graph(toggle_msg)))
        .into()
}
