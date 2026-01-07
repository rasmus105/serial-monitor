//! Config panel widget using serial-core's declarative config system.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use serial_core::ui::config::{ConfigNav, FieldDef, FieldKind, FieldValue, Section};

use crate::theme::Theme;

// Re-export ConfigKeyResult for convenience
pub use serial_core::ui::config::ConfigKeyResult;

/// Handle a key event for a config panel.
///
/// This is a helper function that handles the common config panel key bindings.
/// Returns what action was taken so the caller can respond appropriately
/// (e.g., sync to buffer, request screen clear).
///
/// # Example
///
/// ```ignore
/// let result = handle_config_key(key, &mut self.config_nav, SECTIONS, &mut self.config);
/// if result.changed() {
///     self.sync_config_to_buffer(handle);
/// }
/// if result != ConfigKeyResult::NotHandled {
///     return Some(TrafficAction::RequestClear);
/// }
/// ```
pub fn handle_config_key<T: 'static>(
    key: KeyEvent,
    nav: &mut ConfigNav,
    sections: &[Section<T>],
    config: &mut T,
) -> ConfigKeyResult {
    // Handle text editing mode
    if nav.edit_mode.is_text_input() {
        return handle_text_edit_key(key, nav, sections, config);
    }

    // Handle dropdown mode separately
    if nav.edit_mode.is_dropdown() {
        return handle_dropdown_key(key, nav, sections, config);
    }

    // Ignore j/k with CTRL modifier (let it be consumed without action)
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
            nav.next_field(sections, config);
            ConfigKeyResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
            nav.prev_field(sections, config);
            ConfigKeyResult::Handled
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(field) = nav.current_field(sections, config) {
                if matches!(field.kind, FieldKind::Toggle) {
                    let _ = nav.toggle_current(sections, config);
                    return ConfigKeyResult::Changed;
                } else if field.kind.is_select() {
                    let _ = nav.cycle_select_prev(sections, config);
                    return ConfigKeyResult::Changed;
                }
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(field) = nav.current_field(sections, config) {
                if matches!(field.kind, FieldKind::Toggle) {
                    let _ = nav.toggle_current(sections, config);
                    return ConfigKeyResult::Changed;
                } else if field.kind.is_select() {
                    let _ = nav.cycle_select_next(sections, config);
                    return ConfigKeyResult::Changed;
                }
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(field) = nav.current_field(sections, config) {
                if field.kind.is_select() {
                    nav.open_dropdown(sections, config);
                    return ConfigKeyResult::Handled;
                } else if field.kind.is_editable() {
                    nav.start_text_edit(sections, config);
                    return ConfigKeyResult::Handled;
                } else if matches!(field.kind, FieldKind::Toggle) {
                    let _ = nav.toggle_current(sections, config);
                    return ConfigKeyResult::Changed;
                }
            }
            ConfigKeyResult::Handled
        }
        _ => {
            ConfigKeyResult::NotHandled
        }
    }
}

/// Handle a key event when a dropdown is open.
fn handle_dropdown_key<T: 'static>(
    key: KeyEvent,
    nav: &mut ConfigNav,
    sections: &[Section<T>],
    config: &mut T,
) -> ConfigKeyResult {
    // Ignore j/k with CTRL modifier
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down if !has_ctrl => {
            nav.dropdown_next(sections, config);
            ConfigKeyResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up if !has_ctrl => {
            nav.dropdown_prev(sections, config);
            ConfigKeyResult::Handled
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let _ = nav.apply_dropdown(sections, config);
            ConfigKeyResult::Changed
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            nav.close_dropdown();
            ConfigKeyResult::EditClosed
        }
        _ => ConfigKeyResult::NotHandled,
    }
}

/// Handle a key event when editing text.
fn handle_text_edit_key<T: 'static>(
    key: KeyEvent,
    nav: &mut ConfigNav,
    sections: &[Section<T>],
    config: &mut T,
) -> ConfigKeyResult {
    // Check if we're editing a numeric field
    let is_numeric = nav.current_field(sections, config)
        .map(|f| f.kind.is_numeric_input())
        .unwrap_or(false);
    
    match key.code {
        KeyCode::Enter => {
            match nav.apply_text_edit(sections, config) {
                Ok(()) => ConfigKeyResult::Changed,
                Err(msg) => ConfigKeyResult::ValidationFailed(msg),
            }
        }
        KeyCode::Esc => {
            nav.cancel_text_edit();
            ConfigKeyResult::EditClosed
        }
        KeyCode::Char(c) => {
            // Handle Ctrl+<key> sequences
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                    match c {
                        'a' => buf.move_start(),
                        'e' => buf.move_end(),
                        'u' => buf.delete_to_start(),
                        'w' => buf.delete_word_before(),
                        'k' => buf.delete_to_end(),
                        _ => {}
                    }
                }
                ConfigKeyResult::Handled
            } else {
                // For numeric fields, only allow digits
                if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                    if is_numeric {
                        if c.is_ascii_digit() {
                            buf.insert_char(c);
                        }
                        // Silently ignore non-digit characters for numeric input
                    } else {
                        buf.insert_char(c);
                    }
                }
                ConfigKeyResult::Handled
            }
        }
        KeyCode::Backspace => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                buf.delete_char_before();
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Delete => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                buf.delete_char_after();
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Left => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    buf.move_word_left();
                } else {
                    buf.move_left();
                }
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Right => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    buf.move_word_right();
                } else {
                    buf.move_right();
                }
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Home => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                buf.move_start();
            }
            ConfigKeyResult::Handled
        }
        KeyCode::End => {
            if let Some(buf) = nav.edit_mode.text_buffer_mut() {
                buf.move_end();
            }
            ConfigKeyResult::Handled
        }
        _ => ConfigKeyResult::Handled, // Consume all other keys when editing
    }
}

