use ratatui::crossterm::event::KeyEvent;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub trait FormField {
    fn render_lines(&self, focused: bool, width: u16) -> Vec<Line<'_>>;
    fn handle_key(&mut self, key: KeyEvent);
}

pub fn label_line(name: &str, focused: bool) -> Line<'static> {
    let (indicator, indicator_style) = if focused {
        ("▸ ", Style::new().fg(Color::Cyan))
    } else {
        ("  ", Style::default())
    };
    let label_style = if focused {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::White)
    };

    Line::from(vec![
        Span::styled(indicator.to_string(), indicator_style),
        Span::styled(name.to_string(), label_style),
    ])
}

pub fn help_line(help: &str) -> Option<Line<'static>> {
    if help.is_empty() {
        return None;
    }
    Some(Line::from(vec![
        Span::raw("  "),
        Span::styled(help.to_string(), Style::new().fg(Color::DarkGray)),
    ]))
}
