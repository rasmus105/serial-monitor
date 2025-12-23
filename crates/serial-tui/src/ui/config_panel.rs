//! Generic config panel widget for rendering any type that implements `Configure`.
//!
//! This module provides a generic, reusable config panel that can render any type
//! implementing the `Configure` trait from the `config` crate. It eliminates the
//! boilerplate of manually implementing config panels for each config type.
//!
//! # Example
//!
//! ```ignore
//! use config::Configure;
//! use serial_tui::ui::config_panel::{ConfigPanelState, ConfigPanelWidget};
//!
//! #[derive(Debug, Clone, Default, Configure)]
//! struct MyConfig {
//!     #[config(label = "Name")]
//!     name: String,
//!     #[config(label = "Count", min = 0, max = 100)]
//!     count: u32,
//! }
//!
//! // In your render function:
//! let mut state = ConfigPanelState::new();
//! let widget = ConfigPanelWidget::new(&config, &mut state)
//!     .title("My Config")
//!     .focused(true);
//! frame.render_widget(widget, area);
//! ```

use config::{ConfigSchema, ConfigValue, ConfigValues, Configure, FieldType};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

// =============================================================================
// Config Panel State
// =============================================================================

/// State for a generic config panel
#[derive(Debug, Clone)]
pub struct ConfigPanelState {
    /// Whether the config panel is visible
    pub visible: bool,
    /// Index of the currently selected field
    pub selected_field: usize,
    /// Dropdown selection index (when dropdown is open)
    pub dropdown_index: usize,
    /// Scroll offset for the config panel list
    pub scroll_offset: usize,
    /// Whether we're in dropdown mode
    pub dropdown_open: bool,
    /// Whether we're in text input mode
    pub text_input_open: bool,
    /// Text input buffer
    pub text_buffer: String,
    /// Total number of fields (cached from schema)
    field_count: usize,
    /// Visual line positions for each field (for scroll calculation)
    field_line_positions: Vec<usize>,
}

impl Default for ConfigPanelState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigPanelState {
    /// Create a new config panel state
    pub fn new() -> Self {
        Self {
            visible: false,
            selected_field: 0,
            dropdown_index: 0,
            scroll_offset: 0,
            dropdown_open: false,
            text_input_open: false,
            text_buffer: String::new(),
            field_count: 0,
            field_line_positions: Vec::new(),
        }
    }

    /// Create with initial visibility
    pub fn with_visible(visible: bool) -> Self {
        Self {
            visible,
            ..Self::new()
        }
    }

    /// Initialize the state with a schema (call once when the config type is known)
    pub fn init_with_schema(&mut self, schema: &ConfigSchema) {
        self.field_count = schema.fields.len();
        self.selected_field = self.selected_field.min(self.field_count.saturating_sub(1));
    }

    /// Move to next field
    pub fn next_field(&mut self) {
        if self.field_count > 0 {
            self.selected_field = (self.selected_field + 1) % self.field_count;
        }
    }

    /// Move to previous field
    pub fn prev_field(&mut self) {
        if self.field_count > 0 {
            self.selected_field = (self.selected_field + self.field_count - 1) % self.field_count;
        }
    }

    /// Open dropdown with given current index
    pub fn open_dropdown(&mut self, current_index: usize) {
        self.dropdown_index = current_index;
        self.dropdown_open = true;
    }

    /// Close dropdown
    pub fn close_dropdown(&mut self) {
        self.dropdown_open = false;
    }

    /// Open text input with initial value
    pub fn open_text_input(&mut self, initial_value: &str) {
        self.text_buffer = initial_value.to_string();
        self.text_input_open = true;
    }

    /// Close text input
    pub fn close_text_input(&mut self) {
        self.text_input_open = false;
        self.text_buffer.clear();
    }

    /// Get text input value
    pub fn text_value(&self) -> &str {
        &self.text_buffer
    }

    /// Push character to text buffer
    pub fn text_push(&mut self, c: char) {
        self.text_buffer.push(c);
    }

    /// Pop character from text buffer
    pub fn text_pop(&mut self) {
        self.text_buffer.pop();
    }

    /// Move dropdown selection down
    pub fn dropdown_next(&mut self, options_count: usize) {
        if options_count > 0 {
            self.dropdown_index = (self.dropdown_index + 1) % options_count;
        }
    }

