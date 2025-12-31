//! Loading overlay widget with smart delayed display.
//!
//! Only shows the loading indicator if the operation takes longer than a
//! configurable threshold, preventing visual noise for fast operations.

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::theme::Theme;

/// Default delay before showing the loading overlay (150ms).
pub const DEFAULT_SHOW_DELAY: Duration = Duration::from_millis(150);

/// Minimum time to show the overlay once visible (300ms).
/// Prevents a brief flash if the operation completes right after the threshold.
pub const MIN_VISIBLE_DURATION: Duration = Duration::from_millis(300);

/// State for a pending loading operation.
#[derive(Debug, Clone)]
pub struct LoadingState {
    /// When the operation started.
    pub started: Instant,
    /// Message to display.
    pub message: String,
    /// Delay before showing the overlay.
    pub show_delay: Duration,
    /// When the overlay became visible (if it has).
    visible_since: Option<Instant>,
}

impl LoadingState {
    /// Create a new loading state with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            started: Instant::now(),
            message: message.into(),
            show_delay: DEFAULT_SHOW_DELAY,
            visible_since: None,
        }
    }

    /// Create a loading state with a custom show delay.
    pub fn with_delay(message: impl Into<String>, delay: Duration) -> Self {
        Self {
            started: Instant::now(),
            message: message.into(),
            show_delay: delay,
            visible_since: None,
        }
    }

    /// Check if the overlay should be visible now.
    pub fn should_show(&self) -> bool {
        self.started.elapsed() >= self.show_delay
    }

    /// Check if the loading operation can be dismissed.
    ///
    /// Returns true if either:
    /// - The overlay was never shown (fast operation)
    /// - The overlay has been visible for at least MIN_VISIBLE_DURATION
    pub fn can_dismiss(&self) -> bool {
        match self.visible_since {
            None => true, // Never shown, can dismiss immediately
            Some(visible_start) => visible_start.elapsed() >= MIN_VISIBLE_DURATION,
        }
    }

    /// Mark the overlay as visible. Call this when rendering if should_show() is true.
    pub fn mark_visible(&mut self) {
        if self.visible_since.is_none() && self.should_show() {
            self.visible_since = Some(Instant::now());
        }
    }

    /// Get elapsed time since the operation started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

/// Loading overlay widget.
///
/// Renders a centered modal overlay with a spinner and message.
pub struct LoadingOverlay<'a> {
    state: &'a LoadingState,
}

impl<'a> LoadingOverlay<'a> {
    pub fn new(state: &'a LoadingState) -> Self {
        Self { state }
    }

    /// Get a spinner character based on elapsed time.
    fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['|', '/', '-', '\\'];
        let idx = (self.state.elapsed().as_millis() / 100) as usize % SPINNER.len();
        SPINNER[idx]
    }
}

impl Widget for LoadingOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Only render if past the delay threshold
        if !self.state.should_show() {
            return;
        }

        // Calculate overlay size (compact, centered)
        let message_len = self.state.message.len() as u16;
        let width = (message_len + 8).min(area.width.saturating_sub(4)).max(20);
        let height = 3;

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let overlay_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(overlay_area, buf);

        // Block with border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border_focused());

        let inner = block.inner(overlay_area);
        block.render(overlay_area, buf);

        // Content: spinner + message
        let content = Line::from(format!("{} {}", self.spinner_char(), self.state.message));
        Paragraph::new(content)
            .alignment(Alignment::Center)
            .render(inner, buf);
    }
}
