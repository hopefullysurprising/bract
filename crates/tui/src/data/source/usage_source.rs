//! Sources for tools that expose a complete [Usage](https://usage.jdx.dev) spec
//! in one dump: mise itself (`mise usage`) and usage-lib CLIs (`<bin>
//! --usage-spec`). Unlike Cobra/Knack — where each `--help` reveals one level —
//! a usage spec carries the whole nested command tree, so the tree is built
//! eagerly from a single parse (the same shape as Mise Tasks).

use helptext_parser::{InputFormat, Spec, SpecCommand};

use crate::data::node::{Children, Node, NodeKind};

use super::{convert_args, convert_flags, Loaded, Source};

/// Supplies a tool's Usage KDL spec. Abstracted so tests inject a fixture
/// instead of spawning the real tool.
pub trait SpecProvider: Send + Sync {
    fn fetch_spec(&self) -> Result<String, Box<dyn std::error::Error>>;
}

/// Runs a command and returns its stdout as the spec.
pub struct CommandSpecProvider {
    command: Vec<String>,
}

impl SpecProvider for CommandSpecProvider {
    fn fetch_spec(&self) -> Result<String, Box<dyn std::error::Error>> {
        let (program, args) = self.command.split_first().ok_or("empty spec command")?;
        let output = std::process::Command::new(program).args(args).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{} failed: {stderr}", self.command.join(" ")).into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

/// A tool whose entire command tree comes from one Usage KDL dump.
pub struct UsageSpecSource {
    tool_id: String,
    tool_name: String,
    bin: Vec<String>,
    separator: String,
    provider: Box<dyn SpecProvider>,
}

impl UsageSpecSource {
    /// Mise itself, via `mise usage` — pinned right under Mise Tasks.
    pub fn mise_self() -> Self {
        Self::from_command(
            "mise",
            "Mise",
            vec!["mise".into()],
            vec!["mise".into(), "usage".into()],
        )
    }

    /// A usage-lib CLI found among mise tools, via `<bin> --usage-spec` (wrapped
    /// in `mise exec` so the active version is used).
    pub fn for_tool(binary: &str) -> Self {
        Self::from_command(
            binary,
            binary,
            vec![binary.into()],
            vec!["mise".into(), "exec".into(), "--".into(), binary.into(), "--usage-spec".into()],
        )
    }

    fn from_command(tool_id: &str, tool_name: &str, bin: Vec<String>, command: Vec<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            tool_name: tool_name.into(),
            bin,
            separator: " ".into(),
            provider: Box::new(CommandSpecProvider { command }),
        }
    }

    #[cfg(test)]
    fn with_provider(tool_id: &str, provider: Box<dyn SpecProvider>) -> Self {
        Self {
            tool_id: tool_id.into(),
            tool_name: tool_id.into(),
            bin: vec![tool_id.into()],
            separator: " ".into(),
            provider,
        }
    }
}

impl Source for UsageSpecSource {
    fn tool_id(&self) -> &str {
        &self.tool_id
    }
    fn tool_name(&self) -> &str {
        &self.tool_name
    }
    fn tool_bin(&self) -> Vec<String> {
        self.bin.clone()
    }
    fn tool_path_separator(&self) -> &str {
        &self.separator
    }

    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>> {
        // One spec dump yields the whole tree, returned fully built at the root;
        // deeper paths are already `Loaded` in memory, so they're never fetched.
        if !command_path.is_empty() {
            return Ok(Loaded {
                description: String::new(),
                runnable: false,
                flags: vec![],
                args: vec![],
                children: vec![],
            });
        }
        let content = self.provider.fetch_spec()?;
        let spec = helptext_parser::parse(InputFormat::UsageKdl, &content)?;
        Ok(Loaded {
            description: spec.cmd.help.clone().unwrap_or_default(),
            runnable: false,
            flags: convert_flags(&spec.cmd.flags),
            args: convert_args(&spec.cmd.args),
            children: nodes_from_nested_spec(&spec, &self.tool_id),
        })
    }
}

/// Build the full node tree by recursing the spec's nested subcommands. (Unlike
/// `nodes_from_spec`, which splits mise's colon-joined task names, usage specs
/// nest subcommands directly.)
pub fn nodes_from_nested_spec(spec: &Spec, tool_id: &str) -> Vec<Node> {
    build_nested(tool_id, &[], &spec.cmd)
}

fn build_nested(tool_id: &str, prefix: &[String], cmd: &SpecCommand) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (name, sub) in &cmd.subcommands {
        let mut segments = prefix.to_vec();
        segments.push(name.clone());
        let children = build_nested(tool_id, &segments, sub);
        let kind = if children.is_empty() { NodeKind::Leaf } else { NodeKind::Branch };
        nodes.push(Node {
            id: format!("{tool_id}/{}", segments.join("/")),
            name: name.clone(),
            description: sub.help.clone().unwrap_or_default(),
            kind,
            // A command that requires a subcommand is a pure group; a leaf or a
            // parent with its own run form stays runnable.
            runnable: !sub.subcommand_required,
            flags: convert_flags(&sub.flags),
            args: convert_args(&sub.args),
            tool_id: tool_id.to_string(),
            command_path: segments,
            children: Children::Loaded(children),
        });
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct StaticSpecProvider(String);
    impl SpecProvider for StaticSpecProvider {
        fn fetch_spec(&self) -> Result<String, Box<dyn std::error::Error>> {
            Ok(self.0.clone())
        }
    }

    fn usage_fixture() -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/usage-kdl/usage_3.5.0.kdl");
        std::fs::read_to_string(path).unwrap()
    }

    fn find<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
        nodes.iter().find(|n| n.name == name)
    }

    #[test]
    fn builds_the_full_nested_tree_from_one_spec() {
        let src = UsageSpecSource::with_provider("usage", Box::new(StaticSpecProvider(usage_fixture())));
        let loaded = src.load(&[]).unwrap();

        // `generate` is a branch with its own subcommands…
        let generate = find(&loaded.children, "generate").expect("generate present");
        assert!(matches!(generate.kind, NodeKind::Branch), "generate is a group");
        let Children::Loaded(gen_children) = &generate.children else {
            panic!("generate's subtree is loaded eagerly");
        };
        assert!(find(gen_children, "completion").is_some(), "generate has a 'completion' child");

        // …while `lint` is a runnable leaf.
        let lint = find(&loaded.children, "lint").expect("lint present");
        assert!(matches!(lint.kind, NodeKind::Leaf), "lint is a leaf");
        assert!(lint.runnable, "a leaf command is runnable");
    }

    #[test]
    fn nested_command_paths_are_fully_qualified() {
        let src = UsageSpecSource::with_provider("usage", Box::new(StaticSpecProvider(usage_fixture())));
        let loaded = src.load(&[]).unwrap();
        let generate = find(&loaded.children, "generate").unwrap();
        let Children::Loaded(gen_children) = &generate.children else { unreachable!() };
        let completion = find(gen_children, "completion").unwrap();
        assert_eq!(completion.command_path, vec!["generate", "completion"]);
        assert_eq!(completion.tool_id, "usage");
    }

    #[test]
    fn deeper_paths_are_not_refetched() {
        // The whole tree is built at the root; a non-root load returns nothing.
        let src = UsageSpecSource::with_provider("usage", Box::new(StaticSpecProvider(usage_fixture())));
        let loaded = src.load(&["generate".to_string()]).unwrap();
        assert!(loaded.children.is_empty(), "non-root loads are no-ops; the tree is eager");
    }
}
