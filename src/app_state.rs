use eframe::egui;
use serde::{Deserialize, Serialize};

/// Messages to UI thread (contains all possible data that can be sent to the]
/// UI thread)
pub enum MessageToUi {
    LostConnection,
    Connected(String), // contains device which was connected to
}

/// Messages to Serial Connection thread (contains all possible data that
/// can be sent to the serial thread)
pub enum MessageToSerial {
    Connect(String), // request a connection to a specific device
}

/// For all UI state
#[derive(Default)]
pub struct UiState {}

pub enum Connection {
    NotConnected,
    Connected(String), // contains device which is connected to
}

impl Default for Connection {
    fn default() -> Self {
        Self::NotConnected
    }
}

/// For connection state
#[derive(Default)]
pub struct ConnectionState {
    state: Connection, // connected or not connected
    available_devices: (),
}

/// For all data
#[derive(Default)]
pub struct TrafficState {
    /// Received data parsed into line-seperated utf8 strings.
    utf8: (),

    /// `utf8` traffic filtered.
    filtered_utf8: (),

    /// Raw binary traffic
    raw: (),
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

impl Theme {
    fn apply(self, ctx: &egui::Context) {
        match self {
            Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct SettingsState {
    #[serde(default)]
    shortcuts: (),
    #[serde(default)]
    theme: Theme,
}

pub struct Configuration {
    show_timestamps: bool,
    auto_scroll: bool,
    lock_to_bottom: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            show_timestamps: true,
            auto_scroll: true,
            lock_to_bottom: true,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    /// UI specific states (e.g. which view is selected, etc.)
    pub ui: UiState,

    /// Whether connected, available devices,
    pub connection: ConnectionState,

    /// All data stored
    pub traffic: TrafficState,

    /// Persistent application settings (saved to configuration file)
    pub settings: SettingsState,

    /// Session-specific configuration (should timestamps be shown, auto-scroll, etc.)
    pub config: Configuration,
}

impl AppState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = AppState::default();

        if let Some(storage) = cc.storage {
            if let Some(settings) = eframe::get_value::<SettingsState>(storage, eframe::APP_KEY) {
                app.settings = settings;
            }
        }

        app.apply_theme(&cc.egui_ctx);

        app
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        self.settings.theme.apply(ctx);
    }
}

impl eframe::App for AppState {
    // This is the method that saves the state.
    // It's called automatically when the app is about to close.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // `eframe::set_value` is a helper to serialize and store the state.
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }

    // The regular update method to draw the UI.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Persistent State Example");
            ui.separator();

            ui.label("This text will be saved when you close the app:");
            ui.separator();

            ui.heading("Appearance");
            ui.horizontal(|ui| {
                ui.label("Theme:");
                let mut changed = false;
                changed |= ui
                    .selectable_value(&mut self.settings.theme, Theme::Light, "Light")
                    .changed();
                changed |= ui
                    .selectable_value(&mut self.settings.theme, Theme::Dark, "Dark")
                    .changed();

                if changed {
                    self.apply_theme(ctx);
                }
            });
        });
    }
}