    /// Move dropdown selection up
    pub fn dropdown_prev(&mut self, options_count: usize) {
        if options_count > 0 {
            self.dropdown_index = (self.dropdown_index + options_count - 1) % options_count;
        }
    }

    /// Update field line positions (called during render)
    pub fn set_field_line_positions(&mut self, positions: Vec<usize>) {
        self.field_line_positions = positions;
    }

    /// Adjust scroll to keep selected field visible
    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if let Some(&line_idx) = self.field_line_positions.get(self.selected_field) {
            if line_idx < self.scroll_offset {
                self.scroll_offset = line_idx;
            } else if line_idx >= self.scroll_offset + visible_height {
                self.scroll_offset = line_idx.saturating_sub(visible_height - 1);
            }
        }
    }
}

// =============================================================================
// Field Helpers
// =============================================================================

/// Check if a field type is a toggle (boolean)
pub fn is_field_toggle(field_type: &FieldType) -> bool {
    matches!(field_type, FieldType::Bool)
}

/// Check if a field type is a dropdown (enum)
pub fn is_field_dropdown(field_type: &FieldType) -> bool {
    matches!(field_type, FieldType::Enum { .. })
}

/// Check if a field type is a text input (string, numeric, path)
pub fn is_field_text_input(field_type: &FieldType) -> bool {
    matches!(
        field_type,
        FieldType::String
            | FieldType::Char
            | FieldType::Int { .. }
            | FieldType::UInt { .. }
            | FieldType::Float { .. }
    )
}

/// Get dropdown options for an enum field type
pub fn get_dropdown_options(field_type: &FieldType) -> Vec<&'static str> {
    if let FieldType::Enum { variants } = field_type {
        variants.iter().map(|v| v.label).collect()
    } else {
        Vec::new()
    }
}

/// Get the current enum variant index from a value
pub fn get_variant_index(value: &ConfigValue) -> usize {
    if let ConfigValue::Enum { variant_index, .. } = value {
        *variant_index
    } else {
        0
    }
}

/// Get the text value for text input fields
pub fn get_text_value(value: &ConfigValue) -> String {
    match value {
        ConfigValue::String(s) => s.clone(),
        ConfigValue::Char(c) => c.to_string(),
        ConfigValue::Int(n) => n.to_string(),
        ConfigValue::UInt(n) => n.to_string(),
        ConfigValue::Float(n) => n.to_string(),
        ConfigValue::Optional(Some(inner)) => match inner.as_ref() {
            ConfigValue::String(s) => s.clone(),
            ConfigValue::Int(n) => n.to_string(),
            ConfigValue::UInt(n) => n.to_string(),
            ConfigValue::Float(n) => n.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Format a ConfigValue for display
pub fn format_value(value: &ConfigValue, field_type: &FieldType) -> String {
    match (value, field_type) {
        (ConfigValue::Bool(b), _) => if *b { "ON" } else { "OFF" }.to_string(),
        (ConfigValue::String(s), _) => {
            if s.is_empty() {
                "(empty)".to_string()
            } else {
                s.clone()
            }
        }
        (ConfigValue::Char(c), _) => format!("'{}'", c),
        (ConfigValue::Int(n), _) => n.to_string(),
        (ConfigValue::UInt(n), _) => n.to_string(),
        (ConfigValue::Float(n), _) => format!("{:.2}", n),
        (ConfigValue::Enum { variant_index, .. }, FieldType::Enum { variants }) => {
            variants
                .get(*variant_index)
                .map(|v| v.label.to_string())
                .unwrap_or_else(|| format!("Unknown({})", variant_index))
        }
        (ConfigValue::Optional(opt), FieldType::Optional { inner }) => match opt {
            Some(inner_val) => format_value(inner_val, inner),
            None => "(none)".to_string(),
        },
        (ConfigValue::List(items), _) => format!("[{} items]", items.len()),
        (ConfigValue::Struct(_), _) => "(nested)".to_string(),
        _ => "(unknown)".to_string(),
    }
}

// =============================================================================
// Config Panel Widget
// =============================================================================

/// A generic config panel widget that renders any `Configure` type
pub struct ConfigPanelWidget<'a, T: Configure> {
    config: &'a T,
    state: &'a mut ConfigPanelState,
    title: &'a str,
    focused: bool,
}

impl<'a, T: Configure> ConfigPanelWidget<'a, T> {
    /// Create a new config panel widget
    pub fn new(config: &'a T, state: &'a mut ConfigPanelState) -> Self {
        // Initialize state with schema
        state.init_with_schema(T::schema());
        Self {
            config,
            state,
            title: "Config",
            focused: false,
        }
    }

    /// Set the panel title
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// Set whether the panel is focused
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Build the lines to render (returns lines and field positions)
    fn build_lines(&self) -> (Vec<Line<'static>>, Vec<usize>) {
        let schema = T::schema();
        let values = self.config.to_values();
        let is_focused = self.focused || self.state.dropdown_open || self.state.text_input_open;

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut field_positions: Vec<usize> = Vec::new();

        // Add description as header if present
        if let Some(desc) = schema.description {
            lines.push(Line::from(Span::styled(
                desc.to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
        }

        for (field_index, (field_schema, field_value)) in
            schema.fields.iter().zip(values.values.iter()).enumerate()
        {
            // Record the line position for this field
            field_positions.push(lines.len());

            let is_selected = field_index == self.state.selected_field && is_focused;
            let prefix = if is_selected { "> " } else { "  " };

            let label_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let base_value_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };

            // For text input mode, show the buffer with cursor
            let display_value = if self.state.text_input_open
                && is_selected
                && is_field_text_input(&field_schema.field_type)
            {
                format!("{}|", self.state.text_buffer)
            } else {
                format_value(field_value, &field_schema.field_type)
            };

            // Special styling for toggles
            let value_style = if is_field_toggle(&field_schema.field_type) {
                if let ConfigValue::Bool(true) = field_value {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        })
                } else {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        })
                }
            } else {
                base_value_style
            };

            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), label_style),
                Span::styled(format!("{}: ", field_schema.label), label_style),
                Span::styled(display_value, value_style),
            ]));

            // Add description as hint if present
            if let Some(desc) = field_schema.description {
                if is_selected {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
        }

        (lines, field_positions)
    }
}

