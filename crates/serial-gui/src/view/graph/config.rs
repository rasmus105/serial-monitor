//! Configuration panel for graph settings.

use iced::widget::{Column, button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Fill, Length};

use serial_core::buffer::graph::GraphMode;

use crate::app::{ConnectedMsg, ConnectedState, Message};
use crate::theme::{self, Theme};

use super::state::GraphMsg;

// Option types for pick lists
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeOption(pub usize);

impl std::fmt::Display for ModeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "Parse Data"),
            1 => write!(f, "RX/TX Rate"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserOption(pub usize);

impl std::fmt::Display for ParserOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "Smart"),
            1 => write!(f, "CSV"),
            2 => write!(f, "JSON"),
            3 => write!(f, "Regex"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelimiterOption(pub usize);

impl std::fmt::Display for DelimiterOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "Comma (,)"),
            1 => write!(f, "Semicolon (;)"),
            2 => write!(f, "Tab"),
            3 => write!(f, "Space"),
            4 => write!(f, "Pipe (|)"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRangeOption(pub usize);

impl std::fmt::Display for TimeRangeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "All"),
            1 => write!(f, "1 Hour"),
            2 => write!(f, "5 Min"),
            3 => write!(f, "Custom"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeUnitOption(pub usize);

impl std::fmt::Display for TimeUnitOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "seconds"),
            1 => write!(f, "minutes"),
            2 => write!(f, "hours"),
            _ => write!(f, "Unknown"),
        }
    }
}

/// Render the graph configuration panel.
pub fn view(state: &ConnectedState) -> Element<'_, Message> {
    let graph_view = &state.graph_view;
    let config = &graph_view.config;
    let mode = config.mode();

    let mut sections = Column::new().spacing(theme::spacing::SECTION_GAP as u32);

    // Connection info section
    sections = sections.push(connection_section(state));

    // Display section (mode selection)
    sections = sections.push(display_section(state));

    // Parser section (only for ParsedData mode)
    if mode == GraphMode::ParsedData {
        sections = sections.push(parser_section(state));
    }

    // Time range section
    sections = sections.push(time_range_section(state));

    container(sections)
        .width(220)
        .height(Fill)
        .padding(theme::spacing::CONTAINER)
        .style(Theme::bordered_container)
        .into()
}

/// Connection info section.
fn connection_section(state: &ConnectedState) -> Element<'_, Message> {
    let is_collapsed = state.collapsed_sections.contains("Connection");

    let header = section_header(
        "Connection",
        is_collapsed,
        Message::Connected(ConnectedMsg::ToggleSectionCollapse(
            "Connection".to_string(),
        )),
    );

    if is_collapsed {
        return header;
    }

    let stats = state.handle.statistics();

    let content = column![
        config_row("Port", text(&state.port_name).size(12)),
        config_row("Baud", text(format!("{}", state.config.baud_rate)).size(12)),
        config_row("RX", text(format!("{} bytes", stats.bytes_rx())).size(12)),
        config_row("TX", text(format!("{} bytes", stats.bytes_tx())).size(12)),
    ]
    .spacing(4);

    column![header, container(content).padding(8)].into()
}

/// Display section with mode selection.
fn display_section(state: &ConnectedState) -> Element<'_, Message> {
    let is_collapsed = state.collapsed_sections.contains("Display");
    let config = &state.graph_view.config;

    let header = section_header(
        "Display",
        is_collapsed,
        Message::Connected(ConnectedMsg::ToggleSectionCollapse("Display".to_string())),
    );

    if is_collapsed {
        return header;
    }

    let mode_options: Vec<ModeOption> = (0..2).map(ModeOption).collect();
    let mode_pick = pick_list(mode_options, Some(ModeOption(config.mode_index)), |opt| {
        Message::Connected(ConnectedMsg::Graph(GraphMsg::SetMode(opt.0)))
    })
    .width(Length::Fill)
    .style(Theme::pick_list);

    let content = column![config_row("Mode", mode_pick),].spacing(4);

    column![header, container(content).padding(8)].into()
}

