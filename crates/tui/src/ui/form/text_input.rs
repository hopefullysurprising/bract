use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::field::{self, FieldValue, FormField};

pub struct TextInput {
    pub name: String,
    pub help: String,
    /// The tool's own default, shown as a dim placeholder. It is *not* part of
    /// the value — leaving the field untouched omits the flag, so the tool
    /// applies this default itself instead of us passing it back redundantly.
    pub default: String,
    /// The value entered here last time, if any. Shown as a ghost (preferred over
    /// `default`) and accepted with → / End — so a repeated argument is one key
    /// away instead of retyped.
    pub remembered: Option<String>,
    pub chars: Vec<char>,
    pub cursor: usize,
}

impl TextInput {
    /// The placeholder shown when the field is empty: the previously-used value
    /// if we have one, else the tool's default. `None` if neither exists.
    fn ghost(&self) -> Option<&str> {
        self.remembered
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(if self.default.is_empty() { None } else { Some(&self.default) })
    }

    fn accept_ghost(&mut self) {
        if self.chars.is_empty()
            && let Some(ghost) = self.ghost().map(str::to_string)
        {
            self.set_text(&ghost);
        }
    }
}

impl FormField for TextInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_text(&mut self, value: &str) -> bool {
        self.chars = value.chars().collect();
        self.cursor = self.chars.len();
        true
    }

    fn render_lines(&self, focused: bool, _width: u16) -> Vec<Line<'_>> {
        let mut lines = vec![field::label_line(&self.name, focused)];

        let track = Span::styled("  │ ".to_string(), Style::new().fg(Color::DarkGray));
        let placeholder = Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);

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

            let mut spans = vec![
                track,
                Span::styled(before, Style::new().fg(Color::White)),
                Span::styled(cursor_ch, Style::new().fg(Color::Black).bg(Color::White)),
                Span::styled(after, Style::new().fg(Color::White)),
            ];
            if self.chars.is_empty()
                && let Some(ghost) = self.ghost()
            {
                spans.push(Span::styled(ghost.to_string(), placeholder));
            }
            lines.push(Line::from(spans));
        } else if self.chars.is_empty()
            && let Some(ghost) = self.ghost()
        {
            lines.push(Line::from(vec![track, Span::styled(ghost.to_string(), placeholder)]));
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
                } else {
                    // At the end of an empty field, → accepts the ghost value.
                    self.accept_ghost();
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => {
                if self.chars.is_empty() {
                    self.accept_ghost();
                } else {
                    self.cursor = self.chars.len();
                }
            }
            _ => {}
        }
    }

    fn value(&self) -> FieldValue {
        FieldValue::Text(self.chars.iter().collect())
    }
}