impl<T: Configure> Widget for ConfigPanelWidget<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.focused || self.state.dropdown_open || self.state.text_input_open;

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Build and render lines
        let (lines, field_positions) = self.build_lines();

        // Store field positions in state for scroll adjustment
        self.state.set_field_line_positions(field_positions);

        // Calculate scroll
        let visible_height = inner.height as usize;
        let total_lines = lines.len();
        let scroll_offset = self.state.scroll_offset.min(total_lines.saturating_sub(visible_height));

        // Take visible lines
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(scroll_offset)
            .take(visible_height)
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);

        // Render scrollbar if needed
        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_lines)
                .position(scroll_offset)
                .viewport_content_length(visible_height);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("|"))
                .thumb_symbol("#")
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray));
            StatefulWidget::render(scrollbar, inner, buf, &mut scrollbar_state);
        }
    }
}

// =============================================================================
// Dropdown Widget
// =============================================================================

/// Render a dropdown popup for enum selection
pub fn render_dropdown(
    buf: &mut Buffer,
    area: Rect,
    options: &[&str],
    selected_index: usize,
    anchor_y: u16,
    anchor_x: u16,
) {
    let dropdown_height = (options.len() + 2).min(10) as u16; // +2 for borders, max 10
    let dropdown_width = options.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6;

    // Position dropdown
    let dropdown_y = anchor_y.min(area.height.saturating_sub(dropdown_height));
    let dropdown_x = anchor_x.min(area.width.saturating_sub(dropdown_width));

    let dropdown_area = Rect::new(
        area.x + dropdown_x,
        area.y + dropdown_y,
        dropdown_width.min(area.width),
        dropdown_height.min(area.height),
    );

    // Clear the dropdown area
    Clear.render(dropdown_area, buf);

    // Build dropdown items
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let is_selected = i == selected_index;
            let prefix = if is_selected { "> " } else { "  " };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(format!("{}{}", prefix, option)).style(style)
        })
        .collect();

    let dropdown_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let dropdown_list = List::new(items).block(dropdown_block);
    Widget::render(dropdown_list, dropdown_area, buf);
}

// =============================================================================
// Value Update Helpers
// =============================================================================

/// Update a single field value in a ConfigValues
pub fn update_field_value(
    values: &mut ConfigValues,
    field_index: usize,
    new_value: ConfigValue,
) {
    if field_index < values.values.len() {
        values.values[field_index] = new_value;
    }
}

