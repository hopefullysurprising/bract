use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use helptext_parser::{InputFormat, SpecCommand};
use serde::Deserialize;

use crate::data::node::{Children, Node, NodeKind};

use super::{
    classify, convert_args, convert_flags, fingerprint, help_cache, is_executable, usage_source,
    HelpProvider, Loaded, Source,
};

pub struct MiseHelpProvider;

impl HelpProvider for MiseHelpProvider {
    fn fetch_help(
        &self,
        binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut args = vec!["exec", "--", binary];
        args.extend_from_slice(subcommand_path);
        args.push("--help");

        let output = std::process::Command::new("mise").args(&args).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("help failed: {stderr}").into());
        }

        Ok(String::from_utf8(output.stdout)?)
    }
}

#[derive(Deserialize)]
struct MiseToolVersion {
    version: String,
    #[allow(dead_code)]
    install_path: String,
    active: bool,
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

/// Enumerate active mise tools and classify each executable by framework. This
/// performs no `--help` calls — those happen lazily as the tree is navigated —
/// so startup stays instant even with a tool as large as `az`.
pub fn discover_sources() -> Vec<Box<dyn Source>> {
    let output = match std::process::Command::new("mise").args(["ls", "--json"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let tools: BTreeMap<String, Vec<MiseToolVersion>> = match serde_json::from_slice(&output.stdout)
    {
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
            let cache_dir = help_cache::default_cache_dir();

            let sources: Vec<Box<dyn Source>> = executables
                .into_iter()
                .filter_map(|binary_path| {
                    let binary = binary_path.file_name()?.to_str()?.to_string();
                    // Curated usage-lib CLIs: built on mise's own spec framework,
                    // not auto-detectable from the binary, so recognized by name.
                    if binary == "usage" {
                        return Some(Box::new(usage_source::UsageSpecSource::for_tool(&binary))
                            as Box<dyn Source>);
                    }
                    let (format, program) = classify::program_and_format(&binary_path)?;
                    // Cache `--help` keyed by the program's own bytes, so repeat
                    // launches skip the subprocess while a replaced tool is re-read.
                    // Mise's version can't serve here: it describes the tool, and a
                    // bin dir may hold binaries mise never installed. Without a
                    // fingerprint we decline to cache rather than key on something
                    // unverified.
                    let provider: Box<dyn HelpProvider> =
                        match (&cache_dir, fingerprint::of(&program)) {
                            (Some(dir), Some(fingerprint)) => {
                                Box::new(help_cache::CachingHelpProvider::new(
                                    Box::new(MiseHelpProvider),
                                    dir.clone(),
                                    fingerprint,
                                ))
                            }
                            _ => Box::new(MiseHelpProvider),
                        };
                    // Scoped by the mise tool that led here: the same binary name
                    // can be reached through two tools whose bin dirs differ.
                    let tool_id = format!("{key}::{binary}");
                    Some(Box::new(HelpToolSource::with_tool_id(tool_id, binary, format, provider))
                        as Box<dyn Source>)
                })
                .collect();
            Some(sources)
        })
        .flatten()
        .collect()
}

/// A tool whose command tree is discovered by parsing its `--help` output, one
/// level at a time. Works for any framework the helptext parser understands
/// (Cobra, Knack, …) — the framework only changes how `--help` is parsed and how
/// a child's expandability is inferred.
pub struct HelpToolSource {
    tool_id: String,
    binary: String,
    format: InputFormat,
    help_provider: Box<dyn HelpProvider>,
}

impl HelpToolSource {
    /// Identified by its binary name — correct wherever a binary is reached once.
    pub fn new(binary: String, format: InputFormat, help_provider: Box<dyn HelpProvider>) -> Self {
        Self::with_tool_id(binary.clone(), binary, format, help_provider)
    }

    /// Identified explicitly, for a binary reachable through more than one mise
    /// tool — `cargo-sweep` is installed by the `cargo:` backend and again sits in
    /// the shared `~/.cargo/bin` that `rust` exposes. The id keys both the loader's
    /// source table and every node in the tree, so two tools sharing one would
    /// collide: one copy would never load, and the other would respin forever.
    pub fn with_tool_id(
        tool_id: String,
        binary: String,
        format: InputFormat,
        help_provider: Box<dyn HelpProvider>,
    ) -> Self {
        Self { tool_id, binary, format, help_provider }
    }

    fn child_node(&self, command_path: &[String], name: &str, cmd: &SpecCommand) -> Node {
        let mut child_path = command_path.to_vec();
        child_path.push(name.to_string());

        // Knack help declares whether a child needs a subcommand, so we know its
        // expandability up front. Cobra help does not, so it stays Unknown until
        // the child is loaded.
        let (kind, runnable) = match self.format {
            InputFormat::KnackHelptext => {
                if cmd.subcommand_required {
                    (NodeKind::Branch, false)
                } else {
                    (NodeKind::Leaf, true)
                }
            }
            _ => (NodeKind::Unknown, true),
        };

        Node {
            id: format!("{}/{}", self.tool_id, child_path.join("/")),
            name: name.to_string(),
            description: cmd.help.clone().unwrap_or_default(),
            kind,
            runnable,
            flags: vec![],
            args: vec![],
            tool_id: self.tool_id.clone(),
            command_path: child_path,
            children: Children::Unloaded,
        }
    }
}

impl Source for HelpToolSource {
    fn tool_id(&self) -> &str {
        &self.tool_id
    }

    fn tool_name(&self) -> &str {
        &self.binary
    }

    fn tool_bin(&self) -> Vec<String> {
        vec![self.binary.clone()]
    }

    fn cached(&self, command_path: &[String]) -> bool {
        let path_refs: Vec<&str> = command_path.iter().map(String::as_str).collect();
        self.help_provider.is_cached(&self.binary, &path_refs)
    }

    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>> {
        let path_refs: Vec<&str> = command_path.iter().map(String::as_str).collect();
        let content = self.help_provider.fetch_help(&self.binary, &path_refs)?;
        let spec = helptext_parser::parse(self.format, &content)?;

        let children = spec
            .cmd
            .subcommands
            .iter()
            .map(|(name, cmd)| self.child_node(command_path, name, cmd))
            .collect();

        Ok(Loaded {
            description: spec.cmd.help.clone().unwrap_or_default(),
            // A command that requires a subcommand (a pure group) is not runnable;
            // anything else — a leaf, or a group with its own run form — is.
            runnable: !spec.cmd.subcommand_required,
            flags: convert_flags(&spec.cmd.flags),
            args: convert_args(&spec.cmd.args),
            children,
        })
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    struct NoHelp;
    impl HelpProvider for NoHelp {
        fn fetch_help(&self, _b: &str, _p: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
            Err("not needed: identity is decided before any help is fetched".into())
        }
    }

    // `cargo-sweep` arrives twice — from the `cargo:` backend, and again from the
    // shared `~/.cargo/bin` that `rust` exposes. The id keys the loader's source
    // table and every tree node, so one shared id left the second copy permanently
    // unreachable while the first respun forever.
    #[test]
    fn two_tools_sharing_a_binary_name_stay_distinct() {
        let build = |tool_id: &str| {
            HelpToolSource::with_tool_id(
                tool_id.to_string(),
                "cargo-sweep".to_string(),
                InputFormat::ClapHelptext,
                Box::new(NoHelp),
            )
        };
        let backend = build("cargo:cargo-sweep::cargo-sweep");
        let shared_dir = build("rust::cargo-sweep");

        assert_ne!(backend.tool_id(), shared_dir.tool_id());

        // Identity is all that differs: both are still the same tool to the user,
        // shown under one name and run by one command.
        assert_eq!(backend.tool_name(), shared_dir.tool_name());
        assert_eq!(backend.tool_bin(), shared_dir.tool_bin());

        let cmd = SpecCommand::default();
        let from_backend = backend.child_node(&[], "sweep", &cmd);
        let from_shared_dir = shared_dir.child_node(&[], "sweep", &cmd);
        assert_ne!(from_backend.id, from_shared_dir.id, "child nodes collide too");
        assert_ne!(from_backend.tool_id, from_shared_dir.tool_id);
    }
}

