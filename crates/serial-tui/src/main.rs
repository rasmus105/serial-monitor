//! Serial Monitor TUI - Main Entry Point

use std::io;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use serial_tui::App;

mod event_loop {
    use serial_tui::App;
    use crate::event::{poll_event, is_quit_key, AppEvent};

    pub fn run(app: &mut App, terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>) -> std::io::Result<()> {
        loop {
            // Poll for session events
            app.poll_session_events();
            
            // Poll for file send progress
            app.poll_file_send();

            // Render
            terminal.draw(|frame| serial_tui::ui::render(frame, app))?;

            // Handle input
            if let Some(event) = poll_event(app.tick_rate())? {
                match event {
                    AppEvent::Key(key) => {
                        if is_quit_key(&key) && app.view == serial_tui::app::View::PortSelect {
                            app.should_quit = true;
                        } else {
                            app.handle_key(key);
                        }
                    }
                    AppEvent::Resize(_, _) => {
                        // Terminal will handle resize automatically
                    }
                    AppEvent::Tick => {
                        // Just continue the loop
                    }
                }
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    }
}

mod event {
    pub use serial_tui::event::{poll_event, is_quit_key, AppEvent};
}

fn main() -> io::Result<()> {
    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new()?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(runtime.handle().clone());

    // Run event loop
    let result = event_loop::run(&mut app, &mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
