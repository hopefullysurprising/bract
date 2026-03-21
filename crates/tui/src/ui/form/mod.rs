mod field;
mod text_input;
mod toggle;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};
use ratatui::Frame;

use field::FormField;
use text_input::TextInput;
use toggle::Toggle;

use super::{View, ViewAction};
use crate::data::commands::{Command, FlagKind};

struct FormSection {
    label: String,
    fields: Vec<Box<dyn FormField>>,
}

pub struct FormView {
    title: String,
    description: String,
    sections: Vec<FormSection>,
    total_fields: usize,
    focused: usize,
    scroll_offset: u16,
}

impl FormView {
    pub fn new(ancestors: Vec<Command>, bin: &str, path_separator: &str) -> Self {
        let command_path = ancestors.iter()
            .skip(1)
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(path_separator);
        let title = if command_path.is_empty() {
            bin.to_string()
        } else {
            format!("{bin} {command_path}")
        };

        let description = ancestors.last()
            .map(|c| c.description.clone())
            .unwrap_or_default();

        let mut sections = Vec::new();
        let ancestor_count = ancestors.len();
        for (i, ancestor) in ancestors.iter().rev().enumerate() {
            let fields = build_fields(ancestor);
            if !fields.is_empty() {
                let is_leaf = i == 0 && ancestor_count > 1;
                sections.push(FormSection {
                    label: if is_leaf { String::new() } else { ancestor.name.clone() },
                    fields,
                });
            }
        }

        let total_fields = sections.iter().map(|s| s.fields.len()).sum();

        Self {
            title,
            description,
            sections,
            total_fields,
            focused: 0,
            scroll_offset: 0,
        }
    }

    fn focus_next(&mut self) {
        if self.total_fields > 0 {
            self.focused = (self.focused + 1) % self.total_fields;
        }
    }

    fn focus_prev(&mut self) {
        if self.total_fields > 0 {
            self.focused = (self.focused + self.total_fields - 1) % self.total_fields;
        }
    }

    fn focused_field_mut(&mut self) -> Option<&mut Box<dyn FormField>> {
        let mut remaining = self.focused;
        for section in &mut self.sections {
            if remaining < section.fields.len() {
                return Some(&mut section.fields[remaining]);
            }
            remaining -= section.fields.len();
        }
        None
    }
}

impl View for FormView {
    fn render(&mut self, frame: &mut Frame) {
        let area = centered_area(frame.area(), 72);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(format!(" {} ", self.title))
            .title_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .padding(Padding::new(2, 2, 1, 1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [content_area, footer_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(2)]).areas(inner);

        let mut lines: Vec<Line> = Vec::new();
        let mut focused_line_start: u16 = 0;
        let mut focused_line_end: u16 = 0;

        if !self.description.is_empty() {
            lines.push(Line::from(Span::styled(
                self.description.clone(),
                Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC),
            )));
            lines.push(Line::raw(""));
        }

        let mut global_field_index = 0;

        for (section_idx, section) in self.sections.iter().enumerate() {
            if section_idx > 0 {
                lines.push(Line::raw(""));
            }
            if !section.label.is_empty() {
                lines.push(section_header(&section.label, content_area.width));
                lines.push(Line::raw(""));
            }

            for field in &section.fields {
                let focused = global_field_index == self.focused;
                if focused {
                    focused_line_start = lines.len() as u16;
                }
                lines.extend(field.render_lines(focused, content_area.width));
                if focused {
                    focused_line_end = lines.len() as u16;
                }
                lines.push(Line::raw(""));
                global_field_index += 1;
            }
        }

        if self.total_fields == 0 {
            lines.push(Line::from(Span::styled(
                "No parameters",
                Style::new()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        let visible_height = content_area.height;
        if focused_line_end > self.scroll_offset + visible_height {
            self.scroll_offset = focused_line_end.saturating_sub(visible_height);
        }
        if focused_line_start < self.scroll_offset {
            self.scroll_offset = focused_line_start;
        }

        frame.render_widget(
            Paragraph::new(Text::from(lines)).scroll((self.scroll_offset, 0)),
            content_area,
        );
        frame.render_widget(footer(footer_area.width), footer_area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction> {
        match key.code {
            KeyCode::Esc => None,
            KeyCode::Tab => {
                self.focus_next();
                Some(ViewAction::Consumed)
            }
            KeyCode::BackTab => {
                self.focus_prev();
                Some(ViewAction::Consumed)
            }
            _ => {
                if let Some(field) = self.focused_field_mut() {
                    field.handle_key(key);
                }
                Some(ViewAction::Consumed)
            }
        }
    }
}

fn centered_area(area: Rect, max_width: u16) -> Rect {
    if area.width <= max_width {
        area
    } else {
        let x = area.x + (area.width - max_width) / 2;
        Rect::new(x, area.y, max_width, area.height)
    }
}

fn build_fields(command: &Command) -> Vec<Box<dyn FormField>> {
    let mut fields: Vec<Box<dyn FormField>> = Vec::new();

    for arg in &command.args {
        let chars: Vec<char> = arg.default.chars().collect();
        let cursor = chars.len();
        fields.push(Box::new(TextInput {
            name: if arg.required {
                format!("<{}>", arg.name)
            } else {
                format!("[{}]", arg.name)
            },
            help: arg.description.clone(),
            chars,
            cursor,
        }));
    }

    for flag in &command.flags {
        match &flag.kind {
            FlagKind::Boolean => {
                fields.push(Box::new(Toggle {
                    name: flag.name.clone(),
                    help: flag.description.clone(),
                    value: false,
                }));
            }
            FlagKind::Value { default, .. } => {
                let chars: Vec<char> = default.chars().collect();
                let cursor = chars.len();
                fields.push(Box::new(TextInput {
                    name: flag.name.clone(),
                    help: flag.description.clone(),
                    chars,
                    cursor,
                }));
            }
        }
    }

    fields
}

fn section_header(label: &str, width: u16) -> Line<'static> {
    let prefix = format!("── {} ", label);
    let remaining = (width as usize).saturating_sub(prefix.chars().count());
    Line::from(Span::styled(
        format!("{}{}", prefix, "─".repeat(remaining)),
        Style::new().fg(Color::DarkGray),
    ))
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width as usize),
        Style::new().fg(Color::DarkGray),
    ))
}

fn footer(width: u16) -> Paragraph<'static> {
    Paragraph::new(vec![
        separator_line(width),
        Line::from(vec![
            Span::styled(
                "↹",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" next", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "space",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" toggle", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "esc",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" back", Style::new().fg(Color::DarkGray)),
        ]),
    ])
}
