//! Declarative configuration field system
//!
//! This module provides a framework-agnostic way to define configuration panels.
//! Frontends can use these definitions to render config UIs without duplicating
//! the logic for field types, validation, and navigation.
//!
//! # Example
//!
//! ```ignore
//! use serial_core::ui::config::{FieldDef, FieldKind, FieldValue, Section};
//!
//! struct MySettings {
//!     show_timestamps: bool,
//!     max_lines: usize,
//!     encoding: Encoding,
//! }
//!
//! const MY_CONFIG: &[Section<MySettings>] = &[
//!     Section {
//!         header: None,
//!         fields: &[
//!             FieldDef {
//!                 id: "timestamps",
//!                 label: "Show Timestamps",
//!                 kind: FieldKind::Toggle,
//!                 get: |s| FieldValue::Bool(s.show_timestamps),
//!                 set: |s, v| { if let FieldValue::Bool(b) = v { s.show_timestamps = b; } },
//!                 visible: |_| true,
//!                 validate: |_| Ok(()),
//!             },
//!         ],
//!     },
//! ];
//! ```

use std::borrow::Cow;

/// A value that can be stored in a config field
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Boolean value (for toggles)
    Bool(bool),
    /// String value (for text inputs)
    String(Cow<'static, str>),
    /// Unsigned integer value
    Usize(usize),
    /// Signed integer value
    Isize(isize),
    /// Floating point value
    Float(f64),
    /// Index into a set of options (for dropdowns/selects)
    OptionIndex(usize),
}

impl FieldValue {
    /// Create a string value from a static str
    pub fn str(s: &'static str) -> Self {
        FieldValue::String(Cow::Borrowed(s))
    }

    /// Create a string value from an owned String
    pub fn string(s: String) -> Self {
        FieldValue::String(Cow::Owned(s))
    }

    /// Try to get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FieldValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    /// Try to get as usize
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            FieldValue::Usize(n) => Some(*n),
            FieldValue::OptionIndex(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get as option index
    pub fn as_option_index(&self) -> Option<usize> {
        match self {
            FieldValue::OptionIndex(n) => Some(*n),
            _ => None,
        }
    }
}

/// What kind of UI control a field needs
#[derive(Debug, Clone)]
pub enum FieldKind {
    /// Boolean toggle (checkbox, switch)
    Toggle,

    /// Select from predefined options (dropdown, radio buttons)
    /// The options are static strings for display
    Select { options: &'static [&'static str] },

    /// Free-form text input
    TextInput {
        /// Placeholder text shown when empty
        placeholder: &'static str,
    },

    /// Numeric input (integer)
    NumericInput {
        /// Minimum allowed value (inclusive)
        min: Option<i64>,
        /// Maximum allowed value (inclusive)
        max: Option<i64>,
    },
}

impl FieldKind {
    /// Check if this field kind uses a dropdown/select UI
    pub fn is_select(&self) -> bool {
        matches!(self, FieldKind::Select { .. })
    }

    /// Check if this field kind uses text input
    pub fn is_text_input(&self) -> bool {
        matches!(self, FieldKind::TextInput { .. })
    }

    /// Get the number of options for a Select field
    pub fn option_count(&self) -> usize {
        match self {
            FieldKind::Select { options } => options.len(),
            _ => 0,
        }
    }

    /// Get options for a Select field
    pub fn options(&self) -> &'static [&'static str] {
        match self {
            FieldKind::Select { options } => options,
            _ => &[],
        }
    }
}

/// Result of validating a field value
pub type ValidationResult = Result<(), Cow<'static, str>>;

/// Definition of a single config field
///
/// This is the core building block. Each field defines:
/// - How to display it (label, kind)
/// - How to get/set its value from state
/// - When it should be visible
/// - How to validate input
pub struct FieldDef<T> {
    /// Unique identifier for this field (used for focus tracking, etc.)
    pub id: &'static str,

    /// Display label shown to user
    pub label: &'static str,

    /// What kind of control to render
    pub kind: FieldKind,

    /// Get current value from state
    pub get: fn(&T) -> FieldValue,

    /// Set value on state
    pub set: fn(&mut T, FieldValue),