/// Config panel widget.
pub struct ConfigPanel<'a, T: 'static> {
    sections: &'a [Section<T>],
    data: &'a T,
    nav: &'a ConfigNav,
    focused: bool,
    block: Option<Block<'a>>,
    /// Whether to show config as read-only (grayed out).
    read_only: bool,
    /// Whether to use disconnected theming (yellow instead of cyan).
    disconnected: bool,
}

impl<'a, T: 'static> ConfigPanel<'a, T> {
    pub fn new(sections: &'a [Section<T>], data: &'a T, nav: &'a ConfigNav) -> Self {
        Self {
            sections,
            data,
            nav,
            focused: false,
            block: None,
            read_only: false,
            disconnected: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Use disconnected theming (yellow section headers instead of cyan).
    pub fn disconnected(mut self, disconnected: bool) -> Self {
        self.disconnected = disconnected;
        self
    }

    /// Get info needed to render dropdown overlay after the main panel.
    /// Returns (field_y, options, selected_index, disconnected) if dropdown is open.
    fn get_dropdown_info(&self, inner: Rect) -> Option<(u16, &'static [&'static str], usize, bool)> {
        let dropdown_index = self.nav.edit_mode.dropdown_index()?;
        if !self.focused {
            return None;
        }

        let mut y = inner.y;
        let mut field_index = 0;

        for section in self.sections {
            if y >= inner.y + inner.height {
                break;
            }

            // Account for section header
            if section.header.is_some() {
                y += 2; // header + separator
            }

            for field in section.fields {
                if y >= inner.y + inner.height {
                    break;
                }

                if !(field.visible)(self.data) {
                    continue;
                }

                if self.nav.selected == field_index
                    && let FieldKind::Select { options } = &field.kind
                {
                    return Some((y, options, dropdown_index, self.disconnected));
                }

                y += 1;
                field_index += 1;
            }

            y += 1; // spacing between sections
        }

        None
    }
}

