use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding};
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use super::form::FormView;
use super::{View, ViewAction};
use crate::data::commands::{Command, Tool};

struct ToolMeta {
    bin: String,
    path_separator: String,
}

pub struct BrowseView {
    tree_state: TreeState<String>,
    items: Vec<TreeItem<'static, String>>,
    commands: HashMap<String, Command>,
    tool_meta: HashMap<String, ToolMeta>,
}

impl BrowseView {
    pub fn new(tools: &[Tool]) -> Result<Self, std::io::Error> {
        let items = build_items(tools)?;
        let commands = build_command_map(tools);
        let tool_meta = tools.iter().map(|t| {
            (t.id.clone(), ToolMeta {
                bin: t.bin.clone(),
                path_separator: t.path_separator.clone(),
            })
        }).collect();

        let mut tree_state = TreeState::default();
        if let Some(first) = items.first() {
            tree_state.select(vec![first.identifier().clone()]);
        }

        Ok(Self {
            tree_state,
            items,
            commands,
            tool_meta,
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

    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.tree_state.key_down();
                Some(ViewAction::Consumed)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.tree_state.key_up();
                Some(ViewAction::Consumed)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.tree_state.key_right();
                Some(ViewAction::Consumed)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let selected = self.tree_state.selected();
                let is_open = self.tree_state.opened().contains(selected);
                if is_open || selected.len() > 1 {
                    self.tree_state.key_left();
                }
                Some(ViewAction::Consumed)
            }
            KeyCode::Enter => {
                let selected = self.tree_state.selected().to_vec();
                if let Some(id) = selected.last() {
                    if let Some(cmd) = self.commands.get(id) {
                        if cmd.subcommands.is_empty() {
                            let tool_id = selected.first().unwrap();
                            let meta = self.tool_meta.get(tool_id);
                            let bin = meta.map(|m| m.bin.as_str()).unwrap_or("");
                            let sep = meta.map(|m| m.path_separator.as_str()).unwrap_or(" ");
                            let ancestors: Vec<Command> = selected.iter()
                                .filter_map(|ancestor_id| self.commands.get(ancestor_id))
                                .cloned()
                                .collect();
                            return Some(ViewAction::Push(
                                Box::new(FormView::new(ancestors, bin, sep)),
                            ));
                        }
                    }
                }
                self.tree_state.toggle_selected();
                Some(ViewAction::Consumed)
            }
            KeyCode::Home => {
                self.tree_state.select_first();
                Some(ViewAction::Consumed)
            }
            KeyCode::End => {
                self.tree_state.select_last();
                Some(ViewAction::Consumed)
            }
            _ => None,
        }
    }
}

fn build_command_map(tools: &[Tool]) -> HashMap<String, Command> {
    let mut map = HashMap::new();
    for tool in tools {
        map.insert(tool.id.clone(), Command {
            id: tool.id.clone(),
            name: tool.name.clone(),
            description: tool.description.clone(),
            flags: tool.flags.clone(),
            args: tool.args.clone(),
            subcommands: vec![],
        });
        collect_commands(&tool.commands, &mut map);
    }
    map
}

fn collect_commands(commands: &[Command], map: &mut HashMap<String, Command>) {
    for cmd in commands {
        map.insert(cmd.id.clone(), cmd.clone());
        collect_commands(&cmd.subcommands, map);
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