/// Toggle a boolean field
pub fn toggle_bool_field(values: &mut ConfigValues, field_index: usize) {
    if let Some(ConfigValue::Bool(b)) = values.values.get_mut(field_index) {
        *b = !*b;
    }
}

/// Set enum variant by index
pub fn set_enum_variant(values: &mut ConfigValues, field_index: usize, variant_index: usize) {
    if let Some(ConfigValue::Enum { variant_index: idx, data }) = values.values.get_mut(field_index)
    {
        *idx = variant_index;
        // Clear data when changing variants (data should be set separately if needed)
        *data = None;
    }
}

/// Parse and set a string field
pub fn set_string_field(values: &mut ConfigValues, field_index: usize, text: &str) {
    if field_index < values.values.len() {
        values.values[field_index] = ConfigValue::String(text.to_string());
    }
}

/// Parse and set an integer field
pub fn set_int_field(values: &mut ConfigValues, field_index: usize, text: &str) -> bool {
    if let Ok(n) = text.parse::<i64>() {
        if field_index < values.values.len() {
            values.values[field_index] = ConfigValue::Int(n);
            return true;
        }
    }
    false
}

/// Parse and set an unsigned integer field
pub fn set_uint_field(values: &mut ConfigValues, field_index: usize, text: &str) -> bool {
    if let Ok(n) = text.parse::<u64>() {
        if field_index < values.values.len() {
            values.values[field_index] = ConfigValue::UInt(n);
            return true;
        }
    }
    false
}

/// Parse and set a float field
pub fn set_float_field(values: &mut ConfigValues, field_index: usize, text: &str) -> bool {
    if let Ok(n) = text.parse::<f64>() {
        if field_index < values.values.len() {
            values.values[field_index] = ConfigValue::Float(n);
            return true;
        }
    }
    false
}

// =============================================================================
// Input Handling Helpers
// =============================================================================

/// Result of handling a config panel key event
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigPanelAction {
    /// No action needed
    None,
    /// Field value was changed, apply to config
    ValueChanged,
    /// Need to open dropdown for field
    OpenDropdown { field_index: usize },
    /// Need to open text input for field
    OpenTextInput { field_index: usize, initial_value: String },
    /// Dropdown selection confirmed
    DropdownConfirmed { field_index: usize, variant_index: usize },
    /// Text input confirmed
    TextInputConfirmed { field_index: usize, value: String },
    /// Cancelled (Esc pressed)
    Cancelled,
    /// Navigation occurred (scroll may need adjustment)
    Navigated,
}

/// Get the action to take when confirm is pressed on a field
pub fn get_confirm_action<T: Configure>(
    config: &T,
    state: &ConfigPanelState,
) -> ConfigPanelAction {
    let schema = T::schema();
    let values = config.to_values();

    if state.selected_field >= schema.fields.len() {
        return ConfigPanelAction::None;
    }

    let field_schema = &schema.fields[state.selected_field];
    let field_value = &values.values[state.selected_field];

    match &field_schema.field_type {
        FieldType::Bool => {
            // Toggle immediately
            ConfigPanelAction::ValueChanged
        }
        FieldType::Enum { .. } => {
            ConfigPanelAction::OpenDropdown {
                field_index: state.selected_field,
            }
        }
        FieldType::String | FieldType::Char | FieldType::Int { .. } | FieldType::UInt { .. } | FieldType::Float { .. } => {
            let initial_value = get_text_value(field_value);
            ConfigPanelAction::OpenTextInput {
                field_index: state.selected_field,
                initial_value,
            }
        }
        FieldType::Optional { inner } => {
            // For optional, toggle the presence or open editor for inner value
            match field_value {
                ConfigValue::Optional(None) => ConfigPanelAction::ValueChanged, // Will set to Some(default)
                ConfigValue::Optional(Some(_)) => {
                    // Open editor for inner value based on inner type
                    match **inner {
                        FieldType::String => ConfigPanelAction::OpenTextInput {
                            field_index: state.selected_field,
                            initial_value: get_text_value(field_value),
                        },
                        FieldType::Enum { .. } => ConfigPanelAction::OpenDropdown {
                            field_index: state.selected_field,
                        },
                        _ => ConfigPanelAction::OpenTextInput {
                            field_index: state.selected_field,
                            initial_value: get_text_value(field_value),
                        },
                    }
                }
                _ => ConfigPanelAction::None,
            }
        }
        _ => ConfigPanelAction::None,
    }
}