impl<T: 'static> Widget for ConfigPanel<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < 4 || inner.height < 1 {
            return;
        }

        // Get dropdown info before iterating (needed for overlay)
        let dropdown_info = self.get_dropdown_info(inner);

        let mut y = inner.y;
        let mut field_index = 0;

        // Track which fields have visible children (for tree connector rendering)
        let mut fields_with_children: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for section in self.sections {
            for field in section.fields {
                if !(field.visible)(self.data) {
                    continue;
                }
                if let Some(parent_id) = field.parent_id {
                    fields_with_children.insert(parent_id);
                }
            }
        }

        for (section_idx, section) in self.sections.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            // Render section header if present
            if let Some(header) = &section.header {
                let header_style = if self.read_only {
                    Theme::muted()
                } else if self.disconnected {
                    Theme::title_disconnected()
                } else {
                    Theme::title()
                };

                let line = Line::from(vec![Span::styled(header.to_string(), header_style)]);
                Paragraph::new(line).render(Rect::new(inner.x, y, inner.width, 1), buf);
                y += 1;

                // Separator line
                if y < inner.y + inner.height {
                    let sep = "─".repeat(inner.width as usize);
                    Paragraph::new(sep)
                        .style(Theme::muted())
                        .render(Rect::new(inner.x, y, inner.width, 1), buf);
                    y += 1;
                }
            }

            // Collect visible fields for this section to determine last child
            let visible_fields: Vec<_> = section.fields.iter()
                .filter(|f| (f.visible)(self.data))
                .collect();

            // Render fields
            for (field_in_section_idx, field) in visible_fields.iter().enumerate() {
                if y >= inner.y + inner.height {
                    break;
                }

                let is_selected = self.focused && self.nav.selected == field_index;
                let is_dropdown_open = is_selected && self.nav.edit_mode.is_dropdown();
                let is_text_editing = is_selected && self.nav.edit_mode.is_text_input();
                let is_enabled = (field.enabled)(self.data);
                let value = (field.get)(self.data);

                // Calculate tree prefix for hierarchical display
                let tree_prefix = calculate_tree_prefix(
                    field,
                    field_in_section_idx,
                    &visible_fields,
                );
                let tree_prefix_width = tree_prefix.chars().count();

                // Calculate styles based on enabled state
                let (label_style, value_style) = if self.read_only || !is_enabled {
                    (Theme::muted(), Theme::muted())
                } else if is_selected {
                    (
                        Style::default().add_modifier(Modifier::BOLD),
                        Theme::highlight(),
                    )
                } else {
                    (Theme::base(), Theme::base())
                };

                // Format field based on kind
                let (value_str, value_style_override) = match (&field.kind, &value) {
                    (FieldKind::Toggle, FieldValue::Bool(b)) => {
                        // Special handling for send_active toggle
                        if field.id == "send_active" {
                            if *b {
                                ("[ Stop]".to_string(), Some(Theme::error()))
                            } else {
                                ("[Start]".to_string(), Some(Theme::success()))
                            }
                        } else {
                            (if *b { "[x]" } else { "[ ]" }.to_string(), None)
                        }
                    }
                    (FieldKind::Select { options }, FieldValue::OptionIndex(i)) => {
                        let option = options.get(*i).unwrap_or(&"?");
                        if is_dropdown_open {
                            // Show dropdown indicator when open
                            (format!("[{}] ▼", option), None)
                        } else {
                            (format!("[{}]", option), None)
                        }
                    }
                    (FieldKind::TextInput { placeholder }, FieldValue::String(s)) => {
                        if is_text_editing {
                            // Show edit buffer with cursor indicator
                            let content = self.nav.edit_mode.text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else if s.is_empty() {
                            (format!("[{}]", placeholder), None)
                        } else {
                            (s.to_string(), None)
                        }
                    }
                    (FieldKind::NumericInput { .. }, FieldValue::Usize(n)) => {
                        if is_text_editing {
                            // Show edit buffer with cursor indicator
                            let content = self.nav.edit_mode.text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else {
                            (n.to_string(), None)
                        }
                    }
                    (FieldKind::NumericInput { .. }, FieldValue::Isize(n)) => {
                        if is_text_editing {
                            let content = self.nav.edit_mode.text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else {
                            (n.to_string(), None)
                        }
                    }
                    (FieldKind::NumericInput { .. }, FieldValue::Float(f)) => {
                        if is_text_editing {
                            let content = self.nav.edit_mode.text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else {
                            (format!("{:.2}", f), None)
                        }
                    }
                    _ => ("?".to_string(), None),
                };

                // Apply value style override if present (for special toggles like send_active)
                let value_style = value_style_override.unwrap_or(value_style);

                // Calculate layout: tree_prefix + label on left, value on right
                let label = &field.label;
                let available = inner.width as usize;
                let value_width = value_str.len().min(available / 2);
                let label_width = available.saturating_sub(value_width + 1 + tree_prefix_width);

                let label_display: String = if label.len() > label_width {
                    format!("{}...", &label[..label_width.saturating_sub(3)])
                } else {
                    (*label).to_string()
                };

                let padding = available.saturating_sub(tree_prefix_width + label_display.len() + value_str.len());

                // Build line with tree prefix
                let tree_prefix_style = Theme::muted();
                let line = Line::from(vec![
                    Span::styled(tree_prefix.clone(), tree_prefix_style),
                    Span::styled(label_display, label_style),
                    Span::raw(" ".repeat(padding)),
                    Span::styled(value_str, value_style),
                ]);

                // Only highlight if selected AND enabled
                if is_selected && is_enabled {
                    // Highlight the entire row
                    let highlight_area = Rect::new(inner.x, y, inner.width, 1);
                    for x in highlight_area.x..highlight_area.x + highlight_area.width {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_bg(Theme::SELECTION);
                        }
                    }
                }

                Paragraph::new(line)
                    .wrap(Wrap { trim: false })
                    .render(Rect::new(inner.x, y, inner.width, 1), buf);

                y += 1;
                field_index += 1;
            }

            // Add spacing between sections (but not after last section)
            if section_idx < self.sections.len() - 1 {
                y += 1;
            }
        }

        // Render dropdown overlay if open
        if let Some((field_y, options, selected_idx, disconnected)) = dropdown_info {
            render_dropdown_overlay(buf, inner, field_y, options, selected_idx, disconnected);
        }
    }
}

