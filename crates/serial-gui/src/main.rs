//! Serial Monitor GUI - Iced-based graphical interface.

mod app;
mod theme;
mod view;
mod widget_options;

use app::App;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .run()
}