    /// Check if field should be visible given current state
    /// Return false to hide the field (e.g., "Custom Delimiter" only shown when mode is Custom)
    pub visible: fn(&T) -> bool,

    /// Validate a value before setting
    /// Return Err with message if invalid
    pub validate: fn(&FieldValue) -> ValidationResult,
}

impl<T> std::fmt::Debug for FieldDef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldDef")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl<T> FieldDef<T> {
    /// Check if this field is currently visible
    pub fn is_visible(&self, state: &T) -> bool {
        (self.visible)(state)
    }

    /// Get the current value from state
    pub fn get_value(&self, state: &T) -> FieldValue {
        (self.get)(state)
    }

    /// Validate and set a value on state
    ///
    /// Returns Ok(()) if set successfully, Err with message if validation failed
    pub fn set_value(&self, state: &mut T, value: FieldValue) -> ValidationResult {
        (self.validate)(&value)?;
        (self.set)(state, value);
        Ok(())
    }

    /// Set value without validation (use when you know the value is valid)
    pub fn set_value_unchecked(&self, state: &mut T, value: FieldValue) {
        (self.set)(state, value);
    }

    /// Get display string for current value
    pub fn display_value(&self, state: &T) -> Cow<'static, str> {
        let value = self.get_value(state);
        match (&self.kind, &value) {
            (FieldKind::Toggle, FieldValue::Bool(b)) => {
                Cow::Borrowed(if *b { "ON" } else { "OFF" })
            }
            (FieldKind::Select { options }, FieldValue::OptionIndex(idx)) => {
                Cow::Borrowed(options.get(*idx).copied().unwrap_or("???"))
            }
            (FieldKind::TextInput { placeholder }, FieldValue::String(s)) => {
                if s.is_empty() {
                    Cow::Borrowed(*placeholder)
                } else {
                    s.clone()
                }
            }
            (_, FieldValue::String(s)) => s.clone(),
            (_, FieldValue::Usize(n)) => Cow::Owned(n.to_string()),
            (_, FieldValue::Isize(n)) => Cow::Owned(n.to_string()),
            (_, FieldValue::Float(n)) => Cow::Owned(format!("{:.2}", n)),
            (_, FieldValue::Bool(b)) => Cow::Borrowed(if *b { "true" } else { "false" }),
            (_, FieldValue::OptionIndex(n)) => Cow::Owned(n.to_string()),
        }
    }
}

/// A named section of fields
///
/// Sections allow grouping related fields together with an optional header.
#[derive(Debug)]
pub struct Section<T: 'static> {
    /// Section header (None for first/main section that needs no header)
    pub header: Option<&'static str>,

    /// Fields in this section
    pub fields: &'static [FieldDef<T>],
}

impl<T: 'static> Section<T> {
    /// Get visible fields in this section
    pub fn visible_fields<'a>(&'a self, state: &T) -> impl Iterator<Item = &'a FieldDef<T>> {
        self.fields.iter().filter(|f| f.is_visible(state))
    }

    /// Count visible fields
    pub fn visible_field_count(&self, state: &T) -> usize {
        self.fields.iter().filter(|f| f.is_visible(state)).count()
    }
}

/// Helper functions for working with a slice of sections
pub trait SectionSliceExt<T: 'static> {
    /// Get total count of visible fields across all sections
    fn total_visible_fields(&self, state: &T) -> usize;

    /// Find a field by id
    fn find_field(&self, id: &str) -> Option<&FieldDef<T>>;

    /// Get the nth visible field (flattened across sections)
    fn nth_visible_field<'a>(&'a self, state: &T, n: usize) -> Option<&'a FieldDef<T>>;

    /// Iterate all visible fields with their section index
    fn visible_fields_with_section<'a>(
        &'a self,
        state: &T,
    ) -> impl Iterator<Item = (usize, &'a FieldDef<T>)>;
}

