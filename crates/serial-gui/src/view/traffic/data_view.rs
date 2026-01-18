//! Main traffic data display area with virtual scrolling.

use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Fill, Length};
use serial_core::Direction;
use serial_core::ui::TimestampFormat;
use std::time::{Duration, SystemTime};

use crate::app::{ConnectedMsg, ConnectedState, Message, ScrollState, VisibleChunkCache};
use crate::theme::{Theme, font_size, spacing};

use super::widgets::{LINE_ENDING_OPTIONS, LineEndingOption};

/// Estimated row height in pixels for virtual scrolling calculations
const ROW_HEIGHT: f32 = 20.0;
/// Number of extra rows to render above and below the viewport as buffer
const RENDER_BUFFER: usize = 10;
/// Default viewport lines when viewport height is unknown
const DEFAULT_VIEWPORT_LINES: usize = 50;
/// Default viewport height when unknown
const DEFAULT_VIEWPORT_HEIGHT: f32 = 500.0;
/// Fixed width for TX/RX prefix container
const PREFIX_WIDTH: f32 = 24.0;

/// Build the main traffic display area (data lines + send input)
pub fn traffic_area(state: &ConnectedState) -> Element<'_, Message> {
    // Borrow buffer directly - no intermediate storage needed
    let buffer = state.handle.buffer();
    let total_lines = buffer.len();

    // Data display with virtual scrolling
    let data_content: Element<'_, Message> = if total_lines == 0 {
        // Clear cache when buffer is empty
        *state.visible_cache.borrow_mut() = None;

        container(
            text("No data yet...")
                .color(Theme::TEXT_SECONDARY)
                .size(font_size::BODY),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else {
        let timestamp_width = if state.show_timestamps {
            max_timestamp_width(state.timestamp_format_index, state.session_start)
        } else {
            0
        };

        // Calculate visible range based on scroll state
        let (visible_start, visible_end) = match &state.scroll_state {
            ScrollState::LockedToBottom => {
                // When locked to bottom, we render the last N lines that fit
                let viewport_lines = state
                    .viewport_height
                    .map(|h| (h / ROW_HEIGHT).ceil() as usize)
                    .unwrap_or(DEFAULT_VIEWPORT_LINES);
                let start = total_lines.saturating_sub(viewport_lines + RENDER_BUFFER);
                (start, total_lines)
            }
            ScrollState::Off { offset }
            | ScrollState::AutoScroll { offset }
            | ScrollState::Manual { offset } => {
                // Calculate which lines are visible based on scroll offset
                let viewport_height = state.viewport_height.unwrap_or(DEFAULT_VIEWPORT_HEIGHT);
                let start_line = (*offset / ROW_HEIGHT).floor() as usize;
                let visible_count = (viewport_height / ROW_HEIGHT).ceil() as usize;

                let visible_start = start_line.saturating_sub(RENDER_BUFFER);
                let visible_end = (start_line + visible_count + RENDER_BUFFER).min(total_lines);
                (visible_start, visible_end)
            }
        };

        // Top spacer to maintain scroll position
        let top_spacer_height = visible_start as f32 * ROW_HEIGHT;

        // Check if cache is valid and update if needed
        // Cache is valid if: same visible range, same buffer length, same encoding
        let needs_rebuild = {
            let cache = state.visible_cache.borrow();
            match cache.as_ref() {
                Some(c) => {
                    c.start_index != visible_start
                        || c.end_index != visible_end
                        || c.buffer_len != total_lines
                        || c.encoding != state.encoding
                }
                None => true,
            }
        };

        if needs_rebuild {
            // Clone visible chunks and store in cache
            let chunks: Vec<_> = buffer
                .chunks()
                .skip(visible_start)
                .take(visible_end - visible_start)
                .map(|chunk| (chunk.direction, chunk.encoded.to_string(), chunk.timestamp))
                .collect();

            *state.visible_cache.borrow_mut() = Some(VisibleChunkCache {
                chunks,
                start_index: visible_start,
                end_index: visible_end,
                buffer_len: total_lines,
                encoding: state.encoding,
            });
        }

        // Drop the buffer guard before building widgets
        drop(buffer);

        // Build visible lines from cached data
        let cache = state.visible_cache.borrow();
        let cached_chunks = &cache.as_ref().expect("cache should be populated").chunks;

        let visible_lines: Vec<Element<'_, Message>> = cached_chunks
            .iter()
            .map(|(direction, content, timestamp)| {
                let (prefix, color) = match direction {
                    Direction::Tx => ("TX", Theme::TX),
                    Direction::Rx => ("RX", Theme::RX),
                };

                // Fixed-width container for TX/RX prefix
                let prefix_container = container(text(prefix).color(color).size(font_size::BODY))
                    .width(Length::Fixed(PREFIX_WIDTH));

                let mut line_row =
                    row![prefix_container, Space::new().width(8),].align_y(Alignment::Center);

                if state.show_timestamps {
                    let ts_str = format_timestamp(
                        *timestamp,
                        state.session_start,
                        state.timestamp_format_index,
                    );
                    // Right-align timestamp within fixed width
                    let padded_timestamp = format!("{:>width$}", ts_str, width = timestamp_width);
                    line_row = line_row.push(
                        text(padded_timestamp)
                            .color(Theme::TEXT_SECONDARY)
                            .size(font_size::BODY),
                    );
                    line_row = line_row.push(Space::new().width(10));
                }

                line_row = line_row.push(text(content.clone()).size(font_size::BODY));
                line_row.into()
            })
            .collect();

        // Must drop the cache borrow before we can create the scrollable
        // (because we're returning an Element that borrows from state)
        drop(cache);

        // Bottom spacer to maintain total scroll height
        let bottom_spacer_height = (total_lines - visible_end) as f32 * ROW_HEIGHT;

        // Build scrollable content with spacers for virtual scrolling
        let content = column![
            Space::new().width(Fill).height(top_spacer_height),
            column(visible_lines).spacing(2),
            Space::new().width(Fill).height(bottom_spacer_height),
        ]
        .padding(spacing::CONTAINER)
        .width(Fill);

        // Determine if we should anchor to bottom
        let scrollable_widget = scrollable(content)
            .height(Fill)
            .width(Fill)
            .on_scroll(|viewport| Message::Connected(ConnectedMsg::ScrollChanged(viewport)))
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(8).scroller_width(8),
            ))
            .style(Theme::scrollbar);

        // Only anchor to bottom when locked
        let scrollable_widget = if matches!(state.scroll_state, ScrollState::LockedToBottom) {
            scrollable_widget.anchor_bottom()
        } else {
            scrollable_widget
        };

        scrollable_widget.into()
    };

    // Line ending selector - use static options to avoid allocation
    let current_line_ending = LineEndingOption(state.send_line_ending_index);
    let line_ending_picker = pick_list(LINE_ENDING_OPTIONS, Some(current_line_ending), |opt| {
        Message::Connected(ConnectedMsg::SelectSendLineEnding(opt.0))
    })
    .width(120)
    .style(Theme::pick_list);

    // Send input
    let send_input = text_input("Type to send...", &state.send_input)
        .on_input(|input| Message::Connected(ConnectedMsg::SendInput(input)))
        .on_submit(Message::Connected(ConnectedMsg::Send))
        .width(Fill)
        .style(Theme::text_input);

    let send_btn = button(text("Send"))
        .on_press(Message::Connected(ConnectedMsg::Send))
        .style(Theme::button_primary);

    let send_row = row![
        send_input,
        Space::new().width(spacing::ROW_GAP as f32),
        line_ending_picker,
        Space::new().width(spacing::ROW_GAP as f32),
        send_btn,
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    // Wrap send row in a container with matching horizontal padding to the traffic content
    // Traffic content has: 1px border + CONTAINER padding inside
    // So send row needs: CONTAINER + 1 padding to align with the inner content
    let send_container = container(send_row).padding([0, spacing::CONTAINER + 1]);

    container(
        column![
            container(data_content)
                .width(Fill)
                .height(Fill)
                .style(Theme::bordered_container),
            Space::new().height(5),
            send_container,
        ]
        .spacing(0),
    )
    .width(Fill)
    .into()
}

