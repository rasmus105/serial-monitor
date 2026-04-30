use std::io::{self, Write};

use base64::Engine;

fn is_ssh() -> bool {
    std::env::var("SSH_TTY").is_ok()
        || std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok()
}

fn is_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

fn write_osc52(text: &str) -> io::Result<()> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let inner = format!("\x1b]52;c;{}\x07", encoded);

    let sequence = if is_tmux() {
        format!("\x1bPtmux;{inner}\x1b\\")
    } else {
        inner
    };

    let mut stdout = io::stdout().lock();
    stdout.write_all(sequence.as_bytes())?;
    stdout.flush()
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if is_ssh() {
        return write_osc52(text).map_err(|e| format!("OSC 52 clipboard error: {e}"));
    }

    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| format!("Clipboard error: {e}"))
}
