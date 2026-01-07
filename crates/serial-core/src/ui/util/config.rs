//! Declarative configuration panel system.
//!
//! This module provides a framework-agnostic way to define configuration panels.
//! Frontends can use these definitions to render config UIs without duplicating
//! the logic for field types, validation, and navigation.
//!
//! # Architecture
//!
//! - [`FieldDef`]: Defines a single config field (type, get/set, visibility, validation)
//! - [`Section`]: Groups related fields with an optional header
//! - [`ConfigNav`]: Navigation state for a config panel (selection, edit mode)
//!
//! # Example
//!
//! ```ignore
//! use serial_core::ui::config::{FieldDef, FieldKind, FieldValue, Section, ConfigNav};
//!
//! struct MySettings {
//!     show_timestamps: bool,
//!     encoding_index: usize,
//! }
//!
//! static SECTIONS: &[Section<MySettings>] = &[
//!     Section {
//!         header: None,
//!         fields: &[
//!             FieldDef {
//!                 id: "timestamps",
//!                 label: "Timestamps",
//!                 kind: FieldKind::Toggle,
//!                 get: |s| FieldValue::Bool(s.show_timestamps),
//!                 set: |s, v| { if let FieldValue::Bool(b) = v { s.show_timestamps = b; } },
//!                 ..FieldDef::DEFAULT
//!             },
//!         ],
//!     },
//! ];
//!
//! let mut nav = ConfigNav::new();
//! nav.next_field(SECTIONS, &settings);
//! ```

use std::borrow::Cow;

use super::text::TextBuffer;

// =============================================================================
// Field Values
// =============================================================================