/// Parser section for parsed data mode.
fn parser_section(state: &ConnectedState) -> Element<'_, Message> {
    let is_collapsed = state.collapsed_sections.contains("Parser");
    let config = &state.graph_view.config;

    let header = section_header(
        "Parser",
        is_collapsed,
        Message::Connected(ConnectedMsg::ToggleSectionCollapse("Parser".to_string())),
    );

    if is_collapsed {
        return header;
    }

    let parser_options: Vec<ParserOption> = (0..4).map(ParserOption).collect();
    let parser_pick = pick_list(
        parser_options,
        Some(ParserOption(config.parser_type_index)),
        |opt| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetParserType(opt.0))),
    )
    .width(Length::Fill)
    .style(Theme::pick_list);

    let mut content = Column::new().spacing(4);
    content = content.push(config_row("Type", parser_pick));

    // Parser-specific options
    match config.parser_type_index {
        1 => {
            // CSV options
            let delimiter_options: Vec<DelimiterOption> = (0..5).map(DelimiterOption).collect();
            let delimiter_pick = pick_list(
                delimiter_options,
                Some(DelimiterOption(config.csv_delimiter_index)),
                |opt| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetCsvDelimiter(opt.0))),
            )
            .width(Length::Fill)
            .style(Theme::pick_list);

            let columns_input = text_input("col1,col2,...", &config.csv_columns)
                .on_input(|s| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetCsvColumns(s))))
                .size(12)
                .padding(6)
                .style(Theme::text_input);

            content = content.push(config_row("Delimiter", delimiter_pick));
            content = content.push(config_row("Columns", columns_input));
        }
        3 => {
            // Regex options
            let pattern_input = text_input("(?P<name>\\d+)", &config.regex_pattern)
                .on_input(|s| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetRegexPattern(s))))
                .size(12)
                .padding(6)
                .style(Theme::text_input);

            content = content.push(config_row("Pattern", pattern_input));
        }
        _ => {}
    }

    // Parse direction toggles
    let rx_checkbox = checkbox(config.parse_rx)
        .label("Parse RX")
        .on_toggle(|_| Message::Connected(ConnectedMsg::Graph(GraphMsg::ToggleParseRx)))
        .size(14)
        .text_size(12);

    let tx_checkbox = checkbox(config.parse_tx)
        .label("Parse TX")
        .on_toggle(|_| Message::Connected(ConnectedMsg::Graph(GraphMsg::ToggleParseTx)))
        .size(14)
        .text_size(12);

    content = content.push(row![rx_checkbox, tx_checkbox].spacing(12));

    // Apply button
    let apply_btn = button(text("Apply").size(12))
        .padding([4, 12])
        .style(Theme::button_primary)
        .on_press(Message::Connected(ConnectedMsg::Graph(
            GraphMsg::ApplyParserChanges,
        )));

    content = content.push(container(apply_btn).padding(8));

    column![header, container(content).padding(8)].into()
}

/// Time range section.
fn time_range_section(state: &ConnectedState) -> Element<'_, Message> {
    let is_collapsed = state.collapsed_sections.contains("Time Range");
    let config = &state.graph_view.config;

    let header = section_header(
        "Time Range",
        is_collapsed,
        Message::Connected(ConnectedMsg::ToggleSectionCollapse(
            "Time Range".to_string(),
        )),
    );

    if is_collapsed {
        return header;
    }

    let range_options: Vec<TimeRangeOption> = (0..4).map(TimeRangeOption).collect();
    let range_pick = pick_list(
        range_options,
        Some(TimeRangeOption(config.time_range_index)),
        |opt| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetTimeRange(opt.0))),
    )
    .width(Length::Fill)
    .style(Theme::pick_list);

    let mut content = Column::new().spacing(4);
    content = content.push(config_row("Range", range_pick));

    // Custom time input (only when Custom is selected)
    if config.time_range_index == 3 {
        let value_input = text_input("60", &config.custom_time_value.to_string())
            .on_input(|s| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetCustomTimeValue(s))))
            .width(60)
            .size(12)
            .padding(6)
            .style(Theme::text_input);

        let unit_options: Vec<TimeUnitOption> = (0..3).map(TimeUnitOption).collect();
        let unit_pick = pick_list(
            unit_options,
            Some(TimeUnitOption(config.custom_time_unit_index)),
            |opt| Message::Connected(ConnectedMsg::Graph(GraphMsg::SetCustomTimeUnit(opt.0))),
        )
        .width(Length::Fill)
        .style(Theme::pick_list);

        content = content.push(
            row![value_input, unit_pick]
                .spacing(8)
                .align_y(Alignment::Center),
        );
    }

    column![header, container(content).padding(8)].into()
}

/// Helper to create a config row with label and widget.
fn config_row<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![
        text(label).size(12).color(Theme::TEXT_SECONDARY).width(70),
        widget.into(),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// Section header helper (collapsible).
pub fn section_header<'a>(
    title: &'a str,
    is_collapsed: bool,
    on_toggle: Message,
) -> Element<'a, Message> {
    let arrow = if is_collapsed { ">" } else { "v" };

    let header_content = row![
        text(arrow).size(12).color(Theme::TEXT_SECONDARY),
        text(title).size(13).color(Theme::TEXT_PRIMARY),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    button(
        container(header_content)
            .width(Fill)
            .style(Theme::section_header_container),
    )
    .width(Fill)
    .padding(4)
    .style(Theme::button_section_header)
    .on_press(on_toggle)
    .into()
}
