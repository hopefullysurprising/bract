use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding};
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use super::View;
use crate::data::commands::{Command, Tool};

pub struct BrowseView {
    tree_state: TreeState<String>,
    items: Vec<TreeItem<'static, String>>,
}

impl BrowseView {
    pub fn new(tools: &[Tool]) -> Result<Self, std::io::Error> {
        let items = build_items(tools)?;

        let mut tree_state = TreeState::default();
        if let Some(first) = items.first() {
            tree_state.select(vec![first.identifier().clone()]);
        }

        Ok(Self {
            tree_state,
            items,
        })
    }
}

impl View for BrowseView {
    fn render(&mut self, frame: &mut Frame) {
        let Ok(tree) = Tree::new(&self.items) else {
            return;
        };

        let tree = tree
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title(" Commands ")
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::new().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▸ ")
            .node_closed_symbol("▶ ")
            .node_open_symbol("▼ ");

        frame.render_stateful_widget(tree, frame.area(), &mut self.tree_state);
    }

    fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Down | KeyCode::Char('j') => { self.tree_state.key_down(); }
            KeyCode::Up | KeyCode::Char('k') => { self.tree_state.key_up(); }
            KeyCode::Right | KeyCode::Char('l') => { self.tree_state.key_right(); }
            KeyCode::Left | KeyCode::Char('h') => {
                let selected = self.tree_state.selected();
                let is_open = self.tree_state.opened().contains(selected);
                if is_open || selected.len() > 1 {
                    self.tree_state.key_left();
                }
            }
            KeyCode::Enter => { self.tree_state.toggle_selected(); }
            KeyCode::Home => { self.tree_state.select_first(); }
            KeyCode::End => { self.tree_state.select_last(); }
            _ => {}
        }
    }
}

fn build_items(tools: &[Tool]) -> Result<Vec<TreeItem<'static, String>>, std::io::Error> {
    tools.iter().map(|tool| {
        let children = build_command_items(&tool.commands)?;
        let text = if tool.description.is_empty() {
            Line::from(Span::styled(tool.name.clone(), Style::new().fg(Color::Cyan).bold()))
        } else {
            Line::from(vec![
                Span::styled(tool.name.clone(), Style::new().fg(Color::Cyan).bold()),
                Span::raw("  "),
                Span::styled(tool.description.clone(), Style::new().fg(Color::Gray)),
            ])
        };
        TreeItem::new(tool.id.clone(), text, children)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }).collect()
}

fn build_command_items(commands: &[Command]) -> Result<Vec<TreeItem<'static, String>>, std::io::Error> {
    commands.iter().map(|cmd| {
        let text = Line::from(vec![
            Span::styled(cmd.name.clone(), Style::new().bold()),
            Span::raw("  "),
            Span::styled(cmd.description.clone(), Style::new().fg(Color::Gray)),
        ]);

        if cmd.subcommands.is_empty() {
            Ok(TreeItem::new_leaf(cmd.id.clone(), text))
        } else {
            let children = build_command_items(&cmd.subcommands)?;
            TreeItem::new(cmd.id.clone(), text, children)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }
    }).collect()
}
