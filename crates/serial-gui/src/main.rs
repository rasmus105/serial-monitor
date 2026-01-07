//! Serial Monitor GUI - Iced-based graphical interface.

use iced::{Element, Task};

fn main() -> iced::Result {
    iced::application("Serial Monitor", App::update, App::view).run()
}

/// Main application state.
#[derive(Default)]
struct App {
    // Placeholder for future state
}

/// Application messages.
#[derive(Debug, Clone)]
enum Message {
    // Placeholder for future messages
}

impl App {
    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        iced::widget::text("Serial Monitor GUI - Hello World!").into()
    }
}
