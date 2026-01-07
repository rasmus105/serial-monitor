use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use serial_tui::app::App;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    // Setup signal handling
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    setup_signal_handlers(shutdown_flag.clone());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app with shutdown flag
    let result = App::new()
        .run_with_shutdown(&mut terminal, shutdown_flag)
        .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Setup signal handlers that set the shutdown flag when triggered.
fn setup_signal_handlers(shutdown_flag: Arc<AtomicBool>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        // SIGTERM (kill command, systemd stop)
        let flag = shutdown_flag.clone();
        tokio::spawn(async move {
            if let Ok(mut sig) = signal(SignalKind::terminate()) {
                sig.recv().await;
                flag.store(true, Ordering::SeqCst);
            }
        });

        // SIGINT (Ctrl+C) - crossterm usually handles this, but as backup
        let flag = shutdown_flag.clone();
        tokio::spawn(async move {
            if let Ok(mut sig) = signal(SignalKind::interrupt()) {
                sig.recv().await;
                flag.store(true, Ordering::SeqCst);
            }
        });

        // SIGHUP (terminal closed)
        let flag = shutdown_flag;
        tokio::spawn(async move {
            if let Ok(mut sig) = signal(SignalKind::hangup()) {
                sig.recv().await;
                flag.store(true, Ordering::SeqCst);
            }
        });
    }

    #[cfg(not(unix))]
    {
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                shutdown_flag.store(true, Ordering::SeqCst);
            }
        });
    }
}
