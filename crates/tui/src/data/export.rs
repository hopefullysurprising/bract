//! Emit a tool's whole command tree as a usage spec, for reading without the TUI.
//!
//! The tree the TUI browses lazily is walked eagerly here, so every subcommand's
//! own `--help` is fetched — bar the subtrees a source has already handed over,
//! which are taken as given rather than asked for twice. That is the cost of the
//! mode: the point is to hand a reader — often another program — one document
//! describing everything a CLI can do, rather than making it drive an interactive
//! session to find out.
//!
//! The walk runs through the shared loader pool, so those fetches overlap. It is a
//! bag of tasks: each fetch that comes back drops its children into the bag, and
//! every tool's tree grows at once rather than one after another. A parent is
//! finished as soon as its own help is parsed — it never waits for its subtree —
//! so results arrive in no particular order and are folded into the tree by path,
//! then assembled once the bag has drained.
//!
//! Usage is the output format because it is already this project's lingua franca:
//! mise emits it, `helptext-parser` produces it, and its renderer round-trips the
//! same types the parsers build.

use std::collections::{HashMap, HashSet};

use helptext_parser::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

use crate::data::loader::{BackgroundLoader, LoadRequest, Loader, Priority};
use crate::data::node::{Arg, Children, Flag, FlagKind, Node};
use crate::data::source::Source;

/// A tool that reports itself among its own subcommands would otherwise recurse
/// until the stack runs out. No real CLI nests anywhere near this deep.
const MAX_DEPTH: usize = 10;

/// What a spec needs about its tool, kept aside before the sources are handed to
/// the loader.
struct Tool {
    id: String,
    name: String,
    bin: String,
}

/// Walk every source to its leaves and render each as a usage spec.
pub fn usage_specs(sources: Vec<Box<dyn Source>>) -> Vec<Spec> {
    let tools: Vec<Tool> = sources
        .iter()
        .map(|s| Tool {
            id: s.tool_id().to_string(),
            name: s.tool_name().to_string(),
            bin: s.tool_bin().join(" "),
        })
        .collect();

    let loader = BackgroundLoader::new(sources);
    let mut walk = Walk::new(tools.len());

    for (index, tool) in tools.iter().enumerate() {
        walk.submit(&loader, index, &tool.id, Vec::new(), tool.name.clone(), 0);
    }
    while walk.outstanding > 0 {
        let Some(outcome) = loader.wait() else { break };
        walk.integrate(&loader, &tools, outcome);
    }

    tools.iter().enumerate().filter_map(|(index, tool)| walk.spec_for(index, tool)).collect()
}

/// A child as its parent listed it: the name to nest it under, and the path
/// that identifies it.
struct Child {
    name: String,
    path: Vec<String>,
}

/// A tool's children, keyed by their parent's path.
type ChildIndex = HashMap<Vec<String>, Vec<Child>>;

/// A node handed to the loader, and what the walk needs to place its result.
struct Pending {
    tool: usize,
    path: Vec<String>,
    name: String,
    depth: usize,
}

struct Walk {
    outstanding: usize,
    next_id: usize,
    pending: HashMap<String, Pending>,
    visited: HashSet<(usize, Vec<String>)>,
    /// Commands whose own help was read, by path.
    commands: Vec<HashMap<Vec<String>, SpecCommand>>,
    /// A parent's children, in the order its help listed them.
    order: Vec<ChildIndex>,
    /// All that is known about each child from its parent, should its own help
    /// turn out to be unreadable.
    stubs: Vec<HashMap<Vec<String>, SpecCommand>>,
}

impl Walk {
    fn new(tools: usize) -> Self {
        Self {
            outstanding: 0,
            next_id: 0,
            pending: HashMap::new(),
            visited: HashSet::new(),
            commands: (0..tools).map(|_| HashMap::new()).collect(),
            order: (0..tools).map(|_| HashMap::new()).collect(),
            stubs: (0..tools).map(|_| HashMap::new()).collect(),
        }
    }

    /// Put one node in the bag. `false` if this path was already taken, so a tool
    /// that lists the same command twice is fetched once and emitted once.
    fn submit(
        &mut self,
        loader: &BackgroundLoader,
        tool: usize,
        tool_id: &str,
        path: Vec<String>,
        name: String,
        depth: usize,
    ) -> bool {
        if !self.visited.insert((tool, path.clone())) {
            return false;
        }
        let node_id = self.next_id.to_string();
        self.next_id += 1;
        self.pending.insert(node_id.clone(), Pending { tool, path: path.clone(), name, depth });
        self.outstanding += 1;
        loader.request(LoadRequest {
            node_id,
            tool_id: tool_id.to_string(),
            command_path: path,
            priority: Priority::High,
        });
        true
    }

