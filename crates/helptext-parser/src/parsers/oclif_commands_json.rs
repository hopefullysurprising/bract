//! `@oclif/plugin-commands`' `commands --json`: every visible command, flat, keyed
//! by a colon-joined id whatever separator the CLI shows its users. Topics are
//! not listed — a topic is any id prefix with commands beneath it.

use serde::Deserialize;
use serde_json::{Map, Value};
use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

use crate::error::ParseError;

#[derive(Deserialize)]
struct Command {
    id: String,
    summary: Option<String>,
    description: Option<String>,
    hidden: Option<bool>,
    #[serde(default)]
    flags: Map<String, Value>,
    #[serde(default)]
    args: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Flag {
    name: String,
    char: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    required: Option<bool>,
    options: Option<Vec<String>>,
    default: Option<Value>,
    hidden: Option<bool>,
    summary: Option<String>,
    description: Option<String>,
    help_group: Option<String>,
}

#[derive(Deserialize)]
struct Arg {
    name: String,
    hidden: Option<bool>,
    description: Option<String>,
    required: Option<bool>,
    options: Option<Vec<String>>,
    default: Option<Value>,
}

/// oclif writes a summary for help listings; heroku leaves it out, and a
/// description's first line stands in.
fn summary(summary: Option<String>, description: Option<String>) -> Option<String> {
    summary
        .or_else(|| description.and_then(|d| d.lines().next().map(str::to_string)))
        .filter(|s| !s.is_empty())
}

/// A default that is a value rather than a function oclif evaluates at run time.
fn defaults(value: Option<Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) => vec![s],
        Some(Value::Number(n)) => vec![n.to_string()],
        Some(Value::Bool(b)) => vec![b.to_string()],
        Some(Value::Array(items)) => items.into_iter().flat_map(|v| defaults(Some(v))).collect(),
        _ => vec![],
    }
}

fn choices(options: Option<Vec<String>>) -> Option<SpecChoices> {
    options.map(|choices| SpecChoices { choices })
}

fn spec_flag(flag: Flag) -> SpecFlag {
    let mut builder = SpecFlag::builder().name(flag.name.clone()).long(flag.name.clone());
    if let Some(c) = flag.char.and_then(|c| c.chars().next()) {
        builder = builder.short(c);
    }
    if let Some(help) = summary(flag.summary, flag.description) {
        builder = builder.help(help);
    }
    if flag.kind == "option" {
        let mut arg = SpecArg::builder().name(flag.name).build();
        arg.choices = choices(flag.options);
        builder = builder.arg(arg);
    }
    let mut spec = builder.build();
    spec.required = flag.required.unwrap_or(false);
    spec.default = defaults(flag.default);
    spec.global = flag.help_group.as_deref() == Some("GLOBAL");
    spec
}

fn spec_arg(arg: Arg) -> SpecArg {
    let mut builder = SpecArg::builder().name(arg.name);
    if let Some(help) = arg.description {
        builder = builder.help(help);
    }
    let mut spec = builder.build();
    spec.required = arg.required.unwrap_or(false);
    spec.default = defaults(arg.default);
    spec.choices = choices(arg.options);
    spec
}

fn spec_command(name: &str, command: Command) -> Result<SpecCommand, ParseError> {
    let parse = |e: serde_json::Error| ParseError::InvalidInput(format!("{}: {e}", command.id));
    let mut flags = Vec::new();
    for value in command.flags.values() {
        let flag: Flag = serde_json::from_value(value.clone()).map_err(parse)?;
        if !flag.hidden.unwrap_or(false) {
            flags.push(spec_flag(flag));
        }
    }
    let mut args = Vec::new();
    for value in command.args.values() {
        let arg: Arg = serde_json::from_value(value.clone()).map_err(parse)?;
        if !arg.hidden.unwrap_or(false) {
            args.push(spec_arg(arg));
        }
    }

    let mut builder = SpecCommand::builder().name(name.to_string()).flags(flags).args(args);
    if let Some(help) = summary(command.summary, command.description) {
        builder = builder.help(help);
    }
    Ok(builder.build())
}

fn topic(name: &str) -> SpecCommand {
    let mut cmd = SpecCommand::builder().name(name.to_string()).build();
    cmd.subcommand_required = true;
    cmd
}

/// Place `command` at `path`, creating the topics above it. A command arriving
/// where a topic already stands keeps the topic's children: the node both runs
/// and dispatches.
fn insert(parent: &mut SpecCommand, path: &[&str], command: SpecCommand) {
    let Some((first, rest)) = path.split_first() else { return };
    let node = parent.subcommands.entry(first.to_string()).or_insert_with(|| topic(first));
    if rest.is_empty() {
        let children = std::mem::take(&mut node.subcommands);
        *node = command;
        node.subcommands = children;
    } else {
        insert(node, rest, command);
    }
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let commands: Vec<Command> = serde_json::from_str(content)
        .map_err(|e| ParseError::InvalidInput(format!("not an oclif command list: {e}")))?;

    let mut root = SpecCommand::default();
    root.subcommand_required = true;
    for command in commands.into_iter().filter(|c| !c.hidden.unwrap_or(false)) {
        let id = command.id.clone();
        let path: Vec<&str> = id.split(':').collect();
        let leaf = spec_command(path.last().copied().unwrap_or_default(), command)?;
        insert(&mut root, &path, leaf);
    }

    let mut spec = Spec::default();
    spec.cmd = root;
    Ok(spec)
}
