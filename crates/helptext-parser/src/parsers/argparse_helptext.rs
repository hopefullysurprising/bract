use crate::error::ParseError;
use usage::{Spec, SpecArg, SpecCommand, SpecFlag};

// Python's argparse.HelpFormatter renders, in order: a `usage:` line wrapped onto lines
// aligned beneath it, the parser's description, one section per argument group, then
// the epilog. A section's heading sits at column 0 and its entries two columns in, each
// an invocation followed by help that starts on the same line or, when the invocation
// is too wide, on the next. A subcommand slot lists its choices two columns deeper, and
// leaves out any choice added without help.
//
// Python changed the layout twice: `options:` was `optional arguments:` before 3.10,
// and before 3.13 every option string repeated the metavar (`-c CONFIG, --config
// CONFIG`). From 3.14 the page is coloured whenever FORCE_COLOR is set, piped or not.

const ENTRY_INDENT: usize = 2;
const CHOICE_INDENT: usize = 4;

/// The escape sequences 3.14's theme wraps around headings, options and metavars.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn depth_change(c: char) -> isize {
    match c {
        '[' | '(' | '{' | '<' => 1,
        ']' | ')' | '}' | '>' => -1,
        _ => 0,
    }
}

/// Usage syntax split at whitespace outside brackets: `[--all-files | --files [FILES ...]]`
/// stays one token.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0isize;
    for c in text.chars() {
        if c.is_whitespace() && depth == 0 {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        depth = (depth + depth_change(c)).max(0);
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The option strings of one entry: `-c, --config CONFIG` and, before 3.13,
/// `-c CONFIG, --config CONFIG`. A choices metavar holds commas of its own.
fn option_strings(invocation: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0isize;
    let mut start = 0;
    for (i, c) in invocation.char_indices() {
        depth = (depth + depth_change(c)).max(0);
        if c == ',' && depth == 0 && invocation[i + 1..].starts_with(' ') {
            parts.push(&invocation[start..i]);
            start = i + 2;
        }
    }
    parts.push(&invocation[start..]);
    parts
}

fn is_metavar(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_uppercase()) && token.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// The program's words, leading the usage until the first piece of syntax.
fn is_prog_word(token: &str) -> bool {
    !token.starts_with(['[', '(', '{', '<', '-']) && token != "..." && !is_metavar(token)
}

fn braced_choices(token: &str) -> Option<Vec<String>> {
    let inner = token.strip_prefix('{')?.strip_suffix('}')?;
    Some(inner.split(',').map(String::from).collect())
}

/// textwrap breaks a long word after a hyphen, so `--metadata-` / `dump` is one word.
fn join_wrapped(text: &mut String, line: &str) {
    let hyphen_break = text.ends_with('-') && text[..text.len() - 1].ends_with(|c: char| c.is_alphanumeric());
    if !text.is_empty() && !hyphen_break {
        text.push(' ');
    }
    text.push_str(line.trim());
}

/// An invocation, then help after a gap of two or more spaces.
fn split_entry(line: &str) -> (&str, &str) {
    let entry = line.trim();
    match entry.split_once("  ") {
        Some((invocation, help)) => (invocation, help.trim()),
        None => (entry, ""),
    }
}

struct Entry {
    invocation: String,
    help: String,
    choices: Vec<(String, String)>,
}

fn value_arg(flag_name: &str, metavar: &str) -> SpecArg {
    let first = tokens(metavar).into_iter().next().unwrap_or_default();
    let bare = first.trim_start_matches('[').trim_end_matches(']').split_whitespace().next().unwrap_or("").to_string();
    match braced_choices(&bare) {
        Some(choices) => SpecArg::builder().name(flag_name).choices(choices).required(true).build(),
        None if bare.is_empty() || bare == "..." => SpecArg::builder().name(flag_name).required(true).build(),
        None => SpecArg::builder().name(bare).required(true).build(),
    }
}

fn flag(entry: &Entry, usage: &[String]) -> Option<SpecFlag> {
    let mut shorts = Vec::new();
    let mut longs = Vec::new();
    let mut metavar = None;
    let mut first = None;
    for part in option_strings(&entry.invocation) {
        let (option, value) = part.split_once(' ').unwrap_or((part, ""));
        first.get_or_insert(option);
        if !value.trim().is_empty() {
            metavar = Some(value.trim());
        }
        if let Some(long) = option.strip_prefix("--").filter(|l| !l.is_empty()) {
            longs.push(long.to_string());
        } else if let Some(short) = option.strip_prefix('-') {
            let mut chars = short.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                shorts.push(c);
            }
        }
    }
    let name = longs.first().cloned().or_else(|| shorts.first().map(char::to_string))?;
    // A required option is the one argparse leaves unbracketed in the usage.
    let required = first.is_some_and(|first| usage.iter().any(|t| t == first));
    let mut flag = SpecFlag::builder().name(name.clone()).shorts(shorts).longs(longs).required(required);
    if let Some(metavar) = metavar {
        flag = flag.arg(value_arg(&name, metavar));
    }
    if !entry.help.is_empty() {
        flag = flag.help(entry.help.clone());
    }
    Some(flag.build())
}

/// How a positional appears in the usage: `name`, `[name]`, `name [name ...]`,
/// `[name ...]`, or `[name [name ...]]` before 3.9. One that never appears is
/// `nargs=REMAINDER`, shown as a bare `...`, or sits in a usage the tool wrote itself.
fn occurrence(usage: &[String], name: &str) -> (bool, bool) {
    let many = format!("[{name} ...]");
    for (k, token) in usage.iter().enumerate() {
        if token == name {
            return (true, usage.get(k + 1) == Some(&many));
        }
        if *token == format!("[{name}]") {
            return (false, false);
        }
        if *token == many || *token == format!("[{name} [{name} ...]]") {
            return (false, true);
        }
    }
    (false, false)
}

/// A subcommand slot is rendered `{a,b} ...` or `METAVAR ...` in the usage, and lists
/// its choices beneath its entry.
fn is_subcommand_slot(entry: &Entry, usage: &[String]) -> bool {
    !entry.choices.is_empty() || usage.windows(2).any(|w| w[0] == entry.invocation && w[1] == "...")
}

fn description(lines: &[&str]) -> Option<String> {
    let start = lines.iter().position(|l| !l.trim().is_empty())?;
    let end = lines.iter().rposition(|l| !l.trim().is_empty())? + 1;
    Some(lines[start..end].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n"))
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let content = strip_ansi(content);
    let lines: Vec<&str> = content.lines().collect();

    let mut i = lines.iter().position(|l| l.starts_with("usage:")).unwrap_or(0);
    let mut usage_text = String::new();
    if let Some(first) = lines.get(i).and_then(|l| l.strip_prefix("usage:")) {
        usage_text.push_str(first);
        i += 1;
        while let Some(line) = lines.get(i).filter(|l| !l.trim().is_empty()) {
            usage_text.push(' ');
            usage_text.push_str(line.trim());
            i += 1;
        }
    }
    let usage = tokens(&usage_text);
    let prog: Vec<&str> = usage.iter().map(String::as_str).take_while(|t| is_prog_word(t)).collect();
    // A positional can share its name with a word of the program:
    // `ansible-galaxy collection download [collection ...]`.
    let syntax = &usage[prog.len()..];

    let mut preamble: Vec<&str> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut in_sections = false;
    // The entry, or the listed choice, that an indented line continues; none after a
    // group's description.
    let mut open: Option<(usize, Option<usize>)> = None;

    for (n, line) in lines.iter().enumerate().skip(i) {
        if line.trim().is_empty() {
            if !in_sections {
                preamble.push("");
            }
            continue;
        }
        let depth = indent(line);
        if depth == 0 {
            let opens_section = line.trim_end().ends_with(':') && lines.get(n + 1).is_some_and(|next| !next.trim().is_empty() && indent(next) > 0);
            if opens_section {
                in_sections = true;
                open = None;
                continue;
            }
            // Text after the entries is the epilog, and whatever a tool prints after its
            // help — yq appends `jq --help` — is not argparse's.
            if !entries.is_empty() {
                break;
            }
        }
        if !in_sections {
            preamble.push(line);
            continue;
        }
        if depth == ENTRY_INDENT {
            let (invocation, help) = split_entry(line);
            let is_entry = invocation.starts_with('-') || !invocation.contains(char::is_whitespace);
            open = None;
            if is_entry {
                entries.push(Entry { invocation: invocation.to_string(), help: help.to_string(), choices: Vec::new() });
                open = Some((entries.len() - 1, None));
            }
            continue;
        }
        let Some((index, choice)) = open else { continue };
        let entry = &mut entries[index];
        if depth == CHOICE_INDENT && !entry.invocation.starts_with('-') {
            let (invocation, help) = split_entry(line);
            let name = invocation.split_whitespace().next().unwrap_or_default();
            entry.choices.push((name.to_string(), help.to_string()));
            open = Some((index, Some(entry.choices.len() - 1)));
            continue;
        }
        match choice {
            Some(c) => join_wrapped(&mut entry.choices[c].1, line),
            None => join_wrapped(&mut entry.help, line),
        }
    }

    let mut flags = Vec::new();
    let mut args = Vec::new();
    let mut subcommands = Vec::new();
    for entry in &entries {
        if entry.invocation.starts_with('-') {
            flags.extend(flag(entry, syntax));
            continue;
        }
        if is_subcommand_slot(entry, syntax) {
            let listed = if entry.choices.is_empty() {
                braced_choices(&entry.invocation).unwrap_or_default().into_iter().map(|name| (name, String::new())).collect()
            } else {
                entry.choices.clone()
            };
            for (name, help) in listed {
                let mut cmd = SpecCommand::builder().name(name).build();
                cmd.help = (!help.is_empty()).then_some(help);
                subcommands.push(cmd);
            }
            continue;
        }
        let (required, var) = occurrence(syntax, &entry.invocation);
        let mut arg = SpecArg::builder().name(super::placeholder_name(&entry.invocation)).required(required).var(var);
        if let Some(choices) = braced_choices(&entry.invocation) {
            arg = arg.choices(choices);
        }
        if !entry.help.is_empty() {
            arg = arg.help(entry.help.clone());
        }
        args.push(arg.build());
    }

    let description = description(&preamble);
    // A page's description is its command's long one; the listing above a subcommand
    // carries its summary, and only the root has no listing above it.
    let summary = description.as_deref().filter(|_| prog.len() == 1).and_then(|d| {
        let mut summary = String::new();
        d.split("\n\n").next()?.lines().for_each(|l| join_wrapped(&mut summary, l));
        Some(summary)
    });

    let bin = prog.first().copied().unwrap_or_default().to_string();
    let name = prog.last().copied().unwrap_or_default().to_string();
    let has_subcommands = !subcommands.is_empty();
    let mut cmd = SpecCommand::builder().name(name.clone()).flags(flags).args(args).subcommands(subcommands).build();
    // Help never says whether a subcommand is optional: pre-commit runs `run` when given
    // none, and its page reads like any other group's.
    cmd.subcommand_required = has_subcommands;
    cmd.help = summary;
    cmd.help_long = description;

    let mut spec = Spec::default();
    spec.name = name;
    spec.bin = bin;
    spec.cmd = cmd;
    Ok(spec)
}