/// A value that can be stored in a config field.
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
    /// Create a string value from a static str.
    pub fn str(s: &'static str) -> Self {
        FieldValue::String(Cow::Borrowed(s))
    }

    /// Create a string value from an owned String.
    pub fn string(s: String) -> Self {
        FieldValue::String(Cow::Owned(s))
    }

    /// Try to get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FieldValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    /// Try to get as usize.
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            FieldValue::Usize(n) => Some(*n),
            FieldValue::OptionIndex(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get as isize.
    pub fn as_isize(&self) -> Option<isize> {
        match self {
            FieldValue::Isize(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get as f64.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            FieldValue::Float(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get as option index.
    pub fn as_option_index(&self) -> Option<usize> {
        match self {
            FieldValue::OptionIndex(n) => Some(*n),
            _ => None,
        }
    }
}

// =============================================================================
// Field Kinds
// =============================================================================

/// What kind of UI control a field needs.
#[derive(Debug, Clone)]
pub enum FieldKind {
    /// Boolean toggle (checkbox, switch).
    Toggle,

    /// Select from predefined options (dropdown, radio buttons).
    Select { options: &'static [&'static str] },

    /// Free-form text input.
    TextInput {
        /// Placeholder text shown when empty.
        placeholder: &'static str,
    },

    /// Numeric input (integer).
    NumericInput {
        /// Minimum allowed value (inclusive).
        min: Option<i64>,
        /// Maximum allowed value (inclusive).
        max: Option<i64>,
    },
}

impl FieldKind {
    /// Check if this is a Select field.
    pub fn is_select(&self) -> bool {
        matches!(self, FieldKind::Select { .. })
    }

    /// Check if this is a TextInput field.
    pub fn is_text_input(&self) -> bool {
        matches!(self, FieldKind::TextInput { .. })
    }

    /// Check if this is a NumericInput field.
    pub fn is_numeric_input(&self) -> bool {
        matches!(self, FieldKind::NumericInput { .. })
    }

    /// Check if this field requires text editing (text or numeric input).
    pub fn is_editable(&self) -> bool {
        matches!(
            self,
            FieldKind::TextInput { .. } | FieldKind::NumericInput { .. }
        )
    }

    /// Get the number of options for a Select field.
    pub fn option_count(&self) -> usize {
        match self {
            FieldKind::Select { options } => options.len(),
            _ => 0,
        }
    }

    /// Get options for a Select field.
    pub fn options(&self) -> &'static [&'static str] {
        match self {
            FieldKind::Select { options } => options,
            _ => &[],
        }
    }
}

// =============================================================================
// Field Definition
// =============================================================================

/// Result of validating a field value.
pub type ValidationResult = Result<(), Cow<'static, str>>;

/// Always-true visibility function.
pub fn always_visible<T>(_: &T) -> bool {
    true
}

/// Always-true enabled function.
pub fn always_enabled<T>(_: &T) -> bool {
    true
}

/// Always-valid validation function.
pub fn always_valid(_: &FieldValue) -> ValidationResult {
    Ok(())
}

/// Definition of a single config field.
///
/// Each field defines:
/// - How to display it (label, kind)
/// - How to get/set its value from state
/// - When it should be visible and/or enabled
/// - How to validate input
pub struct FieldDef<T> {
    /// Unique identifier for this field.
    pub id: &'static str,

    /// Display label shown to user.
    pub label: &'static str,

    /// What kind of control to render.
    pub kind: FieldKind,

    /// Get current value from state.
    pub get: fn(&T) -> FieldValue,

    /// Set value on state.
    pub set: fn(&mut T, FieldValue),

    /// Check if field should be visible given current state.
    pub visible: fn(&T) -> bool,

    /// Check if field should be enabled (interactive) given current state.
    pub enabled: fn(&T) -> bool,

    /// Parent field id for tree-style indentation.
    pub parent_id: Option<&'static str>,

    /// Validate a value before setting.
    pub validate: fn(&FieldValue) -> ValidationResult,
}

impl<T> FieldDef<T> {
    /// Default values for optional fields.
    ///
    /// Use with struct update syntax:
    /// ```ignore
    /// FieldDef {
    ///     id: "foo",
    ///     label: "Foo",
    ///     kind: FieldKind::Toggle,
    ///     get: |s| FieldValue::Bool(s.foo),
    ///     set: |s, v| { ... },
    ///     ..FieldDef::DEFAULT
    /// }
    /// ```
    pub const DEFAULT: FieldDef<T> = FieldDef {
        id: "",
        label: "",
        kind: FieldKind::Toggle,
        get: |_| FieldValue::Bool(false),
        set: |_, _| {},
        visible: always_visible,
        enabled: always_enabled,
        parent_id: None,
        validate: always_valid,
    };

    /// Check if this field is currently visible.
    pub fn is_visible(&self, state: &T) -> bool {
        (self.visible)(state)
    }

    /// Check if this field is currently enabled.
    pub fn is_enabled(&self, state: &T) -> bool {
        (self.enabled)(state)
    }

    /// Check if this is a sub-option (has a parent).
    pub fn is_sub_option(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Get the current value from state.
    pub fn get_value(&self, state: &T) -> FieldValue {
        (self.get)(state)
    }

    /// Validate and set a value on state.
    pub fn set_value(&self, state: &mut T, value: FieldValue) -> ValidationResult {
        (self.validate)(&value)?;
        (self.set)(state, value);
        Ok(())
    }

    /// Set value without validation.
    pub fn set_value_unchecked(&self, state: &mut T, value: FieldValue) {
        (self.set)(state, value);
    }

    /// Get display string for current value.
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

impl<T> std::fmt::Debug for FieldDef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldDef")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// Section
// =============================================================================

/// A named section of fields.
#[derive(Debug)]
pub struct Section<T: 'static> {
    /// Section header (None for first/main section that needs no header).
    pub header: Option<&'static str>,

    /// Fields in this section.
    pub fields: &'static [FieldDef<T>],
}

impl<T: 'static> Section<T> {
    /// Get visible fields in this section.
    pub fn visible_fields<'a>(&'a self, state: &T) -> impl Iterator<Item = &'a FieldDef<T>> {
        self.fields.iter().filter(|f| f.is_visible(state))
    }

    /// Count visible fields.
    pub fn visible_field_count(&self, state: &T) -> usize {
        self.fields.iter().filter(|f| f.is_visible(state)).count()
    }
}

// =============================================================================
// Section Slice Extension
// =============================================================================

/// Helper functions for working with a slice of sections.
pub trait SectionSliceExt<T: 'static> {
    /// Get total count of visible fields across all sections.
    fn total_visible_fields(&self, state: &T) -> usize;

    /// Find a field by id.
    fn find_field(&self, id: &str) -> Option<&FieldDef<T>>;

    /// Get the nth visible field (flattened across sections).
    fn nth_visible_field<'a>(&'a self, state: &T, n: usize) -> Option<&'a FieldDef<T>>;

    /// Iterate all visible fields with their section index.
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

// =============================================================================
// Edit Mode
// =============================================================================

/// Current editing mode within a config panel.
#[derive(Debug, Clone, Default)]
pub enum EditMode {
    /// Not editing - normal navigation.
    #[default]
    None,

    /// Dropdown is open for a Select field.
    Dropdown {
        /// Currently highlighted option index.
        index: usize,
    },

    /// Editing a text/numeric field.
    TextInput {
        /// The text being edited.
        buffer: TextBuffer,
    },
}

