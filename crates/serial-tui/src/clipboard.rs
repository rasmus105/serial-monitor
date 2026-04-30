use std::io;

use base64::Engine;

pub(crate) fn is_ssh() -> bool {
    std::env::var("SSH_TTY").is_ok()
        || std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok()
}

pub fn copy_osc52(text: &str) -> Result<(), String> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let sequence = format!("\x1b]52;c;{}\x07", encoded);

    crossterm::execute!(io::stdout(), crossterm::style::Print(&sequence))
        .map_err(|e| format!("OSC 52 clipboard error: {e}"))
}

pub fn copy_arboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| format!("Clipboard error: {e}"))
}
