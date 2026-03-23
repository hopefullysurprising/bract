use std::collections::BTreeMap;

use helptext_parser::InputFormat;
use serde::Deserialize;

use crate::data::commands::Command;

use super::{convert_args, convert_flags, DiscoveryResult, Source};

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

    fn resolve_command(&self, id: &str, name: &str, description: &str, subcommand_path: &[&str]) -> Command {
        let (flags, args, subcommands) = match self.run_help(subcommand_path) {
            Ok(spec) => {
                let children = spec.cmd.subcommands.iter().map(|(child_name, child_cmd)| {
                    let child_id = format!("{id}/{child_name}");
                    let mut child_path = subcommand_path.to_vec();
                    child_path.push(child_name.as_str());
                    self.resolve_command(
                        &child_id,
                        child_name,
                        child_cmd.help.as_deref().unwrap_or_default(),
                        &child_path,
                    )
                }).collect();
                (convert_flags(&spec.cmd.flags), convert_args(&spec.cmd.args), children)
            }
            Err(_) => (vec![], vec![], vec![]),
        };

        Command {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            flags,
            args,
            subcommands,
        }
    }
}

impl Source for MiseToolSource {
    fn tool_id(&self) -> &str {
        &self.binary
    }

    fn tool_name(&self) -> &str {
        &self.name
    }

    fn tool_bin(&self) -> Vec<String> {
        vec![self.binary.clone()]
    }

    fn discover(&self) -> Result<DiscoveryResult, Box<dyn std::error::Error>> {
        let spec = match self.run_help(&[]) {
            Ok(s) => s,
            Err(_) => {
                return Ok(DiscoveryResult {
                    description: "failed to get help".to_string(),
                    flags: vec![],
                    args: vec![],
                    commands: vec![],
                });
            }
        };

        let description = spec.cmd.help.clone().unwrap_or_default();
        let flags = convert_flags(&spec.cmd.flags);
        let args = convert_args(&spec.cmd.args);
        let commands = spec.cmd
            .subcommands
            .iter()
            .map(|(name, cmd)| {
                self.resolve_command(
                    &format!("{}/{name}", self.binary),
                    name,
                    cmd.help.as_deref().unwrap_or_default(),
                    &[name.as_str()],
                )
            })
            .collect();

        Ok(DiscoveryResult {
            description,
            flags,
            args,
            commands,
        })
    }
}
