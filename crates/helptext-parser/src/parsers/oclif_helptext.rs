//! oclif help pages, as `@oclif/core`'s help renderer writes them (lib/help:
//! the same in 4.x and 5.x) and as sf and shopify extend it.
//!
//! A list section — flags, arguments, topics, commands — comes in one of two
//! layouts. Two-column puts each description beside its label, wrapped lines
//! aligned under it. Stacked puts the label alone and the description four
//! columns deeper beneath it; oclif switches a list to it once any description
//! wraps past four lines, and shopify uses it for every flag section.

use crate::error::ParseError;
use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Preamble,
    Usage,
    Arguments,
    Flags { global: bool },
    Topics,
    Commands,
    Other,
}

/// oclif headers are bare uppercase words at column 0. Any flag group a command
/// declares (`PACKAGING FLAGS`) renders as `<GROUP> FLAGS`; `FLAG DESCRIPTIONS`
/// only repeats flags already declared, so it is read as prose.
fn detect_section(line: &str) -> Option<Section> {
    let header = line.trim_end();
    if header.is_empty() || line.starts_with(char::is_whitespace) || header.chars().any(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(match header {
        "USAGE" => Section::Usage,
        "ARGUMENTS" => Section::Arguments,
        "GLOBAL FLAGS" => Section::Flags { global: true },
        "FLAGS" => Section::Flags { global: false },
        h if h.ends_with(" FLAGS") => Section::Flags { global: false },
        "TOPICS" => Section::Topics,
        "COMMANDS" => Section::Commands,
        _ => Section::Other,
    })
}

/// Split an entry from its description at the first run of two or more spaces.
fn split_entry(line: &str) -> (&str, Option<&str>) {
    let trimmed = line.trim();
    match trimmed.find("  ") {
        Some(at) => (&trimmed[..at], Some(trimmed[at..].trim_start())),
        None => (trimmed, None),
    }
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A list entry: its label, and the description lines that belong to it.
struct Entry<'a> {
    label: &'a str,
    lines: Vec<&'a str>,
}

/// Group a list section's lines into entries, whichever layout it uses.
///
/// Two-column: every description starts at one column, and a line starting
/// there continues the entry above. Labels sit left of it — flags without a
/// short form four columns in from those with one. Stacked: labels share the
/// section's indent and everything deeper is description.
fn entries<'a>(lines: &[&'a str], is_label: impl Fn(&str) -> bool) -> Vec<Entry<'a>> {
    let lines: Vec<&str> = lines.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    let description_column = lines.iter().find_map(|line| {
        let (label, description) = split_entry(line);
        let description = description.filter(|_| is_label(label))?;
        Some(line.len() - line.trim_start().len() + line.trim_start().find(description)?)
    });
    let base = lines.iter().map(|l| indent(l)).min().unwrap_or(0);
    let starts_entry = |line: &str| match description_column {
        Some(column) => indent(line) < column && is_label(line.trim()),
        None => indent(line) == base,
    };

    let mut entries: Vec<Entry> = Vec::new();
    for line in lines {
        if starts_entry(line) {
            let (label, description) = split_entry(line);
            entries.push(Entry { label, lines: description.into_iter().collect() });
        } else if let Some(entry) = entries.last_mut() {
            entry.lines.push(line.trim());
        }
    }
    entries
}

/// A command word as oclif prints it in a usage line: `org`, `apps:create`,
/// `data:COMMAND`. Placeholders (`PLUGIN`, `[APP]`, `COMMAND`) and flags are not.
fn is_command_word(token: &str) -> bool {
    token.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !token.contains(['<', '[', '|', '='])
}

struct Usage {
    path: Vec<String>,
    dispatch_only: bool,
    /// The tokens after the command path.
    rest: Vec<String>,
}

fn read_usage(form: &str) -> (String, Usage) {
    let mut tokens = form.split_whitespace().skip_while(|t| *t == "$");
    let bin = tokens.next().unwrap_or_default().to_string();
    let mut path = Vec::new();
    let mut dispatch_only = false;
    let mut rest = Vec::new();
    for token in tokens.by_ref() {
        if let Some(topic) = token.strip_suffix(":COMMAND") {
            path.push(topic.to_string());
            dispatch_only = true;
            break;
        }
        if token == "COMMAND" || token == "[COMMAND]" {
            dispatch_only = true;
            break;
        }
        if !is_command_word(token) {
            rest.push(token.to_string());
            break;
        }
        path.push(token.to_string());
    }
    rest.extend(tokens.map(String::from));
    (bin, Usage { path, dispatch_only, rest })
}

/// A listing names each child by its full path (`org list auth`,
/// `apps:favorites:add`); the child itself is the part after its parent's path.
fn child_name<'a>(entry: &'a str, parent: &str) -> &'a str {
    if parent.is_empty() {
        return entry;
    }
    entry
        .strip_prefix(parent)
        .and_then(|rest| rest.strip_prefix([' ', ':']))
        .unwrap_or(entry)
}

fn choices(list: &str) -> SpecChoices {
    SpecChoices::new(list.split('|'))
}

