//! Toast notification system.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

/// Toast notification level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    pub fn style(self) -> Style {
        match self {
            ToastLevel::Info => Theme::info(),
            ToastLevel::Success => Theme::success(),
            ToastLevel::Warning => Theme::warning(),
            ToastLevel::Error => Theme::error(),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToastLevel::Info => "i",
            ToastLevel::Success => "+",
            ToastLevel::Warning => "!",
            ToastLevel::Error => "x",
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Info)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Success)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Error)
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }

    /// Progress from 0.0 (just created) to 1.0 (expired).
    pub fn progress(&self) -> f64 {
        self.created.elapsed().as_secs_f64() / self.duration.as_secs_f64()
    }
}

/// Toast notification container.
#[derive(Debug, Default)]
pub struct Toasts {
    toasts: VecDeque<Toast>,
    max_visible: usize,
}

impl Toasts {
    pub fn new() -> Self {
        Self {
            toasts: VecDeque::new(),
            max_visible: 5,
        }
    }

    pub fn push(&mut self, toast: Toast) {
        self.toasts.push_back(toast);
        // Limit total toasts to prevent memory issues
        while self.toasts.len() > 20 {
            self.toasts.pop_front();
        }
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.push(Toast::info(message));
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.push(Toast::success(message));
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(Toast::warning(message));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(Toast::error(message));
    }

    /// Remove expired toasts. Returns true if any toasts were removed.
    pub fn tick(&mut self) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|t| !t.is_expired());
        self.toasts.len() < before
    }

    /// Check if there are any toasts to display.
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }
}

/// Widget for rendering toasts.
pub struct ToastsWidget<'a> {
    toasts: &'a Toasts,
}

impl<'a> ToastsWidget<'a> {
    pub fn new(toasts: &'a Toasts) -> Self {
        Self { toasts }
    }
}

impl Widget for ToastsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.toasts.toasts.is_empty() {
            return;
        }

        // Calculate toast area (top-right corner)
        let toast_width = 40.min(area.width.saturating_sub(4));
        let toast_x = area.x + area.width.saturating_sub(toast_width + 2);

        let visible = self.toasts.toasts.iter().take(self.toasts.max_visible);

        let mut y = area.y + 1;
        for toast in visible {
            if y + 3 > area.y + area.height {
                break;
            }

            let toast_area = Rect::new(toast_x, y, toast_width, 3);

            // Clear background
            Clear.render(toast_area, buf);

            // Render toast
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(toast.level.style())
                .title(format!("[{}]", toast.level.icon()));

            let inner = block.inner(toast_area);
            block.render(toast_area, buf);

            Paragraph::new(toast.message.as_str())
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .render(inner, buf);

            y += 4;
        }
    }
}

/// Render toasts as an overlay.
pub fn render_toasts(toasts: &Toasts, area: Rect, buf: &mut Buffer) {
    ToastsWidget::new(toasts).render(area, buf);
}