    fn integrate(&mut self, loader: &BackgroundLoader, tools: &[Tool], outcome: crate::data::loader::LoadOutcome) {
        let Some(node) = self.pending.remove(&outcome.node_id) else {
            self.outstanding -= 1;
            return;
        };

        if let Ok(loaded) = outcome.loaded {
            let mut cmd = SpecCommand::builder().name(node.name).build();
            if !loaded.description.is_empty() {
                cmd.help = Some(loaded.description);
            }
            cmd.flags = loaded.flags.iter().map(spec_flag).collect();
            cmd.args = loaded.args.iter().map(spec_arg).collect();
            cmd.subcommand_required = !loaded.runnable;

            // The count of outstanding work belongs to this thread alone, and a
            // node's children join it in the same step that retires the node. That
            // is what lets zero mean "the tree is exhausted" rather than "nothing
            // happens to be in flight just now".
            if node.depth < MAX_DEPTH {
                for child in &loaded.children {
                    // A source that delivers its whole tree in one dump — mise's
                    // tasks, mise's own CLI, any usage spec — has already handed
                    // over this child and everything beneath it. Asking for it
                    // again answers with nothing, which is the source saying "you
                    // have it", not "there is nothing there".
                    if let Children::Loaded(_) = child.children {
                        if self.visited.insert((node.tool, child.command_path.clone())) {
                            self.record_child(node.tool, &node.path, child);
                            self.commands[node.tool]
                                .insert(child.command_path.clone(), command_from_node(child));
                        }
                        continue;
                    }
                    let accepted = self.submit(
                        loader,
                        node.tool,
                        &tools[node.tool].id,
                        child.command_path.clone(),
                        child.name.clone(),
                        node.depth + 1,
                    );
                    if !accepted {
                        continue;
                    }
                    self.record_child(node.tool, &node.path, child);
                    self.stubs[node.tool].insert(child.command_path.clone(), stub(child));
                }
            }
            self.commands[node.tool].insert(node.path, cmd);
        }

        self.outstanding -= 1;
    }

    fn record_child(&mut self, tool: usize, parent: &[String], child: &Node) {
        self.order[tool]
            .entry(parent.to_vec())
            .or_default()
            .push(Child { name: child.name.clone(), path: child.command_path.clone() });
    }

    fn spec_for(&mut self, index: usize, tool: &Tool) -> Option<Spec> {
        let root = self.commands[index].remove(&Vec::new())?;
        let mut spec = Spec::default();
        spec.cmd = self.assemble(index, &[], root);
        spec.bin = tool.bin.clone();
        spec.name = tool.name.clone();
        Some(spec)
    }

    fn assemble(&mut self, tool: usize, path: &[String], mut cmd: SpecCommand) -> SpecCommand {
        let Some(children) = self.order[tool].remove(path) else {
            return cmd;
        };
        for Child { name, path: child_path } in children {
            // A child whose own help could not be read still belongs in the
            // document: the parent's command listing already gave its name and
            // summary, and a silent hole would read as "this command does not
            // exist".
            let child = self.commands[tool]
                .remove(&child_path)
                .or_else(|| self.stubs[tool].remove(&child_path))
                .unwrap_or_else(|| SpecCommand::builder().name(name.clone()).build());
            let sub = self.assemble(tool, &child_path, child);
            cmd.subcommands.insert(name, sub);
        }
        cmd
    }
}

/// A command, and everything under it, from a subtree the source already
/// delivered — no fetch, because nothing here is missing.
fn command_from_node(node: &Node) -> SpecCommand {
    let mut cmd = SpecCommand::builder().name(node.name.clone()).build();
    if !node.description.is_empty() {
        cmd.help = Some(node.description.clone());
    }
    cmd.flags = node.flags.iter().map(spec_flag).collect();
    cmd.args = node.args.iter().map(spec_arg).collect();
    cmd.subcommand_required = !node.runnable;
    if let Children::Loaded(children) = &node.children {
        for child in children {
            let sub = command_from_node(child);
            cmd.subcommands.insert(sub.name.clone(), sub);
        }
    }
    cmd
}

/// All that is known about a command whose own help could not be fetched.
fn stub(node: &Node) -> SpecCommand {
    let mut cmd = SpecCommand::builder().name(node.name.clone()).build();
    if !node.description.is_empty() {
        cmd.help = Some(node.description.clone());
    }
    cmd
}

fn spec_flag(flag: &Flag) -> SpecFlag {
    // `Flag::name` is a display form (`--verbose`); a spec wants the bare name.
    let name = flag
        .long
        .clone()
        .or_else(|| flag.short.map(|c| c.to_string()))
        .unwrap_or_else(|| flag.name.trim_start_matches('-').to_string());

    let mut spec = SpecFlag::builder().name(name).build();
    spec.short = flag.short.into_iter().collect();
    spec.long = flag.long.clone().into_iter().collect();
    spec.required = flag.required;
    if !flag.description.is_empty() {
        spec.help = Some(flag.description.clone());
    }
    if let FlagKind::Value { arg_name, default, choices } = &flag.kind {
        let mut arg = SpecArg::builder().name(arg_name.clone()).build();
        if !choices.is_empty() {
            arg.choices = Some(SpecChoices { choices: choices.clone() });
        }
        spec.arg = Some(arg);
        if !default.is_empty() {
            spec.default = vec![default.clone()];
        }
    }
    spec
}

fn spec_arg(arg: &Arg) -> SpecArg {
    let mut spec = SpecArg::builder().name(arg.name.clone()).build();
    spec.required = arg.required;
    if !arg.description.is_empty() {
        spec.help = Some(arg.description.clone());
    }
    if !arg.default.is_empty() {
        spec.default = vec![arg.default.clone()];
    }
    if !arg.choices.is_empty() {
        spec.choices = Some(SpecChoices { choices: arg.choices.clone() });
    }
    spec
}
