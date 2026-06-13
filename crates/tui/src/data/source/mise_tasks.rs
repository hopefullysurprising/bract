use std::collections::BTreeMap;

use helptext_parser::{InputFormat, Spec, SpecCommand};

use crate::data::node::{Children, Node, NodeKind};

use super::{convert_args, convert_flags, Loaded, Source};

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

    // Mise hands us every task in one `mise tasks --usage` call, so the whole
    // tree is built eagerly and returned fully loaded — no per-level fetches.
    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>> {
        if !command_path.is_empty() {
            return Ok(Loaded { description: String::new(), runnable: false, flags: vec![], args: vec![], children: vec![] });
        }

        let output = std::process::Command::new("mise")
            .args(["tasks", "--usage"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("mise tasks --usage failed: {stderr}").into());
        }

        let content = String::from_utf8(output.stdout)?;
        let spec = helptext_parser::parse(InputFormat::UsageKdl, &content)?;

        Ok(Loaded {
            description: String::new(),
            runnable: false,
            flags: vec![],
            args: vec![],
            children: nodes_from_spec(&spec, self.tool_id()),
        })
    }
}

pub fn nodes_from_spec(spec: &Spec, tool_id: &str) -> Vec<Node> {
    let entries: Vec<(&str, &SpecCommand)> = spec
        .cmd
        .subcommands
        .iter()
        .map(|(name, cmd)| (name.as_str(), cmd))
        .collect();

    build_hierarchy(tool_id, &[], &entries)
}

fn build_hierarchy(tool_id: &str, prefix: &[String], entries: &[(&str, &SpecCommand)]) -> Vec<Node> {
    let mut leaves: BTreeMap<&str, &SpecCommand> = BTreeMap::new();
    let mut groups: BTreeMap<&str, Vec<(&str, &SpecCommand)>> = BTreeMap::new();

    for &(name, spec_cmd) in entries {
        match name.split_once(':') {
            Some((group, rest)) => {
                groups.entry(group).or_default().push((rest, spec_cmd));
            }
            None => {
                leaves.insert(name, spec_cmd);
            }
        }
    }

    let mut nodes = Vec::new();

    for (group, children) in &groups {
        let mut segments = prefix.to_vec();
        segments.push(group.to_string());
        let subnodes = build_hierarchy(tool_id, &segments, children);

        // A name can be both a runnable task and a group parent (e.g. `app:check`
        // alongside `app:check:be`). Fold the runnable task's metadata into the
        // group node so it stays a single tree identifier instead of colliding.
        let (description, flags, args, runnable) = match leaves.remove(group) {
            Some(spec_cmd) => (
                spec_cmd.help.as_deref().unwrap_or_default().to_string(),
                convert_flags(&spec_cmd.flags),
                convert_args(&spec_cmd.args),
                true,
            ),
            None => (String::new(), vec![], vec![], false),
        };

        nodes.push(Node {
            id: node_id(tool_id, &segments),
            name: group.to_string(),
            description,
            kind: NodeKind::Branch,
            runnable,
            flags,
            args,
            tool_id: tool_id.to_string(),
            command_path: segments,
            children: Children::Loaded(subnodes),
        });
    }

    for (name, spec_cmd) in leaves {
        let mut segments = prefix.to_vec();
        segments.push(name.to_string());
        nodes.push(Node {
            id: node_id(tool_id, &segments),
            name: name.to_string(),
            description: spec_cmd.help.as_deref().unwrap_or_default().to_string(),
            kind: NodeKind::Leaf,
            runnable: true,
            flags: convert_flags(&spec_cmd.flags),
            args: convert_args(&spec_cmd.args),
            tool_id: tool_id.to_string(),
            command_path: segments,
            children: Children::Loaded(vec![]),
        });
    }

    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    nodes
}

fn node_id(tool_id: &str, segments: &[String]) -> String {
    format!("{tool_id}/{}", segments.join("/"))
}