impl<T: 'static> SectionSliceExt<T> for [Section<T>] {
    fn total_visible_fields(&self, state: &T) -> usize {
        self.iter().map(|s| s.visible_field_count(state)).sum()
    }

    fn find_field(&self, id: &str) -> Option<&FieldDef<T>> {
        self.iter()
            .flat_map(|s| s.fields.iter())
            .find(|f| f.id == id)
    }

    fn nth_visible_field<'a>(&'a self, state: &T, n: usize) -> Option<&'a FieldDef<T>> {
        self.iter()
            .flat_map(|s| s.visible_fields(state))
            .nth(n)
    }

    fn visible_fields_with_section<'a>(
        &'a self,
        state: &T,
    ) -> impl Iterator<Item = (usize, &'a FieldDef<T>)> {
        self.iter()
            .enumerate()
            .flat_map(|(si, s)| s.visible_fields(state).map(move |f| (si, f)))
    }
}

/// Always-true visibility function (for fields that are always shown)
pub fn always_visible<T>(_: &T) -> bool {
    true
}

/// Always-valid validation function (for fields with no validation)
pub fn always_valid(_: &FieldValue) -> ValidationResult {
    Ok(())
}

// =============================================================================
// Navigation State
// =============================================================================

/// Navigation state for a config panel
///
/// This tracks which field is selected and manages navigation through
/// the field list. It works with any `Section<T>` slice.
#[derive(Debug, Clone, Default)]
pub struct ConfigPanelNav {
    /// Index of currently selected field (in flattened visible field list)
    pub selected: usize,

    /// Index within dropdown options (when a Select field is being edited)
    pub dropdown_index: usize,

    /// Scroll offset (for panels that need scrolling)
    pub scroll_offset: usize,
}

impl ConfigPanelNav {
    /// Create new navigation state
    pub fn new() -> Self {
        Self::default()
    }

    /// Move to next visible field
    pub fn next_field<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total > 0 {
            self.selected = (self.selected + 1) % total;
        }
    }

    /// Move to previous visible field
    pub fn prev_field<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total > 0 {
            self.selected = if self.selected == 0 {
                total - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Get the currently selected field
    pub fn current_field<'a, T: 'static>(
        &self,
        sections: &'a [Section<T>],
        state: &T,
    ) -> Option<&'a FieldDef<T>> {
        sections.nth_visible_field(state, self.selected)
    }

    /// Get the current field's id
    pub fn current_field_id<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &T,
    ) -> Option<&'static str> {
        self.current_field(sections, state).map(|f| f.id)
    }

    /// Sync dropdown_index with the current field's value
    ///
    /// Call this when entering edit mode on a Select field
    pub fn sync_dropdown_index<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        if let Some(field) = self.current_field(sections, state) {
            if let FieldValue::OptionIndex(idx) = field.get_value(state) {
                self.dropdown_index = idx;
            }
        }
    }

    /// Move dropdown selection down
    pub fn dropdown_next<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        if let Some(field) = self.current_field(sections, state) {
            let count = field.kind.option_count();
            if count > 0 {
                self.dropdown_index = (self.dropdown_index + 1) % count;
            }
        }
    }

    /// Move dropdown selection up
    pub fn dropdown_prev<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        if let Some(field) = self.current_field(sections, state) {
            let count = field.kind.option_count();
            if count > 0 {
                self.dropdown_index = if self.dropdown_index == 0 {
                    count - 1
                } else {
                    self.dropdown_index - 1
                };
            }
        }
    }

    /// Apply the current dropdown selection to the state
    ///
    /// Returns Ok(()) if successful, Err with validation message if failed
    pub fn apply_dropdown_selection<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let Some(field) = sections.nth_visible_field(state, self.selected) {
            field.set_value(state, FieldValue::OptionIndex(self.dropdown_index))
        } else {
            Ok(())
        }
    }

    /// Toggle a boolean field
    ///
    /// Returns Ok(()) if toggled, Err if not a toggle field or validation failed
    pub fn toggle_current<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let Some(field) = sections.nth_visible_field(state, self.selected) {
            if matches!(field.kind, FieldKind::Toggle) {
                if let FieldValue::Bool(current) = field.get_value(state) {
                    return field.set_value(state, FieldValue::Bool(!current));
                }
            }
        }
        Ok(())
    }

    /// Ensure selected index is valid after fields may have changed visibility
    pub fn clamp_selection<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total == 0 {
            self.selected = 0;
        } else if self.selected >= total {
            self.selected = total - 1;
        }
    }
}
