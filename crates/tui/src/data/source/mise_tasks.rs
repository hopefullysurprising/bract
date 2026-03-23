use std::collections::BTreeMap;

use helptext_parser::{InputFormat, Spec, SpecCommand};

use crate::data::commands::Command;

use super::{convert_args, convert_flags, DiscoveryResult, Source};

pub struct MiseTasksSource;

impl Source for MiseTasksSource {
    fn tool_id(&self) -> &str {
        "mise_tasks"
    }

    fn tool_name(&self) -> &str {
        "Mise Tasks"
    }

    fn tool_bin(&self) -> Vec<String> {
        vec!["mise".into(), "run".into()]
    }

    fn tool_path_separator(&self) -> &str {
        ":"
    }

    fn discover(&self) -> Result<DiscoveryResult, Box<dyn std::error::Error>> {
        let output = std::process::Command::new("mise")
            .args(["tasks", "--usage"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("mise tasks --usage failed: {stderr}").into());
        }

        let content = String::from_utf8(output.stdout)?;
        let spec = helptext_parser::parse(InputFormat::UsageKdl, &content)?;

        Ok(DiscoveryResult {
            description: String::new(),
            flags: vec![],
            args: vec![],
            commands: commands_from_spec(&spec, "mise"),
        })
    }
}

fn commands_from_spec(spec: &Spec, tool_id: &str) -> Vec<Command> {
    let entries: Vec<(&str, &SpecCommand)> = spec
        .cmd
        .subcommands
        .iter()
        .map(|(name, cmd)| (name.as_str(), cmd))
        .collect();

    build_hierarchy(tool_id, &entries)
}

fn build_hierarchy(prefix: &str, entries: &[(&str, &SpecCommand)]) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut groups: BTreeMap<&str, Vec<(&str, &SpecCommand)>> = BTreeMap::new();

    for &(name, spec_cmd) in entries {
        match name.split_once(':') {
            Some((group, rest)) => {
                groups.entry(group).or_default().push((rest, spec_cmd));
            }
            None => {
                commands.push(Command {
                    id: format!("{prefix}:{name}"),
                    name: name.to_string(),
                    description: spec_cmd.help.as_deref().unwrap_or_default().to_string(),
                    flags: convert_flags(&spec_cmd.flags),
                    args: convert_args(&spec_cmd.args),
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
            flags: vec![],
            args: vec![],
            subcommands,
        });
    }

    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}