/// Input event types that the config panel can handle
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigInput {
    /// Move to next field (j, Down)
    NextField,
    /// Move to previous field (k, Up)
    PrevField,
    /// Confirm current selection (Enter, Space)
    Confirm,
    /// Cancel (Escape)
    Cancel,
    /// Move dropdown down
    DropdownNext,
    /// Move dropdown up
    DropdownPrev,
    /// Push a character to text input
    TextChar(char),
    /// Backspace in text input
    TextBackspace,
}

/// Handle navigation input for a config panel
/// 
/// This function handles common navigation patterns and returns an action
/// that the caller should process.
pub fn handle_config_nav_input<T: Configure>(
    config: &T,
    state: &mut ConfigPanelState,
    input: ConfigInput,
    visible_height: usize,
) -> ConfigPanelAction {
    match input {
        ConfigInput::NextField => {
            if !state.dropdown_open && !state.text_input_open {
                state.next_field();
                state.adjust_scroll(visible_height);
                ConfigPanelAction::Navigated
            } else {
                ConfigPanelAction::None
            }
        }
        ConfigInput::PrevField => {
            if !state.dropdown_open && !state.text_input_open {
                state.prev_field();
                state.adjust_scroll(visible_height);
                ConfigPanelAction::Navigated
            } else {
                ConfigPanelAction::None
            }
        }
        ConfigInput::Confirm => {
            if state.dropdown_open {
                let result = ConfigPanelAction::DropdownConfirmed {
                    field_index: state.selected_field,
                    variant_index: state.dropdown_index,
                };
                state.close_dropdown();
                result
            } else if state.text_input_open {
                let result = ConfigPanelAction::TextInputConfirmed {
                    field_index: state.selected_field,
                    value: state.text_buffer.clone(),
                };
                state.close_text_input();
                result
            } else {
                // Not in a modal - determine action based on field type
                let action = get_confirm_action(config, state);
                match &action {
                    ConfigPanelAction::OpenDropdown { .. } => {
                        let schema = T::schema();
                        let values = config.to_values();
                        if schema.fields.get(state.selected_field).is_some() {
                            let current_idx = get_variant_index(&values.values[state.selected_field]);
                            state.open_dropdown(current_idx);
                        }
                    }
                    ConfigPanelAction::OpenTextInput { initial_value, .. } => {
                        state.open_text_input(initial_value);
                    }
                    _ => {}
                }
                action
            }
        }
        ConfigInput::Cancel => {
            if state.dropdown_open {
                state.close_dropdown();
                ConfigPanelAction::Cancelled
            } else if state.text_input_open {
                state.close_text_input();
                ConfigPanelAction::Cancelled
            } else {
                ConfigPanelAction::Cancelled
            }
        }
        ConfigInput::DropdownNext => {
            if state.dropdown_open {
                let schema = T::schema();
                if let Some(field) = schema.fields.get(state.selected_field) {
                    let options = get_dropdown_options(&field.field_type);
                    state.dropdown_next(options.len());
                }
                ConfigPanelAction::Navigated
            } else {
                ConfigPanelAction::None
            }
        }
        ConfigInput::DropdownPrev => {
            if state.dropdown_open {
                let schema = T::schema();
                if let Some(field) = schema.fields.get(state.selected_field) {
                    let options = get_dropdown_options(&field.field_type);
                    state.dropdown_prev(options.len());
                }
                ConfigPanelAction::Navigated
            } else {
                ConfigPanelAction::None
            }
        }
        ConfigInput::TextChar(c) => {
            if state.text_input_open {
                state.text_push(c);
                ConfigPanelAction::Navigated // Not really navigation but signals redraw
            } else {
                ConfigPanelAction::None
            }
        }
        ConfigInput::TextBackspace => {
            if state.text_input_open {
                state.text_pop();
                ConfigPanelAction::Navigated // Not really navigation but signals redraw
            } else {
                ConfigPanelAction::None
            }
        }
    }
}

