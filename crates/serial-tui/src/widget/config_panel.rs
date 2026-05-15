//! Config panel widget using serial-core's declarative config system.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Widget, Wrap,
    },
};
use serial_core::ui::config::{ConfigNav, FieldDef, FieldKind, FieldValue, Section};
use serial_core::ui::slice_by_display_width;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

// Re-export ConfigKeyResult for convenience
pub use serial_core::ui::config::ConfigKeyResult;

const CONFIG_PAGE_JUMP_FIELDS: usize = 10;

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
        KeyCode::Char('d') if has_ctrl => {
            for _ in 0..CONFIG_PAGE_JUMP_FIELDS {
                nav.next_field(sections, config);
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Char('u') if has_ctrl => {
            for _ in 0..CONFIG_PAGE_JUMP_FIELDS {
                nav.prev_field(sections, config);
            }
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
        _ => ConfigKeyResult::NotHandled,
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
        KeyCode::Char('d') if has_ctrl => {
            for _ in 0..CONFIG_PAGE_JUMP_FIELDS {
                nav.dropdown_next(sections, config);
            }
            ConfigKeyResult::Handled
        }
        KeyCode::Char('u') if has_ctrl => {
            for _ in 0..CONFIG_PAGE_JUMP_FIELDS {
                nav.dropdown_prev(sections, config);
            }
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
    let is_numeric = nav
        .current_field(sections, config)
        .map(|f| f.kind.is_numeric_input())
        .unwrap_or(false);

    match key.code {
        KeyCode::Enter => match nav.apply_text_edit(sections, config) {
            Ok(()) => ConfigKeyResult::Changed,
            Err(msg) => ConfigKeyResult::ValidationFailed(msg),
        },
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
    fn get_dropdown_info(
        &self,
        inner: Rect,
        scroll: usize,
    ) -> Option<(u16, &'static [&'static str], usize, bool)> {
        let dropdown_index = self.nav.edit_mode.dropdown_index()?;
        if !self.focused {
            return None;
        }

        let mut row = 0usize;
        let mut field_index = 0;

        for section in self.sections {
            // Account for section header
            if section.header.is_some() {
                row += 2; // header + separator
            }

            for field in section.fields {
                if !(field.visible)(self.data) {
                    continue;
                }

                if self.nav.selected == field_index
                    && let FieldKind::Select { options } = &field.kind
                {
                    let visible_row = row.checked_sub(scroll)?;
                    if visible_row >= inner.height as usize {
                        return None;
                    }
                    return Some((
                        inner.y + visible_row as u16,
                        options,
                        dropdown_index,
                        self.disconnected,
                    ));
                }

                row += 1;
                field_index += 1;
            }

            row += 1; // spacing between sections
        }

        None
    }

    fn selected_row(&self) -> Option<usize> {
        let mut row = 0usize;
        let mut field_index = 0;

        for (section_idx, section) in self.sections.iter().enumerate() {
            if section.header.is_some() {
                row += 2;
            }

            for field in section.fields {
                if !(field.visible)(self.data) {
                    continue;
                }

                if field_index == self.nav.selected {
                    return Some(row);
                }

                row += 1;
                field_index += 1;
            }

            if section_idx < self.sections.len() - 1 {
                row += 1;
            }
        }

        None
    }

    fn content_height(&self) -> usize {
        self.sections
            .iter()
            .enumerate()
            .map(|(section_idx, section)| {
                let header_height = usize::from(section.header.is_some()) * 2;
                let fields_height = section
                    .fields
                    .iter()
                    .filter(|field| (field.visible)(self.data))
                    .count();
                let spacing = usize::from(section_idx < self.sections.len() - 1);

                header_height + fields_height + spacing
            })
            .sum()
    }

    fn effective_scroll(&self, viewport_height: usize) -> usize {
        if viewport_height == 0 {
            return self.nav.scroll;
        }

        let Some(selected_row) = self.selected_row() else {
            return self.nav.scroll;
        };

        if selected_row < self.nav.scroll {
            selected_row
        } else if selected_row >= self.nav.scroll + viewport_height {
            selected_row.saturating_sub(viewport_height.saturating_sub(1))
        } else {
            self.nav.scroll
        }
    }
}

impl<T: 'static> Widget for ConfigPanel<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let has_block = self.block.is_some();
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

        let content_height = self.content_height();
        let is_scrollable = content_height > inner.height as usize;
        let content_area = if is_scrollable && !has_block {
            Rect::new(
                inner.x,
                inner.y,
                inner.width.saturating_sub(1),
                inner.height,
            )
        } else {
            inner
        };

        if content_area.width < 4 {
            return;
        }

        let scroll = self.effective_scroll(content_area.height as usize);

        // Get dropdown info before iterating (needed for overlay)
        let dropdown_info = self.get_dropdown_info(content_area, scroll);

        let mut row = 0usize;
        let mut field_index = 0;

        // Track which fields have visible children (for tree connector rendering)
        let mut fields_with_children: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
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
            // Render section header if present
            if let Some(header) = &section.header {
                if let Some(y) = visible_y(content_area, row, scroll) {
                    let header_style = if self.read_only {
                        Theme::muted()
                    } else if self.disconnected {
                        Theme::title_disconnected()
                    } else {
                        Theme::title()
                    };

                    let line = Line::from(vec![Span::styled(header.to_string(), header_style)]);
                    Paragraph::new(line)
                        .render(Rect::new(content_area.x, y, content_area.width, 1), buf);
                }
                row += 1;

                // Separator line
                if let Some(y) = visible_y(content_area, row, scroll) {
                    let sep = "─".repeat(content_area.width as usize);
                    Paragraph::new(sep)
                        .style(Theme::muted())
                        .render(Rect::new(content_area.x, y, content_area.width, 1), buf);
                }
                row += 1;
            }

            // Collect visible fields for this section to determine last child
            let visible_fields: Vec<_> = section
                .fields
                .iter()
                .filter(|f| (f.visible)(self.data))
                .collect();

            // Render fields
            for (field_in_section_idx, field) in visible_fields.iter().enumerate() {
                let y = visible_y(content_area, row, scroll);

                let is_selected = self.focused && self.nav.selected == field_index;
                let is_dropdown_open = is_selected && self.nav.edit_mode.is_dropdown();
                let is_text_editing = is_selected && self.nav.edit_mode.is_text_input();
                let is_enabled = (field.enabled)(self.data);
                let value = (field.get)(self.data);

                // Calculate tree prefix for hierarchical display
                let tree_prefix =
                    calculate_tree_prefix(field, field_in_section_idx, &visible_fields);
                let tree_prefix_width = tree_prefix.chars().count();

                // Calculate styles based on enabled state
                let (label_style, value_style) = if self.read_only || !is_enabled {
                    (Theme::muted(), Theme::muted())
                } else if is_selected {
                    (
                        Theme::base().add_modifier(Modifier::BOLD),
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
                            let content = self
                                .nav
                                .edit_mode
                                .text_buffer()
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
                            let content = self
                                .nav
                                .edit_mode
                                .text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else {
                            (n.to_string(), None)
                        }
                    }
                    (FieldKind::NumericInput { .. }, FieldValue::Isize(n)) => {
                        if is_text_editing {
                            let content = self
                                .nav
                                .edit_mode
                                .text_buffer()
                                .map(|b| b.content())
                                .unwrap_or("");
                            (format!("{}▏", content), None)
                        } else {
                            (n.to_string(), None)
                        }
                    }
                    (FieldKind::NumericInput { .. }, FieldValue::Float(f)) => {
                        if is_text_editing {
                            let content = self
                                .nav
                                .edit_mode
                                .text_buffer()
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

                // Calculate layout: tree_prefix + label on left, value on right.
                // Both sides use display-width-aware truncation so long UTF-8 paths stay visible.
                let label = field.label;
                let available = content_area.width as usize;
                let min_gap = usize::from(available > tree_prefix_width);
                let max_value_width = available.saturating_sub(tree_prefix_width + min_gap);
                let preferred_value_width = value_str.width().min(available / 2);
                let value_width = preferred_value_width.min(max_value_width);
                let label_width =
                    available.saturating_sub(tree_prefix_width + value_width + min_gap);

                let label_display = truncate_with_ellipsis(label, label_width);
                let value_display = truncate_with_ellipsis(&value_str, value_width);

                let padding = available.saturating_sub(
                    tree_prefix_width + label_display.width() + value_display.width(),
                );

                // Build line with tree prefix
                let tree_prefix_style = Theme::muted();
                let line = Line::from(vec![
                    Span::styled(tree_prefix.clone(), tree_prefix_style),
                    Span::styled(label_display, label_style),
                    Span::raw(" ".repeat(padding)),
                    Span::styled(value_display, value_style),
                ]);

                if let Some(y) = y {
                    // Only highlight if selected AND enabled
                    if is_selected && is_enabled {
                        // Highlight the entire row
                        let highlight_area = Rect::new(content_area.x, y, content_area.width, 1);
                        for x in highlight_area.x..highlight_area.x + highlight_area.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.set_bg(Theme::SELECTION);
                            }
                        }
                    }

                    Paragraph::new(line)
                        .wrap(Wrap { trim: false })
                        .render(Rect::new(content_area.x, y, content_area.width, 1), buf);
                }

                row += 1;
                field_index += 1;
            }

            // Add spacing between sections (but not after last section)
            if section_idx < self.sections.len() - 1 {
                row += 1;
            }
        }

        if is_scrollable {
            let scrollbar_area = if has_block {
                Rect::new(
                    area.x + area.width - 1,
                    area.y + 1,
                    1,
                    area.height.saturating_sub(2),
                )
            } else {
                Rect::new(inner.x + inner.width - 1, inner.y, 1, inner.height)
            };
            let mut scrollbar_state = ScrollbarState::new(content_height)
                .position(scroll)
                .viewport_content_length(content_area.height as usize);
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }

        // Render dropdown overlay if open
        if let Some((field_y, options, selected_idx, disconnected)) = dropdown_info {
            render_dropdown_overlay(
                buf,
                content_area,
                field_y,
                options,
                selected_idx,
                disconnected,
            );
        }
    }
}

fn visible_y(inner: Rect, row: usize, scroll: usize) -> Option<u16> {
    let visible_row = row.checked_sub(scroll)?;
    if visible_row >= inner.height as usize {
        return None;
    }
    Some(inner.y + visible_row as u16)
}

fn truncate_with_ellipsis(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if value.width() <= max_width {
        return value.to_string();
    }

    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let (start, end) = slice_by_display_width(value, 0, max_width - 3);
    format!("{}...", &value[start..end])
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

    for (i, option) in options
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_options)
    {
        let y = inner.y + (i - scroll_offset) as u16;
        let is_selected = i == selected_idx;

        // Highlight selected option (use disconnected color when appropriate)
        let highlight_color = if disconnected {
            Theme::DISCONNECTED
        } else {
            Theme::PRIMARY
        };
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
        current_parent = visible_fields
            .iter()
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
            let has_more_siblings = visible_fields
                .iter()
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
            let ancestor_has_more_children = visible_fields.iter().skip(field_idx + 1).any(|f| {
                // Check if this field is a direct child of the ancestor
                // but NOT in our current branch
                f.parent_id == Some(ancestor_id)
                    && child_ancestor
                        .is_none_or(|&child| !is_descendant_of(f, child, visible_fields))
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
fn is_descendant_of<T>(
    field: &FieldDef<T>,
    ancestor_id: &str,
    visible_fields: &[&FieldDef<T>],
) -> bool {
    let mut current_parent = field.parent_id;
    while let Some(pid) = current_parent {
        if pid == ancestor_id {
            return true;
        }
        current_parent = visible_fields
            .iter()
            .find(|f| f.id == pid)
            .and_then(|f| f.parent_id);
    }
    false
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::{Block, Borders, Widget},
    };
    use serial_core::ui::config::{ConfigNav, EditMode, FieldDef, FieldKind, FieldValue, Section};

    use super::{CONFIG_PAGE_JUMP_FIELDS, ConfigKeyResult, ConfigPanel, handle_config_key};

    #[derive(Default)]
    struct TestConfig;

    static FIELDS: &[FieldDef<TestConfig>] = &[
        field("field_1", "Field 1"),
        field("field_2", "Field 2"),
        field("field_3", "Field 3"),
        field("field_4", "Field 4"),
        field("field_5", "Field 5"),
        field("field_6", "Field 6"),
    ];

    static SECTIONS: &[Section<TestConfig>] = &[Section {
        header: Some("Test"),
        fields: FIELDS,
    }];

    static SELECT_FIELDS: &[FieldDef<TestConfig>] = &[FieldDef {
        id: "select",
        label: "Select",
        kind: FieldKind::Select {
            options: &[
                "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
                "eleven", "twelve",
            ],
        },
        get: |_| FieldValue::OptionIndex(0),
        ..FieldDef::DEFAULT
    }];

    static SELECT_SECTIONS: &[Section<TestConfig>] = &[Section {
        header: None,
        fields: SELECT_FIELDS,
    }];

    const fn field(id: &'static str, label: &'static str) -> FieldDef<TestConfig> {
        FieldDef {
            id,
            label,
            get: |_| FieldValue::Bool(false),
            ..FieldDef::DEFAULT
        }
    }

    #[test]
    fn renders_selected_field_when_panel_is_shorter_than_content() {
        let config = TestConfig;
        let nav = ConfigNav {
            selected: 5,
            ..ConfigNav::default()
        };
        let area = Rect::new(0, 0, 30, 4);
        let mut buf = Buffer::empty(area);

        ConfigPanel::new(SECTIONS, &config, &nav)
            .focused(true)
            .render(area, &mut buf);

        assert!(!line_text(&buf, area, 0).contains("Field 1"));
        assert!(line_text(&buf, area, 0).contains("Field 3"));
        assert!(line_text(&buf, area, 3).contains("Field 6"));
    }

    #[test]
    fn keeps_header_visible_when_selected_field_fits_without_scrolling() {
        let config = TestConfig;
        let nav = ConfigNav {
            selected: 0,
            ..ConfigNav::default()
        };
        let area = Rect::new(0, 0, 30, 4);
        let mut buf = Buffer::empty(area);

        ConfigPanel::new(SECTIONS, &config, &nav)
            .focused(true)
            .render(area, &mut buf);

        assert!(line_text(&buf, area, 0).contains("Test"));
        assert!(line_text(&buf, area, 2).contains("Field 1"));
    }

    #[test]
    fn renders_scrollbar_only_when_content_overflows() {
        let config = TestConfig;
        let nav = ConfigNav::default();
        let narrow_area = Rect::new(0, 0, 30, 4);
        let tall_area = Rect::new(0, 0, 30, 8);
        let mut narrow_buf = Buffer::empty(narrow_area);
        let mut tall_buf = Buffer::empty(tall_area);

        ConfigPanel::new(SECTIONS, &config, &nav).render(narrow_area, &mut narrow_buf);
        ConfigPanel::new(SECTIONS, &config, &nav).render(tall_area, &mut tall_buf);

        assert_ne!(right_edge_symbol(&narrow_buf, narrow_area, 2), "]");
        assert_eq!(right_edge_symbol(&tall_buf, tall_area, 2), "]");
    }

    #[test]
    fn renders_bordered_scrollbar_on_right_border() {
        let config = TestConfig;
        let nav = ConfigNav::default();
        let area = Rect::new(0, 0, 30, 6);
        let mut buf = Buffer::empty(area);

        ConfigPanel::new(SECTIONS, &config, &nav)
            .block(Block::default().borders(Borders::ALL))
            .render(area, &mut buf);

        assert_ne!(right_edge_symbol(&buf, area, 2), "│");
        assert_eq!(symbol_at(&buf, area.width - 2, 2), "─");
    }

    #[test]
    fn ctrl_d_and_ctrl_u_jump_selected_config_field() {
        let mut config = TestConfig;
        let mut nav = ConfigNav::default();

        let result = handle_config_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            &mut nav,
            SECTIONS,
            &mut config,
        );

        assert_eq!(result, ConfigKeyResult::Handled);
        assert_eq!(nav.selected, CONFIG_PAGE_JUMP_FIELDS % FIELDS.len());

        let result = handle_config_key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut nav,
            SECTIONS,
            &mut config,
        );

        assert_eq!(result, ConfigKeyResult::Handled);
        assert_eq!(nav.selected, 0);
    }

    #[test]
    fn ctrl_d_and_ctrl_u_jump_dropdown_selection() {
        let mut config = TestConfig;
        let mut nav = ConfigNav {
            edit_mode: EditMode::Dropdown { index: 0 },
            ..ConfigNav::default()
        };

        let result = handle_config_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            &mut nav,
            SELECT_SECTIONS,
            &mut config,
        );

        assert_eq!(result, ConfigKeyResult::Handled);
        assert!(matches!(nav.edit_mode, EditMode::Dropdown { index: 10 }));

        let result = handle_config_key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut nav,
            SELECT_SECTIONS,
            &mut config,
        );

        assert_eq!(result, ConfigKeyResult::Handled);
        assert!(matches!(nav.edit_mode, EditMode::Dropdown { index: 0 }));
    }

    fn line_text(buf: &Buffer, area: Rect, y: u16) -> String {
        (area.x..area.x + area.width)
            .map(|x| buf[(x, area.y + y)].symbol())
            .collect::<String>()
    }

    fn right_edge_symbol(buf: &Buffer, area: Rect, y: u16) -> &str {
        let x = area.x + area.width - 1;
        buf[(x, area.y + y)].symbol()
    }

    fn symbol_at(buf: &Buffer, x: u16, y: u16) -> &str {
        buf[(x, y)].symbol()
    }
}
