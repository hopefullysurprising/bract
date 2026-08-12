//! Emit a tool's whole command tree as a usage spec, for reading without the TUI.
//!
//! The tree the TUI browses lazily is walked eagerly here, so every subcommand's
//! own `--help` is fetched. That is the cost of the mode: the point is to hand a
//! reader — often another program — one document describing everything a CLI can
//! do, rather than making it drive an interactive session to find out.
//!
//! Usage is the output format because it is already this project's lingua franca:
//! mise emits it, `helptext-parser` produces it, and its renderer round-trips the
//! same types the parsers build.

use helptext_parser::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

use crate::data::node::{Arg, Flag, FlagKind, Node};
use crate::data::source::Source;

/// A tool that reports itself among its own subcommands would otherwise recurse
/// until the stack runs out. No real CLI nests anywhere near this deep.
const MAX_DEPTH: usize = 10;

/// Walk every source to its leaves and render each as a usage spec.
pub fn usage_specs(sources: &[Box<dyn Source>]) -> Vec<Spec> {
    sources.iter().filter_map(|source| spec_for(source.as_ref())).collect()
}

fn spec_for(source: &dyn Source) -> Option<Spec> {
    let name = source.tool_name().to_string();
    let mut spec = Spec::default();
    spec.cmd = command_at(source, &[], &name, 0)?;
    spec.bin = source.tool_bin().join(" ");
    spec.name = name;
    Some(spec)
}

fn command_at(source: &dyn Source, path: &[String], name: &str, depth: usize) -> Option<SpecCommand> {
    let loaded = source.load(path).ok()?;

    let mut cmd = SpecCommand::builder().name(name.to_string()).build();
    if !loaded.description.is_empty() {
        cmd.help = Some(loaded.description);
    }
    cmd.flags = loaded.flags.iter().map(spec_flag).collect();
    cmd.args = loaded.args.iter().map(spec_arg).collect();
    cmd.subcommand_required = !loaded.runnable;

    if depth < MAX_DEPTH {
        for child in &loaded.children {
            // A child whose own help cannot be read still belongs in the document:
            // the parent's command listing already gave its name and summary, and
            // a silent hole would read as "this command does not exist".
            let sub = command_at(source, &child.command_path, &child.name, depth + 1)
                .unwrap_or_else(|| stub(child));
            cmd.subcommands.insert(sub.name.clone(), sub);
        }
    }
    Some(cmd)
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
