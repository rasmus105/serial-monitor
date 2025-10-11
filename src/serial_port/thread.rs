use crate::app_state;

pub fn serial_thread(
    rx: std::sync::mpsc::Receiver<app_state::MessageToSerial>,
    to_ui_tx: std::sync::mpsc::Sender<app_state::MessageToUi>,
) {
    loop {}
}
