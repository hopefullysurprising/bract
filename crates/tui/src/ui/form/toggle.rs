use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::field::{self, FieldValue, FormField};

pub struct Toggle {
    pub name: String,
    pub help: String,
    pub value: bool,
}

impl FormField for Toggle {
    fn render_lines(&self, focused: bool, width: u16) -> Vec<Line<'_>> {
        let mut label = field::label_line(&self.name, focused);

        let toggle_str = if self.value { "◉ on" } else { "○" };
        let label_width: usize = label.spans.iter().map(|s| s.content.chars().count()).sum();
        let gap = (width as usize).saturating_sub(label_width + toggle_str.chars().count());

        label.spans.push(Span::raw(" ".repeat(gap)));
        if self.value {
            label.spans.push(Span::styled(
                "◉ on".to_string(),
                Style::new().fg(Color::Cyan),
            ));
        } else {
            label.spans.push(Span::styled(
                "○".to_string(),
                Style::new().fg(Color::DarkGray),
            ));
        }

        let mut lines = vec![label];
        lines.extend(field::help_line(&self.help));
        lines
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char(' ') || key.code == KeyCode::Enter {
            self.value = !self.value;
        }
    }

    fn value(&self) -> FieldValue {
        FieldValue::Bool(self.value)
    }
}
