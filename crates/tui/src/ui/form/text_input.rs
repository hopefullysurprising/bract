use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::field::{self, FieldValue, FormField};

pub struct TextInput {
    pub name: String,
    pub help: String,
    pub chars: Vec<char>,
    pub cursor: usize,
}

impl FormField for TextInput {
    fn render_lines(&self, focused: bool, _width: u16) -> Vec<Line<'_>> {
        let mut lines = vec![field::label_line(&self.name, focused)];

        let track = Span::styled("  │ ".to_string(), Style::new().fg(Color::DarkGray));

        if focused {
            let before: String = self.chars[..self.cursor].iter().collect();
            let cursor_ch = self
                .chars
                .get(self.cursor)
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".to_string());
            let after: String = if self.cursor < self.chars.len() {
                self.chars[self.cursor + 1..].iter().collect()
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                track,
                Span::styled(before, Style::new().fg(Color::White)),
                Span::styled(cursor_ch, Style::new().fg(Color::Black).bg(Color::White)),
                Span::styled(after, Style::new().fg(Color::White)),
            ]));
        } else {
            let value: String = self.chars.iter().collect();
            lines.push(Line::from(vec![
                track,
                Span::styled(value, Style::new().fg(Color::Gray)),
            ]));
        }

        lines.extend(field::help_line(&self.help));
        lines
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.chars.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.chars.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            _ => {}
        }
    }

    fn value(&self) -> FieldValue {
        FieldValue::Text(self.chars.iter().collect())
    }
}