/// Format a timestamp according to the selected format
fn format_timestamp(
    timestamp: SystemTime,
    session_start: SystemTime,
    format_index: usize,
) -> String {
    let format = match format_index {
        0 => TimestampFormat::Relative,
        1 => TimestampFormat::AbsoluteMillis,
        2 => TimestampFormat::Absolute,
        _ => TimestampFormat::Relative,
    };
    format.format(timestamp, session_start)
}

/// Get the maximum timestamp width for the current format
///
/// For relative timestamps, the width depends on session duration.
/// For absolute timestamps, the width is fixed.
fn max_timestamp_width(format_index: usize, session_start: SystemTime) -> usize {
    match format_index {
        0 => {
            // Relative: "+X.XXXs" where X grows with time
            let elapsed = SystemTime::now()
                .duration_since(session_start)
                .unwrap_or(Duration::ZERO);
            let secs = elapsed.as_secs_f64();
            let whole_part = secs as u64;
            let digits = if whole_part == 0 {
                1
            } else {
                (whole_part as f64).log10().floor() as usize + 1
            };
            // "+", digits, ".", 3 decimal digits, "s"
            1 + digits + 1 + 3 + 1
        }
        1 => 12, // "HH:MM:SS.mmm"
        2 => 8,  // "HH:MM:SS"
        _ => 10,
    }
}
