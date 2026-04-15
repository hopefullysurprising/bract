use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use helptext_parser::InputFormat;
use serde::Deserialize;

use crate::data::commands::Command;

use super::go_buildinfo;
use super::{convert_args, convert_flags, DiscoveryResult, Source};

#[derive(Deserialize)]
struct MiseToolVersion {
    version: String,
    #[allow(dead_code)]
    install_path: String,
    active: bool,
}

fn classify_binary(binary_path: &Path) -> Option<InputFormat> {
    let deps = go_buildinfo::read_deps(binary_path)?;
    if deps.iter().any(|d| d.path == "github.com/spf13/cobra") {
        return Some(InputFormat::CobraHelptext);
    }
    None
}

fn resolve_bin_paths(tool_key: &str, version: &str) -> Option<PathBuf> {
    let tool_version = format!("{tool_key}@{version}");
    let output = std::process::Command::new("mise")
        .args(["bin-paths", &tool_version])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.lines().next().map(PathBuf::from)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext == "exe")
        .unwrap_or(false)
}

fn list_executables(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file() && is_executable(&e.path()))
        .map(|e| e.path())
        .collect()
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
        .flat_map(|(key, versions)| {
            let active = versions.into_iter().find(|v| v.active)?;
            let bin_dir = resolve_bin_paths(&key, &active.version)?;
            let executables = list_executables(&bin_dir);

            let sources: Vec<Box<dyn Source>> = executables
                .into_iter()
                .filter_map(|binary_path| {
                    let binary = binary_path.file_name()?.to_str()?.to_string();
                    let format = classify_binary(&binary_path)?;
                    Some(Box::new(MiseToolSource {
                        binary: binary.clone(),
                        name: binary,
                        format,
                    }) as Box<dyn Source>)
                })
                .collect();
            Some(sources)
        })
        .flatten()
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
