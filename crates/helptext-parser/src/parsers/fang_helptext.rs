//! Help that charm's fang renders for a Cobra CLI: titles at a two-space margin,
//! entries indented beneath them. It prints no flag's value placeholder, and no run
//! form for a command that has children.

use crate::error::ParseError;
use crate::parsers::cobra_helptext::{is_metavar, split_flag_and_description};
use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

enum Section {
    Description,
    Usage,
    Examples,
    Commands,
    Flags,
    Done,
}

/// fang 0.3 pads with no-break spaces where later versions use plain ones.
fn normalise(line: &str) -> String {
    line.replace('\u{a0}', " ").trim_end().to_string()
}

fn title(line: &str) -> Option<&str> {
    let text = line.strip_prefix("  ")?;
    (!text.is_empty() && !text.starts_with(' ') && !text.chars().any(char::is_lowercase)).then_some(text)
}

pub fn renders(page: &str) -> bool {
    page.lines().any(|line| title(&normalise(line)) == Some("USAGE"))
}

/// A usage line's words, with each bracketed group kept whole: `[<id | branch>]`.
fn tokens(usage: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in usage.chars() {
        match c {
            '[' | '<' => depth += 1,
            ']' | '>' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_command_word(token: &str) -> bool {
    !token.starts_with(['[', '<']) && !is_metavar(token)
}

/// `[command]` and `<command>` stand for a subcommand only where there are some:
/// `glab duo cli [command]` passes its command on to another program. An
/// alternative written as a flag — `[<repo> | -g <group>]`, or `[-- <gitflags>...]`
/// passing words on — is not a positional.
fn usage_arg(token: &str, has_subcommands: bool) -> Option<SpecArg> {
    let (inner, required) = if let Some(inner) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        (inner, false)
    } else if let Some(inner) = token.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        (inner, true)
    } else if is_metavar(token) {
        (token, true)
    } else {
        return None;
    };
    let alternatives: Vec<&str> =
        inner.trim_end_matches("...").split('|').map(str::trim).filter(|a| !a.starts_with('-')).collect();
    let placeholders = alternatives.iter().any(|a| a.starts_with('<'));
    let names: Vec<String> = alternatives
        .iter()
        .map(|a| a.trim_start_matches('<').trim_end_matches("...").trim_end_matches('>').to_string())
        .collect();
    let name = names.join("|");
    let reserved = name == "--flags" || has_subcommands && matches!(name.as_str(), "command" | "subcommand");
    if name.is_empty() || reserved {
        return None;
    }
    let mut arg = SpecArg::builder().name(name).build();
    arg.required = required;
    if names.len() > 1 && !placeholders {
        arg.choices = Some(SpecChoices::new(names));
    }
    Some(arg)
}

fn command(line: &str) -> Option<SpecCommand> {
    let (key, help) = split_flag_and_description(line.trim());
    let mut builder = SpecCommand::builder().name(key.split_whitespace().next()?.to_string());
    if let Some(help) = help {
        builder = builder.help(help);
    }
    Some(builder.build())
}

/// With no placeholder printed, every flag but Cobra's own `--help` and `--version`
/// is offered a value: the form writes `--flag=value`, which pflag reads for a
/// switch too.
fn flag(line: &str) -> Option<SpecFlag> {
    let (key, help) = split_flag_and_description(line.trim());
    let mut short = None;
    let mut long = None;
    for token in key.split_whitespace() {
        if let Some(name) = token.strip_prefix("--") {
            long = Some(name.to_string());
        } else if let Some(name) = token.strip_prefix('-') {
            short = name.chars().next().filter(|_| name.chars().count() == 1);
        }
    }
    let long = long?;
    let mut builder = SpecFlag::builder().name(long.clone()).long(long.clone());
    if let Some(short) = short {
        builder = builder.short(short);
    }
    if let Some(help) = help {
        builder = builder.help(help);
    }
    if !matches!(long.as_str(), "help" | "version") {
        builder = builder.arg(SpecArg::builder().name(long).build());
    }
    Some(builder.build())
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut section = Section::Description;
    let mut description: Vec<String> = Vec::new();
    let mut usage: Option<String> = None;
    let mut subcommands: Vec<SpecCommand> = Vec::new();
    let mut flags: Vec<SpecFlag> = Vec::new();

    for line in content.lines().map(normalise) {
        if matches!(section, Section::Done) {
            break;
        }
        if line.is_empty() {
            if matches!(section, Section::Description) {
                description.push(String::new());
            }
            continue;
        }
        // fang writes nothing at column 0: this is the tool's own text after the page.
        if !line.starts_with(' ') {
            section = Section::Done;
            continue;
        }
        match (title(&line), &section) {
            (Some("USAGE"), _) => {
                section = Section::Usage;
                continue;
            }
            (Some(text), Section::Usage | Section::Examples | Section::Commands | Section::Flags) => {
                section = match text {
                    "EXAMPLES" => Section::Examples,
                    "FLAGS" => Section::Flags,
                    _ => Section::Commands,
                };
                continue;
            }
            _ => {}
        }
        match section {
            Section::Description => description.push(line.strip_prefix("  ").unwrap_or(&line).to_string()),
            Section::Usage if usage.is_none() => usage = Some(line.trim().to_string()),
            Section::Commands => subcommands.extend(command(&line)),
            Section::Flags => flags.extend(flag(&line)),
            Section::Usage | Section::Examples | Section::Done => {}
        }
    }

    let usage = usage.ok_or_else(|| ParseError::InvalidInput("no USAGE section".into()))?;
    let words = tokens(&usage);
    let bin = words.first().cloned().unwrap_or_default();
    let has_subcommands = !subcommands.is_empty();
    let args: Vec<SpecArg> = words.iter().skip(1).filter_map(|t| usage_arg(t, has_subcommands)).collect();

    let description = description.join("\n").trim().to_string();
    // A page prints its command's long description when it has one; only the
    // listing above it carries the summary, and the root has no listing above it.
    let is_root = words.iter().take_while(|t| is_command_word(t)).count() == 1;
    let summary = description.split("\n\n").next().map(|p| p.lines().map(str::trim).collect::<Vec<_>>().join(" "));

    let mut cmd = SpecCommand::builder().name(bin.clone()).flags(flags).args(args).subcommands(subcommands);
    if let Some(summary) = summary.filter(|s| is_root && !s.is_empty()) {
        cmd = cmd.help(summary);
    }
    if !description.is_empty() {
        cmd = cmd.help_long(description.clone());
    }

    let mut spec = Spec::default();
    spec.name = bin.clone();
    spec.bin = bin;
    spec.cmd = cmd.build();
    spec.cmd.subcommand_required = has_subcommands;
    if !description.is_empty() {
        spec.about = Some(description);
    }
    Ok(spec)
}
