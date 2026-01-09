//! Input history for text inputs with navigation support.
//!
//! Provides a non-persistent history with Ctrl+p/Ctrl+n navigation.
//! When navigating, the current unsaved input is preserved and restored
//! when returning to the "bottom" of the history.

/// Maximum number of entries in history.
const MAX_HISTORY_SIZE: usize = 100;

/// Manages input history for a text field.
#[derive(Debug, Clone, Default)]
pub struct InputHistory {
    /// Past entries (oldest first).
    entries: Vec<String>,
    /// Current position in history (None = editing new input).
    position: Option<usize>,
    /// Saved input when navigating history.
    draft: String,
}

impl InputHistory {
    /// Add a new entry to history.
    ///
    /// Skips empty strings and duplicates of the most recent entry.
    /// Resets navigation position.
    pub fn push(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.is_empty() {
            return;
        }
        // Avoid consecutive duplicates
        if self.entries.last().is_some_and(|last| last == &entry) {
            return;
        }
        self.entries.push(entry);
        // Enforce max size by removing oldest
        if self.entries.len() > MAX_HISTORY_SIZE {
            self.entries.remove(0);
        }
        // Reset position after adding
        self.position = None;
        self.draft.clear();
    }

    /// Navigate to previous (older) entry.
    ///
    /// Returns the entry to display, or None if already at oldest.
    /// On first navigation, saves the current input as draft.
    pub fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        let new_pos = match self.position {
            None => {
                // First navigation: save current as draft, go to newest
                self.draft = current_input.to_string();
                self.entries.len() - 1
            }
            Some(0) => {
                // Already at oldest, can't go further
                return None;
            }
            Some(pos) => pos - 1,
        };

        self.position = Some(new_pos);
        Some(&self.entries[new_pos])
    }

    /// Navigate to next (newer) entry.
    ///
    /// Returns the entry to display, or the saved draft if returning to new input.
    /// Returns None if not currently navigating history.
    pub fn next_entry(&mut self) -> Option<&str> {
        let pos = self.position?;

        if pos + 1 >= self.entries.len() {
            // Return to draft (new input mode)
            self.position = None;
            Some(&self.draft)
        } else {
            self.position = Some(pos + 1);
            Some(&self.entries[pos + 1])
        }
    }

    /// Reset navigation state.
    ///
    /// Called when input is modified (typing), which exits history navigation.
    pub fn reset_navigation(&mut self) {
        self.position = None;
        self.draft.clear();
    }

    /// Check if currently navigating history.
    pub fn is_navigating(&self) -> bool {
        self.position.is_some()
    }
}
