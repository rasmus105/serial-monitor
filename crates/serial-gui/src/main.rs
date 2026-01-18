//! Serial Monitor GUI - Iced-based graphical interface.

mod app;
mod theme;
mod view;
mod widget_options;

use app::App;
use iced::Font;

/// JetBrains Mono font bytes (embedded at compile time)
const JETBRAINS_MONO_BYTES: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");

/// JetBrains Mono font for monospace text (data display, inputs)
pub const JETBRAINS_MONO: Font = Font::with_name("JetBrains Mono");

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .default_font(JETBRAINS_MONO)
        .font(JETBRAINS_MONO_BYTES)
        .run()
}
