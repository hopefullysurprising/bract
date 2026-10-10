use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helptext_parser::{InputFormat, SpecCommand};
use serde::Deserialize;

use crate::data::node::{Children, Node, NodeKind};

use super::bounded::{output_within, HELP_LIMIT};
use super::node_introspect::OclifPackage;
use super::usage_source::{CommandSpecProvider, SpecProvider, UsageSpecSource};
use super::{
    classify, convert_args, convert_flags, fingerprint, help_cache, is_executable, usage_source,
    HelpProvider, Loaded, Source, OCLIF_COLUMNS,
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

        let mut command = std::process::Command::new("mise");
        command.args(&args).env(OCLIF_COLUMNS.0, OCLIF_COLUMNS.1);
        super::help_from_output(output_within(&mut command, HELP_LIMIT)?)
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
                    let runner = vec!["mise".into(), "exec".into(), "--".into(), binary.clone()];
                    Some(Box::new(HelpToolSource::for_program(tool_id, binary, format, &program, provider, runner))
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
    separator: String,
    whole_tree: Option<(InputFormat, Arc<dyn SpecProvider>)>,
    /// Each page read this session, by command path, as a hash: what a child's page is
    /// compared against.
    pages: Mutex<HashMap<Vec<String>, u64>>,
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
        Self {
            tool_id,
            binary,
            format,
            help_provider,
            separator: " ".into(),
            whole_tree: None,
            pages: Mutex::new(HashMap::new()),
        }
    }

    /// The source for `program`, with what its framework's package adds: oclif's
    /// separator, and `commands --json` where the CLI ships it. `runner` invokes
    /// the binary — under `mise exec`, or as found on PATH.
    pub(crate) fn for_program(
        tool_id: String,
        binary: String,
        format: InputFormat,
        program: &Path,
        help_provider: Box<dyn HelpProvider>,
        runner: Vec<String>,
    ) -> Self {
        let source = Self::with_tool_id(tool_id, binary, format, help_provider);
        let Some(package) = (format == InputFormat::OclifHelptext).then(|| OclifPackage::read(program)).flatten() else {
            return source;
        };
        let source = source.with_separator(&package.separator);
        if !package.lists_commands {
            return source;
        }
        let mut command = runner;
        command.extend(["commands".into(), "--json".into()]);
        source.with_whole_tree(InputFormat::OclifCommandsJson, Arc::new(CommandSpecProvider::new(command)))
    }

    /// How command names join when typed: `sf org list`, but `heroku apps:create`.
    pub fn with_separator(mut self, separator: &str) -> Self {
        self.separator = separator.into();
        self
    }

    pub fn with_whole_tree(mut self, format: InputFormat, provider: Arc<dyn SpecProvider>) -> Self {
        self.whole_tree = Some((format, provider));
        self
    }

    /// The arguments naming the command at `command_path`, the way the form joins
    /// them to run it.
    fn invocation(&self, command_path: &[String]) -> Vec<String> {
        command_path.join(&self.separator).split_whitespace().map(String::from).collect()
    }

    fn child_node(&self, command_path: &[String], name: &str, cmd: &SpecCommand) -> Node {
        let mut child_path = command_path.to_vec();
        child_path.push(name.to_string());

        // Knack and oclif help say which children are pure groups, and Knack also
        // which are leaves, so their expandability is known up front. The rest
        // stay Unknown until loaded.
        let (kind, runnable) = match self.format {
            InputFormat::KnackHelptext | InputFormat::OclifHelptext if cmd.subcommand_required => {
                (NodeKind::Branch, false)
            }
            InputFormat::KnackHelptext => (NodeKind::Leaf, true),
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

    fn tool_path_separator(&self) -> &str {
        &self.separator
    }

    fn cached(&self, command_path: &[String]) -> bool {
        let invocation = self.invocation(command_path);
        let path_refs: Vec<&str> = invocation.iter().map(String::as_str).collect();
        self.help_provider.is_cached(&self.binary, &path_refs)
    }

    fn whole_tree(&self) -> Option<Box<dyn Source>> {
        let (format, provider) = self.whole_tree.clone()?;
        Some(Box::new(UsageSpecSource::whole_tree(
            &self.tool_id,
            &self.binary,
            self.tool_bin(),
            &self.separator,
            format,
            provider,
        )))
    }

    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>> {
        let invocation = self.invocation(command_path);
        let path_refs: Vec<&str> = invocation.iter().map(String::as_str).collect();
        let content = self.help_provider.fetch_help(&self.binary, &path_refs)?;
        let spec = helptext_parser::parse(self.format, &content)?;

        // A child answered with its parent's page, byte for byte, is not a page about the
        // child, and taking its children grows the tree again beneath it, level after
        // level: oclif's `help` reprints the root, and Cobra answers a child it does not
        // have — a misread flag, a line of prose — with the parent's own page. A plugin
        // reached through its host (`kubectl ai`) prints a page of its own and passes.
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let page = hasher.finish();
        let mut pages = self.pages.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let echoes_parent = command_path.split_last().is_some_and(|(_, parent)| pages.get(parent) == Some(&page));
        pages.insert(command_path.to_vec(), page);
        drop(pages);
        if echoes_parent {
            return Ok(Loaded { description: String::new(), runnable: true, flags: vec![], args: vec![], children: vec![], notice: None });
        }

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
            notice: None,
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

#[cfg(test)]
mod page_guard_tests {
    use super::*;

    /// Serves real captured pages by command path.
    struct Pages(Vec<(Vec<&'static str>, &'static str)>);
    impl HelpProvider for Pages {
        fn fetch_help(&self, _b: &str, path: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
            let (_, file) = self.0.iter().find(|(p, _)| p.as_slice() == path).ok_or("no page captured")?;
            Ok(std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli-help").join(file))?)
        }
    }

    fn path(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    // `kluctl gitops --gops-agent --help` prints `gitops`'s own page, byte for byte.
    // Taken as the page of `--gops-agent`, its eight subcommands grew beneath every
    // misread child, level after level, and a `--spec` walk never ended.
    #[test]
    fn a_child_answered_with_its_parents_page_holds_no_children() {
        let pages = Pages(vec![
            (vec!["gitops"], "kluctl_2.27.0_gitops.txt"),
            (vec!["gitops", "--gops-agent"], "kluctl_2.27.0_gitops_--gops-agent.txt"),
        ]);
        let source = HelpToolSource::new("kluctl".into(), InputFormat::CobraHelptext, Box::new(pages));

        assert_eq!(source.load(&path(&["gitops"])).unwrap().children.len(), 8);
        assert!(source.load(&path(&["gitops", "--gops-agent"])).unwrap().children.is_empty());
    }

    // `kubectl ai` runs the kubectl-ai plugin (0.0.20 here), whose page names its own
    // binary — `kubectl-ai [command]` — and lists its own subcommands. It is not its
    // host's page, and keeps them.
    #[test]
    fn a_plugin_reached_through_its_host_keeps_its_own_page() {
        let pages = Pages(vec![(vec![], "kubectl_1.35.3_root.txt"), (vec!["ai"], "kubectl_1.35.3_ai.txt")]);
        let source = HelpToolSource::new("kubectl".into(), InputFormat::CobraHelptext, Box::new(pages));

        source.load(&[]).unwrap();
        let ai = source.load(&path(&["ai"])).unwrap();
        assert!(ai.children.iter().any(|c| c.name == "completion"), "{:?}", ai.children.iter().map(|c| &c.name).collect::<Vec<_>>());
    }
}