/// What oclif writes ahead of a description, in the order it does: an
/// argument's `(a|b)`, a flag's `(required)`, then `[default: x, env: Y]` —
/// either half alone too. shopify also adds `[env: Y]` as a final line.
#[derive(Default)]
struct Description {
    text: String,
    required: bool,
    default: Vec<String>,
    choices: Option<SpecChoices>,
}

fn read_description(lines: &[&str]) -> Description {
    let mut parsed = Description::default();
    let mut prose = Vec::new();
    for line in lines {
        match line.strip_prefix("<options: ").and_then(|l| l.strip_suffix('>')) {
            Some(options) => parsed.choices = Some(choices(options)),
            None => prose.push(*line),
        }
    }
    let joined = prose.join(" ");
    let mut text = joined.as_str();
    loop {
        text = text.trim_start();
        if let Some(rest) = text.strip_prefix("(required)") {
            parsed.required = true;
            text = rest;
        } else if let Some((options, rest)) = text.strip_prefix('(').and_then(|t| t.split_once(')')).filter(|(o, _)| o.contains('|') && !o.contains(' ')) {
            parsed.choices.get_or_insert_with(|| choices(options));
            text = rest;
        } else if let Some((metadata, rest)) = text.strip_prefix('[').and_then(|t| t.split_once(']')).filter(|(m, _)| m.starts_with("default: ") || m.starts_with("env: ")) {
            for item in metadata.split(", ") {
                if let Some(value) = item.strip_prefix("default: ") {
                    parsed.default.push(value.to_string());
                }
            }
            text = rest;
        } else {
            break;
        }
    }
    let text = text.trim();
    let text = match text.rfind("[env: ").filter(|_| text.ends_with(']')) {
        Some(at) => text[..at].trim_end(),
        None => text,
    };
    parsed.text = text.to_string();
    parsed
}

/// The flag a list label declares: `-a, --app=<value>`, `    --all`,
/// `--[no-]track-source`, `--header=key:value...`. A value that lists its
/// choices — `=a|b`, or sf's legacy `=(a|b)` — offers them.
fn parse_flag(entry: &Entry, global: bool) -> Option<SpecFlag> {
    let mut short = None;
    let mut long = None;
    let mut value = None;
    for token in entry.label.split([',', ' ']).filter(|t| !t.is_empty()) {
        if let Some(rest) = token.strip_prefix("--") {
            let (name, given) = match rest.split_once('=') {
                Some((name, given)) => (name, Some(given)),
                None => (rest, None),
            };
            long = Some(name.trim_start_matches("[no-]").to_string());
            value = given;
        } else if let Some(c) = token.strip_prefix('-').filter(|c| c.len() == 1) {
            short = c.chars().next();
        }
    }
    let long = long?;
    let description = read_description(&entry.lines);

    let mut builder = SpecFlag::builder().name(long.clone()).long(long.clone());
    if let Some(c) = short {
        builder = builder.short(c);
    }
    if !description.text.is_empty() {
        builder = builder.help(description.text);
    }
    if let Some(value) = value {
        let mut arg = SpecArg::builder().name(long).build();
        let listed = value.trim_end_matches("...").trim_start_matches('(').trim_end_matches(')');
        arg.choices = description.choices.or_else(|| (listed.contains('|') && !listed.contains('<')).then(|| choices(listed)));
        builder = builder.arg(arg);
    }
    let mut flag = builder.build();
    flag.required = description.required;
    flag.default = description.default;
    flag.global = global;
    Some(flag)
}

fn parse_argument(entry: &Entry) -> SpecArg {
    let token = entry.label.trim_end_matches("...");
    let (name, required) = match token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        Some(inner) => (inner.trim_end_matches("..."), false),
        None => (token, true),
    };
    let description = read_description(&entry.lines);
    let mut builder = SpecArg::builder().name(name.to_string());
    if !description.text.is_empty() {
        builder = builder.help(description.text);
    }
    let mut arg = builder.build();
    arg.required = required;
    arg.default = description.default;
    arg.choices = description.choices;
    arg
}

