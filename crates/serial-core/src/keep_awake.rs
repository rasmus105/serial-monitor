//! Simple keep-awake functionality to prevent system sleep during serial sessions.

use keepawake::KeepAwake as KeepAwakeInner;

/// Prevents the system from sleeping while enabled.
pub struct KeepAwake {
    inner: Option<KeepAwakeInner>,
}

impl KeepAwake {
    /// Create a new keep-awake handle (starts disabled).
    pub fn new() -> Self {
        Self { inner: None }
    }

    /// Enable keep-awake, preventing system sleep.
    pub fn enable(&mut self) {
        if self.inner.is_some() {
            return;
        }
        self.inner = keepawake::Builder::default()
            .idle(true)
            .reason("Serial monitor session active")
            .app_name("serial-monitor")
            .app_reverse_domain("io.github.serial-monitor")
            .create()
            .ok();
    }

    /// Disable keep-awake, allowing system sleep.
    pub fn disable(&mut self) {
        self.inner = None;
    }

    /// Set enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled {
            self.enable();
        } else {
            self.disable();
        }
    }

    /// Returns true if keep-awake is currently active.
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::new()
    }
}
