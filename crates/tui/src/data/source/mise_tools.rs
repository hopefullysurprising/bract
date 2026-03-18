use std::collections::BTreeMap;

use helptext_parser::InputFormat;
use serde::Deserialize;

use crate::data::commands::Command;

use super::{DiscoveryResult, Source};

#[derive(Deserialize)]
struct MiseToolVersion {
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    install_path: String,
    active: bool,
}

fn classify_tool(tool_name: &str) -> Option<InputFormat> {
    let binary = tool_name.rsplit('/').last().unwrap_or(tool_name);
    match binary {
        "mani" => Some(InputFormat::CobraHelptext),
        _ => None,
    }
}

fn binary_name(tool_key: &str) -> &str {
    tool_key.rsplit('/').last().unwrap_or(tool_key)
}

fn display_name(binary: &str) -> String {
    binary.to_string()
}

pub fn discover_sources() -> Vec<Box<dyn Source>> {
    let output = match std::process::Command::new("mise")
        .args(["ls", "--json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let tools: BTreeMap<String, Vec<MiseToolVersion>> =
        match serde_json::from_slice(&output.stdout) {
            Ok(t) => t,
            Err(_) => return vec![],
        };

    tools
        .into_iter()
        .filter(|(_, versions)| versions.iter().any(|v| v.active))
        .filter_map(|(key, _)| {
            let format = classify_tool(&key)?;
            let binary = binary_name(&key).to_string();
            Some(Box::new(MiseToolSource {
                binary: binary.clone(),
                name: display_name(&binary),
                format,
            }) as Box<dyn Source>)
        })
        .collect()
}

struct MiseToolSource {
    binary: String,
    name: String,
    format: InputFormat,
}

impl MiseToolSource {
    fn run_help(&self, subcommand_path: &[&str]) -> Result<helptext_parser::Spec, Box<dyn std::error::Error>> {
        let mut args = vec!["exec", "--", self.binary.as_str()];
        args.extend_from_slice(subcommand_path);
        args.push("--help");

        let output = std::process::Command::new("mise")
            .args(&args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("help failed: {stderr}").into());
        }

        let content = String::from_utf8(output.stdout)?;
        Ok(helptext_parser::parse(self.format, &content)?)
    }

    fn resolve_tree(&self, id_prefix: &str, subcommand_path: &[&str]) -> Vec<Command> {
        let spec = match self.run_help(subcommand_path) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        spec.cmd
            .subcommands
            .iter()
            .map(|(name, cmd)| {
                let id = format!("{id_prefix}/{name}");
                let mut child_path = subcommand_path.to_vec();
                child_path.push(name.as_str());
                let subcommands = self.resolve_tree(&id, &child_path);

                Command {
                    id,
                    name: name.clone(),
                    description: cmd.help.as_deref().unwrap_or_default().to_string(),
                    subcommands,
                }
            })
            .collect()
    }
}

impl Source for MiseToolSource {
    fn tool_id(&self) -> &str {
        &self.binary
    }

    fn tool_name(&self) -> &str {
        &self.name
    }

    fn discover(&self) -> Result<DiscoveryResult, Box<dyn std::error::Error>> {
        let spec = match self.run_help(&[]) {
            Ok(s) => s,
            Err(_) => {
                return Ok(DiscoveryResult {
                    description: "failed to get help".to_string(),
                    commands: vec![],
                });
            }
        };

        let description = spec.cmd.help.clone().unwrap_or_default();
        let commands = spec.cmd
            .subcommands
            .iter()
            .map(|(name, cmd)| {
                let id = format!("{}/{name}", self.binary);
                let subcommands = self.resolve_tree(&id, &[name.as_str()]);

                Command {
                    id,
                    name: name.clone(),
                    description: cmd.help.as_deref().unwrap_or_default().to_string(),
                    subcommands,
                }
            })
            .collect();

        Ok(DiscoveryResult {
            description,
            commands,
        })
    }
}