/// Render a dropdown overlay below the selected field.
fn render_dropdown_overlay(
    buf: &mut Buffer,
    panel_area: Rect,
    field_y: u16,
    options: &[&str],
    selected_idx: usize,
    disconnected: bool,
) {
    // Calculate dropdown dimensions
    let max_option_len = options.iter().map(|s| s.len()).max().unwrap_or(10);
    let dropdown_width = (max_option_len + 4).min(panel_area.width as usize) as u16;
    let dropdown_height = (options.len() + 2).min(10) as u16; // +2 for border

    // Position: below the field, right-aligned to panel
    let dropdown_x = panel_area.x + panel_area.width - dropdown_width;
    let dropdown_y = field_y + 1;

    // Ensure it fits in the terminal
    let dropdown_area = Rect::new(
        dropdown_x,
        dropdown_y,
        dropdown_width,
        dropdown_height.min(panel_area.y + panel_area.height - dropdown_y),
    );

    if dropdown_area.height < 3 {
        return; // Not enough space
    }

    // Clear and draw border
    Clear.render(dropdown_area, buf);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if disconnected {
            Theme::border_disconnected()
        } else {
            Theme::border_focused()
        });

    let inner = block.inner(dropdown_area);
    block.render(dropdown_area, buf);

    // Render options
    let visible_options = (inner.height as usize).min(options.len());
    let scroll_offset = if selected_idx >= visible_options {
        selected_idx - visible_options + 1
    } else {
        0
    };

    for (i, option) in options.iter().enumerate().skip(scroll_offset).take(visible_options) {
        let y = inner.y + (i - scroll_offset) as u16;
        let is_selected = i == selected_idx;

        // Highlight selected option (use disconnected color when appropriate)
        let highlight_color = if disconnected { Theme::DISCONNECTED } else { Theme::PRIMARY };
        if is_selected {
            for x in inner.x..inner.x + inner.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(highlight_color);
                    cell.set_fg(Theme::BG);
                }
            }
        }

        let prefix = if is_selected { ">" } else { " " };
        let line = Line::from(format!("{} {}", prefix, option));

        let style = if is_selected {
            Style::default().fg(Theme::BG).add_modifier(Modifier::BOLD)
        } else {
            Theme::base()
        };

        Paragraph::new(line)
            .style(style)
            .render(Rect::new(inner.x, y, inner.width, 1), buf);
    }
}

/// Calculate tree prefix for hierarchical field display.
///
/// Builds a prefix string like "├ ", "└ ", "│ ├ ", "│ └ ", etc.
/// based on the field's position in the parent-child hierarchy.
fn calculate_tree_prefix<T>(
    field: &FieldDef<T>,
    field_idx: usize,
    visible_fields: &[&FieldDef<T>],
) -> String {
    let Some(parent_id) = field.parent_id else {
        return String::new();
    };

    // Build the ancestry chain from root to this field's parent
    let mut ancestry = Vec::new();
    let mut current_parent = Some(parent_id);
    
    while let Some(pid) = current_parent {
        ancestry.push(pid);
        // Find the parent field and get its parent
        current_parent = visible_fields.iter()
            .find(|f| f.id == pid)
            .and_then(|f| f.parent_id);
    }
    
    // Reverse so we go from root to immediate parent
    ancestry.reverse();
    
    let mut prefix = String::new();
    
    // For each ancestor level, determine if we need "│ " (has more siblings) or "  " (no more siblings)
    for (level, &ancestor_id) in ancestry.iter().enumerate() {
        if level == ancestry.len() - 1 {
            // This is the immediate parent - use ├ or └
            let has_more_siblings = visible_fields.iter()
                .skip(field_idx + 1)
                .any(|f| f.parent_id == Some(ancestor_id));
            
            if has_more_siblings {
                prefix.push_str("├ ");
            } else {
                prefix.push_str("└ ");
            }
        } else {
            // This is a grandparent or higher - check if it has more children after current branch
            // Find the child of this ancestor that is an ancestor of our field
            let child_ancestor = ancestry.get(level + 1);
            
            // Check if the ancestor has more children after the branch we're in
            let ancestor_has_more_children = visible_fields.iter()
                .skip(field_idx + 1)
                .any(|f| {
                    // Check if this field is a direct child of the ancestor
                    // but NOT in our current branch
                    f.parent_id == Some(ancestor_id) && 
                    child_ancestor.is_none_or(|&child| !is_descendant_of(f, child, visible_fields))
                });
            
            if ancestor_has_more_children {
                prefix.push_str("│ ");
            } else {
                prefix.push_str("  ");
            }
        }
    }
    
    prefix
}

/// Check if a field is a descendant of a given ancestor (by id)
fn is_descendant_of<T>(field: &FieldDef<T>, ancestor_id: &str, visible_fields: &[&FieldDef<T>]) -> bool {
    let mut current_parent = field.parent_id;
    while let Some(pid) = current_parent {
        if pid == ancestor_id {
            return true;
        }
        current_parent = visible_fields.iter()
            .find(|f| f.id == pid)
            .and_then(|f| f.parent_id);
    }
    false
}
