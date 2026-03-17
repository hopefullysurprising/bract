use std::collections::BTreeMap;

use helptext_parser::{InputFormat, Spec};

use crate::data::commands::Command;

use super::Source;

pub struct MiseTasksSource;

impl Source for MiseTasksSource {
    fn tool_id(&self) -> &str {
        "mise_tasks"
    }

    fn tool_name(&self) -> &str {
        "Mise Tasks"
    }

    fn discover(&self) -> Result<Vec<Command>, Box<dyn std::error::Error>> {
        let output = std::process::Command::new("mise")
            .args(["tasks", "--usage"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("mise tasks --usage failed: {stderr}").into());
        }

        let content = String::from_utf8(output.stdout)?;
        let spec = helptext_parser::parse(InputFormat::UsageKdl, &content)?;

        Ok(commands_from_spec(&spec, "mise"))
    }
}

fn commands_from_spec(spec: &Spec, tool_id: &str) -> Vec<Command> {
    let entries: Vec<(&str, &str)> = spec
        .cmd
        .subcommands
        .iter()
        .map(|(name, cmd)| (name.as_str(), cmd.help.as_deref().unwrap_or_default()))
        .collect();

    build_hierarchy(tool_id, &entries)
}

fn build_hierarchy(prefix: &str, entries: &[(&str, &str)]) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut groups: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();

    for &(name, help) in entries {
        match name.split_once(':') {
            Some((group, rest)) => {
                groups.entry(group).or_default().push((rest, help));
            }
            None => {
                commands.push(Command {
                    id: format!("{prefix}:{name}"),
                    name: name.to_string(),
                    description: help.to_string(),
                    subcommands: vec![],
                });
            }
        }
    }

    for (group, children) in groups {
        let group_prefix = format!("{prefix}:{group}");
        let subcommands = build_hierarchy(&group_prefix, &children);
        commands.push(Command {
            id: group_prefix,
            name: group.to_string(),
            description: String::new(),
            subcommands,
        });
    }

    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}