/// The arguments a usage line names after the command path. Brackets make one
/// optional; a bracketed group opening with a flag is that flag's business, and
/// so is the token after a flag that takes a value (`-a APP`). `[flags]` is a
/// placeholder, as in Cobra-style usage lines.
fn usage_arguments(rest: &[String], flags: &[SpecFlag]) -> Vec<SpecArg> {
    let takes_value = |token: &str| {
        let name = token.trim_start_matches('-');
        flags.iter().any(|f| f.arg.is_some() && (f.long.iter().any(|l| l == name) || f.short.iter().any(|s| name.len() == 1 && name.starts_with(*s))))
    };
    let mut groups: Vec<(bool, bool)> = Vec::new();
    let mut skip_value = false;
    let mut found: Vec<SpecArg> = Vec::new();
    for token in rest {
        let opened: Vec<char> = token.chars().take_while(|c| matches!(c, '[' | '(')).collect();
        let inner = token.trim_start_matches(['[', '(']);
        let closed = inner.chars().rev().take_while(|c| matches!(c, ']' | ')')).count();
        let inner = inner.trim_end_matches([']', ')']);
        for bracket in &opened {
            groups.push((*bracket == '[', inner.starts_with('-')));
        }

        let in_flag_group = groups.iter().any(|(_, flag)| *flag);
        let name = inner.trim_end_matches("...");
        if in_flag_group || name.is_empty() || name == "|" {
        } else if inner.starts_with('-') {
            skip_value = !inner.contains('=') && takes_value(inner);
        } else if std::mem::take(&mut skip_value) {
        } else {
            let optional = groups.iter().any(|(square, _)| *square);
            let reserved = ["flags", "options", "command", "subcommand"].contains(&name.to_ascii_lowercase().as_str());
            let metavar = name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-')
                && name.chars().any(|c| c.is_ascii_uppercase());
            let word = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if !reserved && (metavar || (optional && word)) && !found.iter().any(|a| a.name == name) {
                let mut arg = SpecArg::builder().name(name.to_string()).build();
                arg.required = !optional;
                found.push(arg);
            }
        }
        for _ in 0..closed.min(groups.len()) {
            groups.pop();
        }
    }
    found
}

/// Entries sit at the section's indent; anything deeper continues the summary
/// above it, whether wrapped beside the name or stacked beneath it. An entry may
/// have no summary at all (sf's `version` topic).
fn listing(lines: &[&str]) -> Vec<(String, String)> {
    entries(lines, |_| true)
        .into_iter()
        .map(|entry| (entry.label.to_string(), entry.lines.join(" ")))
        .collect()
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut sections: Vec<(Section, Vec<&str>)> = vec![(Section::Preamble, Vec::new())];
    for line in content.lines() {
        match detect_section(line) {
            Some(next) => sections.push((next, Vec::new())),
            None => sections.last_mut().expect("the preamble is always open").1.push(line),
        }
    }

    let mut summary = None;
    let mut forms: Vec<String> = Vec::new();
    let mut listed_args: Option<Vec<SpecArg>> = None;
    let mut flags: Vec<SpecFlag> = Vec::new();
    let mut topics = Vec::new();
    let mut commands = Vec::new();
    for (section, lines) in &sections {
        match section {
            // A beta, preview or deprecated command says so above its summary,
            // which is always a single line.
            Section::Preamble => summary = lines.iter().map(|l| l.trim()).rfind(|l| !l.is_empty()).map(String::from),
            Section::Usage => {
                for line in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                    match forms.last_mut().filter(|_| !line.starts_with('$')) {
                        Some(form) => {
                            form.push(' ');
                            form.push_str(line);
                        }
                        None => forms.push(line.to_string()),
                    }
                }
            }
            Section::Arguments => {
                listed_args.get_or_insert_with(Vec::new).extend(entries(lines, |_| true).iter().map(parse_argument));
            }
            Section::Flags { global } => {
                flags.extend(entries(lines, |label| label.starts_with('-')).iter().filter_map(|e| parse_flag(e, *global)));
            }
            Section::Topics => topics.extend(listing(lines)),
            Section::Commands => commands.extend(listing(lines)),
            Section::Other => {}
        }
    }

    let usages: Vec<(String, Usage)> = forms.iter().map(|f| read_usage(f)).collect();
    let bin = usages.first().map(|(bin, _)| bin.clone()).unwrap_or_default();
    let path = usages.first().map(|(_, u)| u.path.clone()).unwrap_or_default();
    let parent = path.join(" ");

    let args = listed_args
        .unwrap_or_else(|| usages.first().map(|(_, u)| usage_arguments(&u.rest, &flags)).unwrap_or_default());

    let mut subcommands: Vec<SpecCommand> = Vec::new();
    for (entry, summary) in &topics {
        let mut cmd = SpecCommand::builder().name(child_name(entry, &parent).to_string()).help(summary.clone()).build();
        cmd.subcommand_required = true;
        subcommands.push(cmd);
    }
    // Topic summaries are merged across the plugins sharing a topic and often
    // describe some other command; a command's own summary describes it.
    for (entry, summary) in &commands {
        let name = child_name(entry, &parent);
        match subcommands.iter_mut().find(|c| c.name == name) {
            Some(cmd) => {
                cmd.subcommand_required = false;
                if !summary.is_empty() {
                    cmd.help = Some(summary.clone());
                }
            }
            None => subcommands.push(SpecCommand::builder().name(name.to_string()).help(summary.clone()).build()),
        }
    }

    let runs_itself = usages.iter().any(|(_, u)| !u.dispatch_only);

    let mut builder = SpecCommand::builder().name(bin.clone()).flags(flags).args(args).subcommands(subcommands);
    if let Some(summary) = &summary {
        builder = builder.help(summary.clone());
    }
    let mut cmd = builder.build();
    cmd.subcommand_required = !cmd.subcommands.is_empty() && !runs_itself;
    cmd.full_cmd = path;

    let mut spec = Spec::default();
    spec.name = bin.clone();
    spec.bin = bin;
    spec.about = summary;
    spec.cmd = cmd;
    Ok(spec)
}