/// Apply a confirmed action to values, returning modified values
/// 
/// Call this when you receive a `ConfigPanelAction::DropdownConfirmed` or
/// `ConfigPanelAction::TextInputConfirmed` to update the values.
pub fn apply_action_to_values<T: Configure>(
    config: &T,
    action: &ConfigPanelAction,
) -> Option<config::ConfigValues> {
    let schema = T::schema();
    let mut values = config.to_values();
    
    match action {
        ConfigPanelAction::ValueChanged => {
            // For boolean toggle, we need the state's selected field
            // This case is typically handled by the caller since they have the state
            None
        }
        ConfigPanelAction::DropdownConfirmed { field_index, variant_index } => {
            set_enum_variant(&mut values, *field_index, *variant_index);
            Some(values)
        }
        ConfigPanelAction::TextInputConfirmed { field_index, value } => {
            if let Some(field) = schema.fields.get(*field_index) {
                match &field.field_type {
                    FieldType::String => {
                        set_string_field(&mut values, *field_index, value);
                        Some(values)
                    }
                    FieldType::Int { .. } => {
                        if set_int_field(&mut values, *field_index, value) {
                            Some(values)
                        } else {
                            None
                        }
                    }
                    FieldType::UInt { .. } => {
                        if set_uint_field(&mut values, *field_index, value) {
                            Some(values)
                        } else {
                            None
                        }
                    }
                    FieldType::Float { .. } => {
                        if set_float_field(&mut values, *field_index, value) {
                            Some(values)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Toggle a boolean field and return updated values
pub fn toggle_bool_at<T: Configure>(config: &T, field_index: usize) -> Option<config::ConfigValues> {
    let mut values = config.to_values();
    toggle_bool_field(&mut values, field_index);
    Some(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_panel_state_navigation() {
        let mut state = ConfigPanelState::new();
        state.field_count = 5;
        
        assert_eq!(state.selected_field, 0);
        
        state.next_field();
        assert_eq!(state.selected_field, 1);
        
        state.prev_field();
        assert_eq!(state.selected_field, 0);
        
        // Test wrap around
        state.prev_field();
        assert_eq!(state.selected_field, 4);
        
        state.next_field();
        assert_eq!(state.selected_field, 0);
    }
    
    #[test]
    fn test_dropdown_navigation() {
        let mut state = ConfigPanelState::new();
        
        state.open_dropdown(2);
        assert!(state.dropdown_open);
        assert_eq!(state.dropdown_index, 2);
        
        state.dropdown_next(5);
        assert_eq!(state.dropdown_index, 3);
        
        state.dropdown_prev(5);
        assert_eq!(state.dropdown_index, 2);
        
        state.close_dropdown();
        assert!(!state.dropdown_open);
    }
    
    #[test]
    fn test_text_input() {
        let mut state = ConfigPanelState::new();
        
        state.open_text_input("hello");
        assert!(state.text_input_open);
        assert_eq!(state.text_value(), "hello");
        
        state.text_push('!');
        assert_eq!(state.text_value(), "hello!");
        
        state.text_pop();
        assert_eq!(state.text_value(), "hello");
        
        state.close_text_input();
        assert!(!state.text_input_open);
        assert_eq!(state.text_value(), "");
    }
    
    #[test]
    fn test_format_value() {
        assert_eq!(
            format_value(&ConfigValue::Bool(true), &FieldType::Bool),
            "ON"
        );
        assert_eq!(
            format_value(&ConfigValue::Bool(false), &FieldType::Bool),
            "OFF"
        );
        assert_eq!(
            format_value(&ConfigValue::String("test".to_string()), &FieldType::String),
            "test"
        );
        assert_eq!(
            format_value(&ConfigValue::String("".to_string()), &FieldType::String),
            "(empty)"
        );
        assert_eq!(
            format_value(&ConfigValue::UInt(42), &FieldType::UInt { min: None, max: None }),
            "42"
        );
    }
    
    // =========================================================================
    // Integration test with derive macro
    // =========================================================================
    
    /// A test config struct using the derive macro (no nested enum field for now)
    #[derive(Debug, Clone, Default, config::Configure)]
    #[config(desc = "A test configuration")]
    pub struct TestConfig {
        #[config(label = "Name", desc = "The name of the item")]
        pub name: String,
        
        #[config(label = "Count", desc = "Number of items", min = 0, max = 100)]
        pub count: u32,
        
        #[config(label = "Enabled", desc = "Whether the feature is enabled")]
        pub enabled: bool,
        
        #[config(label = "Ratio", desc = "A floating point ratio")]
        pub ratio: f32,
    }
    
    #[test]
    fn test_derive_macro_with_panel_state() {
        // Create a test config
        let mut config = TestConfig {
            name: "Test".to_string(),
            count: 42,
            enabled: true,
            ratio: 0.5,
        };
        
        // Verify schema
        let schema = TestConfig::schema();
        assert_eq!(schema.name, "TestConfig");
        assert_eq!(schema.description, Some("A test configuration"));
        assert_eq!(schema.fields.len(), 4);
        
        // Verify field details
        assert_eq!(schema.fields[0].label, "Name");
        assert_eq!(schema.fields[1].label, "Count");
        assert_eq!(schema.fields[2].label, "Enabled");
        assert_eq!(schema.fields[3].label, "Ratio");
        
        // Create panel state
        let mut state = ConfigPanelState::new();
        state.init_with_schema(schema);
        assert_eq!(state.field_count, 4);
        
        // Navigate
        state.next_field();
        assert_eq!(state.selected_field, 1);
        
        // Get values and modify
        let mut values = config.to_values();
        assert_eq!(values.values.len(), 4);
        
        // Toggle the boolean
        toggle_bool_field(&mut values, 2);
        
        // Apply changes
        config.apply(&values).expect("apply should succeed");
        assert!(!config.enabled);
        
        // Test get_confirm_action
        state.selected_field = 2; // Select "enabled" (boolean)
        let action = get_confirm_action(&config, &state);
        assert_eq!(action, ConfigPanelAction::ValueChanged);
        
        state.selected_field = 0; // Select "name" (string)
        let action = get_confirm_action(&config, &state);
        assert!(matches!(action, ConfigPanelAction::OpenTextInput { field_index: 0, .. }));
        
        state.selected_field = 1; // Select "count" (uint)
        let action = get_confirm_action(&config, &state);
        assert!(matches!(action, ConfigPanelAction::OpenTextInput { field_index: 1, .. }));
    }
    
    #[test]
    fn test_set_string_field() {
        let mut config = TestConfig::default();
        let mut values = config.to_values();
        
        set_string_field(&mut values, 0, "New Name");
        
        config.apply(&values).expect("apply should succeed");
        assert_eq!(config.name, "New Name");
    }
    
    #[test]
    fn test_set_uint_field() {
        let mut config = TestConfig::default();
        let mut values = config.to_values();
        
        assert!(set_uint_field(&mut values, 1, "99"));
        
        config.apply(&values).expect("apply should succeed");
        assert_eq!(config.count, 99);
        
        // Invalid number should return false
        let mut values2 = config.to_values();
        assert!(!set_uint_field(&mut values2, 1, "not a number"));
    }
    
    /// Test enum standalone - enums work by themselves
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, config::Configure)]
    pub enum TestMode {
        #[default]
        #[config(label = "Mode A")]
        ModeA,
        #[config(label = "Mode B")]
        ModeB,
        #[config(label = "Mode C")]
        ModeC,
    }
    
    #[test]
    fn test_enum_standalone() {
        let mode = TestMode::ModeB;
        
        // Verify schema
        let schema = TestMode::schema();
        assert_eq!(schema.name, "TestMode");
        
        // Convert to values
        let values = mode.to_values();
        assert_eq!(values.len(), 1);
        
        // Verify it's an enum value
        if let ConfigValue::Enum { variant_index, data } = &values.values[0] {
            assert_eq!(*variant_index, 1); // ModeB is index 1
            assert!(data.is_none()); // Unit variant has no data
        } else {
            panic!("Expected Enum value");
        }
        
        // Round-trip
        let restored = TestMode::from_values(&values).expect("from_values should succeed");
        assert_eq!(restored, TestMode::ModeB);
    }
    
    #[test]
    fn test_handle_config_nav_input() {
        let config = TestConfig {
            name: "Test".to_string(),
            count: 42,
            enabled: true,
            ratio: 0.5,
        };
        let mut state = ConfigPanelState::new();
        state.init_with_schema(TestConfig::schema());
        
        // Test navigation
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::NextField, 10);
        assert_eq!(action, ConfigPanelAction::Navigated);
        assert_eq!(state.selected_field, 1);
        
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::PrevField, 10);
        assert_eq!(action, ConfigPanelAction::Navigated);
        assert_eq!(state.selected_field, 0);
        
        // Test confirm on string field -> should open text input
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::Confirm, 10);
        assert!(matches!(action, ConfigPanelAction::OpenTextInput { field_index: 0, .. }));
        assert!(state.text_input_open);
        
        // Test typing in text input
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::TextChar('X'), 10);
        assert_eq!(action, ConfigPanelAction::Navigated);
        assert_eq!(state.text_value(), "TestX");
        
        // Test backspace
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::TextBackspace, 10);
        assert_eq!(action, ConfigPanelAction::Navigated);
        assert_eq!(state.text_value(), "Test");
        
        // Test confirm text input
        let action = handle_config_nav_input(&config, &mut state, ConfigInput::Confirm, 10);
        assert!(matches!(action, ConfigPanelAction::TextInputConfirmed { field_index: 0, value } if value == "Test"));
        assert!(!state.text_input_open);
    }
    
    #[test]
    fn test_apply_action_to_values() {
        let config = TestConfig::default();
        
        // Test text input confirmation
        let action = ConfigPanelAction::TextInputConfirmed {
            field_index: 0,
            value: "New Name".to_string(),
        };
        
        let new_values = apply_action_to_values(&config, &action);
        assert!(new_values.is_some());
        
        // Apply to a fresh config
        let mut updated_config = TestConfig::default();
        updated_config.apply(&new_values.unwrap()).expect("apply should succeed");
        assert_eq!(updated_config.name, "New Name");
    }
    
    #[test]
    fn test_toggle_bool_at() {
        let config = TestConfig {
            name: "Test".to_string(),
            count: 42,
            enabled: true,
            ratio: 0.5,
        };
        
        let new_values = toggle_bool_at(&config, 2);
        assert!(new_values.is_some());
        
        let mut updated_config = config.clone();
        updated_config.apply(&new_values.unwrap()).expect("apply should succeed");
        assert!(!updated_config.enabled);
    }
    
    // =========================================================================
    // Integration test with SerialConfig from serial-core
    // =========================================================================
    
    #[test]
    fn test_serial_config_with_panel() {
        use serial_core::{SerialConfig, DataBits, Parity, Configure};
        
        // Create a SerialConfig with non-default values
        let config = SerialConfig {
            baud_rate: 9600,
            data_bits: DataBits::Seven,
            parity: Parity::Even,
            stop_bits: serial_core::StopBits::Two,
            flow_control: serial_core::FlowControl::Hardware,
        };
        
        // Initialize panel state with schema
        let mut state = ConfigPanelState::new();
        state.init_with_schema(SerialConfig::schema());
        assert_eq!(state.field_count, 5);
        
        // Verify schema has correct labels
        let schema = SerialConfig::schema();
        assert_eq!(schema.fields[0].label, "Baud Rate");
        assert_eq!(schema.fields[1].label, "Data Bits");
        assert_eq!(schema.fields[2].label, "Parity");
        
        // Test navigation
        state.next_field();
        assert_eq!(state.selected_field, 1); // Data Bits
        
        // Test getting the action for an enum field (should open dropdown)
        let action = get_confirm_action(&config, &state);
        assert!(matches!(action, ConfigPanelAction::OpenDropdown { field_index: 1 }));
        
        // Test enum dropdown - get variant labels from schema
        if let FieldType::Enum { variants } = &schema.fields[1].field_type {
            assert_eq!(variants.len(), 4); // 5, 6, 7, 8 data bits
            assert_eq!(variants[0].label, "5");
            assert_eq!(variants[3].label, "8");
        } else {
            panic!("Expected Enum field type for data_bits");
        }
        
        // Simulate changing baud rate via text input
        state.selected_field = 0;
        let action = get_confirm_action(&config, &state);
        assert!(matches!(action, ConfigPanelAction::OpenTextInput { field_index: 0, .. }));
        
        // Apply a baud rate change
        let mut values = config.to_values();
        assert!(set_uint_field(&mut values, 0, "115200"));
        
        let mut updated = SerialConfig::default();
        updated.apply(&values).expect("apply should succeed");
        assert_eq!(updated.baud_rate, 115200);
        assert_eq!(updated.data_bits, DataBits::Seven); // Preserved from original values
    }
    
    #[test]
    fn test_serial_config_enum_variant_change() {
        use serial_core::{SerialConfig, DataBits, Configure};
        use config::ConfigValue;
        
        let mut config = SerialConfig::default();
        let mut values = config.to_values();
        
        // Change data bits from Eight (index 3) to Six (index 1)
        set_enum_variant(&mut values, 1, 1);
        
        config.apply(&values).expect("apply should succeed");
        assert_eq!(config.data_bits, DataBits::Six);
    }
}
