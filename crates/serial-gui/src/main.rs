//! Serial Monitor GUI - Iced-based graphical interface.

mod app;
mod theme;
mod view;
mod widget_options;

use app::App;
use iced::Font;

/// JetBrains Mono Regular font bytes (embedded at compile time)
const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");

/// JetBrains Mono Bold font bytes (embedded at compile time)
const JETBRAINS_MONO_BOLD: &[u8] = include_bytes!("../fonts/JetBrainsMono-Bold.ttf");

/// JetBrains Mono font for monospace text (data display, inputs)
pub const JETBRAINS_MONO: Font = Font::with_name("JetBrains Mono");

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .default_font(JETBRAINS_MONO)
        .font(JETBRAINS_MONO_REGULAR)
        .font(JETBRAINS_MONO_BOLD)
        .run()
}