impl EditMode {
    /// Check if in dropdown mode.
    pub fn is_dropdown(&self) -> bool {
        matches!(self, EditMode::Dropdown { .. })
    }

    /// Check if in text input mode.
    pub fn is_text_input(&self) -> bool {
        matches!(self, EditMode::TextInput { .. })
    }

    /// Check if in any editing mode.
    pub fn is_editing(&self) -> bool {
        !matches!(self, EditMode::None)
    }

    /// Get dropdown index if in dropdown mode.
    pub fn dropdown_index(&self) -> Option<usize> {
        match self {
            EditMode::Dropdown { index } => Some(*index),
            _ => None,
        }
    }

    /// Get text buffer if in text input mode.
    pub fn text_buffer(&self) -> Option<&TextBuffer> {
        match self {
            EditMode::TextInput { buffer } => Some(buffer),
            _ => None,
        }
    }

    /// Get mutable text buffer if in text input mode.
    pub fn text_buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        match self {
            EditMode::TextInput { buffer } => Some(buffer),
            _ => None,
        }
    }
}

// =============================================================================
// Config Navigation
// =============================================================================

/// Navigation state for a config panel.
///
/// Tracks which field is selected and the current editing mode.
#[derive(Debug, Clone, Default)]
pub struct ConfigNav {
    /// Index of currently selected field (in flattened visible field list).
    pub selected: usize,

    /// Scroll offset (for panels that need scrolling).
    pub scroll: usize,

    /// Current editing mode.
    pub edit_mode: EditMode,
}

impl ConfigNav {
    /// Create new navigation state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Move to next visible and enabled field.
    pub fn next_field<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total == 0 {
            return;
        }

