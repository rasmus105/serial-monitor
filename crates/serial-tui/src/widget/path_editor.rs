//! Focused path editor overlay with path completion.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::{
    theme::Theme,
    widget::{
        CompletionKind, CompletionPopup, CompletionState, TextInput,
        text_input::{TextInputState, find_path_completions},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathEditorAction {
    None,
    Applied,
    Cancelled,
}

#[derive(Debug, Default, Clone)]
pub struct PathEditorState {
    pub visible: bool,
    input: TextInputState,
    completion: CompletionState,
}

impl PathEditorState {
    pub fn open(&mut self, content: &str) {
        self.visible = true;
        self.input.set_content(content);
        self.completion.hide();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.completion.hide();
    }

    pub fn content(&self) -> &str {
        self.input.content()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PathEditorAction {
        if !self.visible {
            return PathEditorAction::None;
        }

        match key.code {
            KeyCode::Enter => PathEditorAction::Applied,
            KeyCode::Esc => {
                if self.completion.visible {
                    self.completion.hide();
                    PathEditorAction::None
                } else {
                    PathEditorAction::Cancelled
                }
            }
            KeyCode::Down => {
                if !self.completion.visible {
                    self.update_completions();
                } else {
                    self.completion.next();
                }
                PathEditorAction::None
            }
            KeyCode::Up => {
                if !self.completion.visible {
                    self.update_completions();
                }
                if self.completion.visible {
                    self.completion.prev();
                }
                PathEditorAction::None
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.completion.visible {
                    self.update_completions();
                } else {
                    self.completion.next();
                }
                PathEditorAction::None
            }
            KeyCode::Char('k')
                if self.completion.visible && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.completion.prev();
                PathEditorAction::None
            }
            KeyCode::Char('g')
                if self.completion.visible && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.apply_completion();
                self.completion.hide();
                PathEditorAction::None
            }
            _ => {
                if self.input.handle_key(key) {
                    self.update_completions();
                } else {
                    self.completion.hide();
                }
                PathEditorAction::None
            }
        }
    }

    fn update_completions(&mut self) {
        let completions = find_path_completions(self.input.content());
        self.completion.show(completions, CompletionKind::Argument);
    }

    fn apply_completion(&mut self) {
        if let Some(value) = self.completion.selected_value() {
            self.input.set_content(value.to_string());
        }
    }
}

pub struct PathEditor<'a> {
    state: &'a mut PathEditorState,
    title: &'a str,
    disconnected: bool,
}

impl<'a> PathEditor<'a> {
    pub fn new(state: &'a mut PathEditorState, title: &'a str) -> Self {
        Self {
            state,
            title,
            disconnected: false,
        }
    }

    pub fn disconnected(mut self, disconnected: bool) -> Self {
        self.disconnected = disconnected;
        self
    }
}

impl Widget for PathEditor<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || area.width < 8 || area.height < 5 {
            return;
        }

        let max_width = area.width.saturating_sub(2);
        let width = if max_width >= 36 {
            (area.width * 9 / 10).clamp(36, max_width)
        } else {
            max_width
        };
        let height = 5.min(area.height);
        let x = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        let overlay = Rect::new(x, y, width, height);

        Clear.render(overlay, buf);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(if self.disconnected {
                Theme::border_disconnected()
            } else {
                Theme::border()
            });
        let inner = block.inner(overlay);
        block.render(overlay, buf);

        if inner.height < 3 {
            return;
        }

        let input_area = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1);
        TextInput::new(&mut self.state.input)
            .focused(true)
            .render(input_area, buf);

        let key_style = if self.disconnected {
            Theme::keybind_disconnected()
        } else {
            Theme::keybind()
        };
        let hints = Line::from(vec![
            Span::styled("[Up/Down]", key_style),
            Span::raw("/"),
            Span::styled("[C-j/k]", key_style),
            Span::raw(" Suggest  "),
            Span::styled("[C-g]", key_style),
            Span::raw(" Select  "),
            Span::styled("[Enter]", key_style),
            Span::raw(" Apply  "),
            Span::styled("[Esc]", key_style),
            Span::raw(" Cancel"),
        ]);
        Paragraph::new(hints)
            .alignment(Alignment::Center)
            .style(Theme::muted())
            .render(Rect::new(inner.x, inner.y + 2, inner.width, 1), buf);

        CompletionPopup::new(&self.state.completion, input_area.y, input_area.x)
            .disconnected(self.disconnected)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Mutex, MutexGuard},
    };

    use crossterm::event::KeyModifiers;

    use super::*;

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

    fn enter_temp_dir(name: &str) -> (MutexGuard<'static, ()>, std::path::PathBuf) {
        let guard = CURRENT_DIR_LOCK.lock().unwrap();
        let original = std::env::current_dir().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "serial-tui-path-editor-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        (guard, original)
    }

    #[test]
    fn ctrl_g_accepts_selected_completion_after_down() {
        let (_guard, original) = enter_temp_dir("accept-down");
        fs::create_dir("Documents").unwrap();
        fs::write("Downloads", "file").unwrap();

        let mut editor = PathEditorState::default();
        editor.open("Do");
        assert_eq!(
            editor.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            PathEditorAction::None
        );
        assert_eq!(editor.content(), "Do");
        assert_eq!(
            editor.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            PathEditorAction::None
        );

        std::env::set_current_dir(original).unwrap();
        assert_eq!(editor.content(), "Documents/");
    }

    #[test]
    fn completion_appears_while_typing() {
        let (_guard, original) = enter_temp_dir("typing-opens");
        fs::create_dir("Documents").unwrap();
        fs::write("Downloads", "file").unwrap();

        let mut editor = PathEditorState::default();
        editor.open("");
        editor.handle_key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::NONE));
        editor.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        std::env::set_current_dir(original).unwrap();
        assert!(editor.completion.visible);
        assert_eq!(editor.content(), "Do");
    }

    #[test]
    fn up_opens_completion_at_previous_item() {
        let (_guard, original) = enter_temp_dir("up-wraps");
        fs::create_dir("Documents").unwrap();
        fs::write("Downloads", "file").unwrap();

        let mut editor = PathEditorState::default();
        editor.open("Do");
        editor.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        editor.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));

        std::env::set_current_dir(original).unwrap();
        assert_eq!(editor.content(), "Downloads");
    }
}
