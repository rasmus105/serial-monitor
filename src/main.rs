#![allow(unused)]

/// Application modules
mod app_state;
mod serial_port;

/// Imports
use std::{
    sync::mpsc::{self},
    thread,
};

/// Main entry point
fn main() {
    // Initialize env_logger for debugging
    env_logger::init();

    let (to_ui_tx, to_ui_rx) = mpsc::channel::<app_state::MessageToUi>();
    let (to_serial_tx, to_serial_rx) = mpsc::channel::<app_state::MessageToSerial>();

    thread::spawn(move || serial_port::thread::serial_thread(to_serial_rx, to_ui_tx));

    let eframe_options = eframe::NativeOptions::default();

    let _ = eframe::run_native(
        "Serial Monitor",
        eframe_options,
        Box::new(|cc| Ok(Box::new(app_state::AppState::new(cc)))),
    );
}