        let start = self.selected;
        loop {
            self.selected = (self.selected + 1) % total;
            if let Some(field) = sections.nth_visible_field(state, self.selected)
                && field.is_enabled(state)
            {
                break;
            }
            if self.selected == start {
                break;
            }
        }
    }

    /// Move to previous visible and enabled field.
    pub fn prev_field<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total == 0 {
            return;
        }

        let start = self.selected;
        loop {
            self.selected = if self.selected == 0 {
                total - 1
            } else {
                self.selected - 1
            };
            if let Some(field) = sections.nth_visible_field(state, self.selected)
                && field.is_enabled(state)
            {
                break;
            }
            if self.selected == start {
                break;
            }
        }
    }

    /// Get the currently selected field.
    pub fn current_field<'a, T: 'static>(
        &self,
        sections: &'a [Section<T>],
        state: &T,
    ) -> Option<&'a FieldDef<T>> {
        sections.nth_visible_field(state, self.selected)
    }

    /// Get the current field's id.
    pub fn current_field_id<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &T,
    ) -> Option<&'static str> {
        self.current_field(sections, state).map(|f| f.id)
    }

    /// Ensure selected index is valid after fields may have changed.
    pub fn clamp<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let total = sections.total_visible_fields(state);
        if total == 0 {
            self.selected = 0;
            return;
        }

        if self.selected >= total {
            self.selected = total - 1;
        }

        // If current field is disabled, find an enabled one
        if let Some(field) = sections.nth_visible_field(state, self.selected)
            && !field.is_enabled(state)
        {
            for i in 0..total {
                if let Some(f) = sections.nth_visible_field(state, i)
                    && f.is_enabled(state)
                {
                    self.selected = i;
                    return;
                }
            }
        }
    }

    // =========================================================================
    // Dropdown operations
    // =========================================================================

    /// Open dropdown for the current Select field.
    pub fn open_dropdown<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        if let Some(field) = self.current_field(sections, state)
            && field.kind.is_select()
            && let FieldValue::OptionIndex(idx) = field.get_value(state)
        {
            self.edit_mode = EditMode::Dropdown { index: idx };
        }
    }

    /// Close the dropdown without applying.
    pub fn close_dropdown(&mut self) {
        self.edit_mode = EditMode::None;
    }

    /// Move dropdown selection down.
    pub fn dropdown_next<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let count = self
            .current_field(sections, state)
            .map(|f| f.kind.option_count())
            .unwrap_or(0);

        if let EditMode::Dropdown { index } = &mut self.edit_mode
            && count > 0
        {
            *index = (*index + 1) % count;
        }
    }

    /// Move dropdown selection up.
    pub fn dropdown_prev<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        let count = self
            .current_field(sections, state)
            .map(|f| f.kind.option_count())
            .unwrap_or(0);

        if let EditMode::Dropdown { index } = &mut self.edit_mode
            && count > 0
        {
            *index = if *index == 0 { count - 1 } else { *index - 1 };
        }
    }

    /// Apply dropdown selection and close.
    pub fn apply_dropdown<T: 'static>(
        &mut self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let EditMode::Dropdown { index } = self.edit_mode
            && let Some(field) = sections.nth_visible_field(state, self.selected)
        {
            field.set_value(state, FieldValue::OptionIndex(index))?;
        }
        self.edit_mode = EditMode::None;
        Ok(())
    }

    // =========================================================================
    // Text input operations
    // =========================================================================

    /// Start text editing for the current field.
    pub fn start_text_edit<T: 'static>(&mut self, sections: &[Section<T>], state: &T) {
        if let Some(field) = self.current_field(sections, state) {
            let initial = match field.get_value(state) {
                FieldValue::String(s) => s.into_owned(),
                FieldValue::Usize(n) => n.to_string(),
                FieldValue::Isize(n) => n.to_string(),
                FieldValue::Float(n) => n.to_string(),
                _ => return,
            };
            self.edit_mode = EditMode::TextInput {
                buffer: TextBuffer::with_content(initial),
            };
        }
    }

    /// Cancel text editing.
    pub fn cancel_text_edit(&mut self) {
        self.edit_mode = EditMode::None;
    }

    /// Apply text edit and close.
    pub fn apply_text_edit<T: 'static>(
        &mut self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        let EditMode::TextInput { buffer } = &self.edit_mode else {
            return Ok(());
        };

        let Some(field) = sections.nth_visible_field(state, self.selected) else {
            self.edit_mode = EditMode::None;
            return Ok(());
        };

        // Determine the appropriate FieldValue based on field kind
        let value = if field.kind.is_numeric_input() {
            let text = buffer.content();
            if let Ok(n) = text.parse::<usize>() {
                FieldValue::Usize(n)
            } else if let Ok(n) = text.parse::<isize>() {
                FieldValue::Isize(n)
            } else if let Ok(n) = text.parse::<f64>() {
                FieldValue::Float(n)
            } else {
                return Err(Cow::Borrowed("Invalid number"));
            }
        } else {
            FieldValue::string(buffer.content().to_string())
        };

        // Validate before applying
        let result = (field.validate)(&value);
        if result.is_ok() {
            field.set_value_unchecked(state, value);
            self.edit_mode = EditMode::None;
        }
        result
    }

    // =========================================================================
    // Toggle operations
    // =========================================================================

    /// Toggle a boolean field.
    pub fn toggle_current<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let Some(field) = sections.nth_visible_field(state, self.selected)
            && matches!(field.kind, FieldKind::Toggle)
            && let FieldValue::Bool(current) = field.get_value(state)
        {
            return field.set_value(state, FieldValue::Bool(!current));
        }
        Ok(())
    }

    /// Cycle a Select field without opening dropdown (for h/l navigation).
    pub fn cycle_select_next<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let Some(field) = sections.nth_visible_field(state, self.selected)
            && field.kind.is_select()
            && let FieldValue::OptionIndex(current) = field.get_value(state)
        {
            let count = field.kind.option_count();
            if count > 0 {
                let next = (current + 1) % count;
                return field.set_value(state, FieldValue::OptionIndex(next));
            }
        }
        Ok(())
    }

    /// Cycle a Select field backward without opening dropdown.
    pub fn cycle_select_prev<T: 'static>(
        &self,
        sections: &[Section<T>],
        state: &mut T,
    ) -> ValidationResult {
        if let Some(field) = sections.nth_visible_field(state, self.selected)
            && field.kind.is_select()
            && let FieldValue::OptionIndex(current) = field.get_value(state)
        {
            let count = field.kind.option_count();
            if count > 0 {
                let prev = if current == 0 { count - 1 } else { current - 1 };
                return field.set_value(state, FieldValue::OptionIndex(prev));
            }
        }
        Ok(())
    }
}

// =============================================================================
// Key handling result
// =============================================================================

/// Result of handling a config panel key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigKeyResult {
    /// Key was not handled.
    NotHandled,
    /// Key was handled, no state change.
    Handled,
    /// Key was handled and config value changed.
    Changed,
    /// Dropdown/edit was closed (may need clear for overlay removal).
    EditClosed,
    /// Validation failed (includes error message).
    ValidationFailed(Cow<'static, str>),
}

impl ConfigKeyResult {
    /// Returns true if the config state changed.
    pub fn is_changed(&self) -> bool {
        *self == ConfigKeyResult::Changed
    }

    /// Returns true if the key was handled.
    pub fn is_handled(&self) -> bool {
        *self != ConfigKeyResult::NotHandled
    }
}
