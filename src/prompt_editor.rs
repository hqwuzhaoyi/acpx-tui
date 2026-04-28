use ratatui::{
    style::{Color, Modifier, Style},
    widgets::Block,
};
use tui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone)]
pub struct PromptEditor {
    textarea: TextArea<'static>,
}

impl Default for PromptEditor {
    fn default() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_cursor_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED),
        );
        Self { textarea }
    }
}

impl PromptEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub fn set_text(&mut self, text: &str) {
        let mut textarea = if text.is_empty() {
            TextArea::default()
        } else {
            TextArea::from(text.split('\n'))
        };
        textarea.set_cursor_line_style(Style::default());
        textarea.set_cursor_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED),
        );
        textarea.move_cursor(CursorMove::Bottom);
        textarea.move_cursor(CursorMove::End);
        self.textarea = textarea;
    }

    pub fn clear(&mut self) {
        self.set_text("");
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    pub fn insert_char(&mut self, c: char) {
        self.textarea.insert_char(c);
    }

    pub fn insert_str(&mut self, text: &str) -> bool {
        self.textarea.insert_str(text)
    }

    pub fn insert_newline(&mut self) {
        self.textarea.insert_newline();
    }

    pub fn delete_before_cursor(&mut self) -> bool {
        self.textarea.delete_char()
    }

    pub fn delete_after_cursor(&mut self) -> bool {
        self.textarea.delete_next_char()
    }

    pub fn delete_to_line_start(&mut self) -> bool {
        if self.cursor_col() == 0 {
            return false;
        }
        self.textarea.delete_line_by_head()
    }

    pub fn delete_to_buffer_start(&mut self) -> bool {
        let mut changed = false;
        while !self.is_at_start() {
            changed |= self.textarea.delete_char();
        }
        changed
    }

    pub fn delete_to_line_end(&mut self) -> bool {
        if self.cursor_col() == self.current_line_len() {
            return false;
        }
        self.textarea.delete_line_by_end()
    }

    pub fn delete_word_before(&mut self) -> bool {
        self.textarea.delete_word()
    }

    pub fn move_left(&mut self) {
        self.textarea.move_cursor(CursorMove::Back);
    }

    pub fn move_right(&mut self) {
        self.textarea.move_cursor(CursorMove::Forward);
    }

    pub fn move_up(&mut self) {
        self.textarea.move_cursor(CursorMove::Up);
    }

    pub fn move_down(&mut self) {
        self.textarea.move_cursor(CursorMove::Down);
    }

    pub fn move_to_line_start(&mut self) {
        self.textarea.move_cursor(CursorMove::Head);
    }

    pub fn move_to_line_end(&mut self) {
        self.textarea.move_cursor(CursorMove::End);
    }

    pub fn is_at_start(&self) -> bool {
        self.textarea.cursor() == (0, 0)
    }

    pub fn is_at_end(&self) -> bool {
        let (row, col) = self.textarea.cursor();
        row + 1 == self.textarea.lines().len() && col == self.current_line_len()
    }

    #[cfg(test)]
    pub fn cursor(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    pub fn current_line_len(&self) -> usize {
        let row = self.textarea.cursor().0;
        self.textarea
            .lines()
            .get(row)
            .map(|line| line.chars().count())
            .unwrap_or(0)
    }

    pub fn cursor_col(&self) -> usize {
        self.textarea.cursor().1
    }

    pub fn set_block(&mut self, block: Block<'static>) {
        self.textarea.set_block(block);
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.textarea.set_placeholder_text(placeholder);
    }

    pub fn widget(&self) -> &TextArea<'static> {
        &self.textarea
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_text_at_cursor_and_newlines() {
        let mut editor = PromptEditor::new();
        editor.insert_str("hello");
        editor.move_left();
        editor.insert_char('!');
        editor.insert_newline();

        assert_eq!(editor.text(), "hell!\no");
        assert_eq!(editor.cursor(), (1, 0));
    }

    #[test]
    fn deletes_before_and_after_cursor() {
        let mut editor = PromptEditor::new();
        editor.insert_str("abc");
        editor.move_left();

        assert!(editor.delete_before_cursor());
        assert_eq!(editor.text(), "ac");
        assert!(editor.delete_after_cursor());
        assert_eq!(editor.text(), "a");
    }

    #[test]
    fn deletes_to_line_boundaries_without_crossing_lines() {
        let mut editor = PromptEditor::new();
        editor.insert_str("abc\ndef");
        editor.move_to_line_start();
        editor.move_right();

        assert!(editor.delete_to_line_start());
        assert_eq!(editor.text(), "abc\nef");

        editor.move_to_line_end();
        editor.move_to_line_start();
        assert!(!editor.delete_to_line_start());

        editor.move_to_line_end();
        editor.move_left();
        assert!(editor.delete_to_line_end());
        assert_eq!(editor.text(), "abc\ne");
        assert!(!editor.delete_to_line_end());
    }

    #[test]
    fn deletes_to_buffer_start() {
        let mut editor = PromptEditor::new();
        editor.insert_str("one\ntwo");

        assert!(editor.delete_to_buffer_start());
        assert_eq!(editor.text(), "");
        assert!(editor.is_at_start());
    }

    #[test]
    fn deletes_previous_word() {
        let mut editor = PromptEditor::new();
        editor.insert_str("one two");

        assert!(editor.delete_word_before());
        assert_eq!(editor.text(), "one ");
    }

    #[test]
    fn reports_boundaries() {
        let mut editor = PromptEditor::new();
        assert!(editor.is_at_start());
        assert!(editor.is_at_end());

        editor.insert_str("a\nb");
        assert!(!editor.is_at_start());
        assert!(editor.is_at_end());

        editor.move_left();
        assert!(!editor.is_at_end());
    }
}
