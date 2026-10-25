use crate::error::ParseError;
use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

// mitchellh/cli (continued as hashicorp/cli) renders two things itself. The root page
// is the tool's own HelpFunc, listing commands under headings each tool names. A
// command page is the command's hand-written `Help()`, then — when it has children —
// a `Subcommands:` heading over `    name    synopsis` lines. A parent nobody
// registered gets the framework's placeholder text instead of a `Help()`.
//
// Everything inside `Help()` is HashiCorp house style, not the framework: a `Usage:`
// line, an indented description, and sections whose flag lines start `-name` one or
// two columns in, described on the same line or on deeper-indented lines beneath.

const GENERATED_PARENT: &str = "This command is accessed by using one of the subcommands below.";

const FLAG_INDENT: usize = 2;

enum Section {
    Description,
    Commands,
    Other { global: bool },
}

/// In columns, with a tab stop every eight: nomad indents an example with tabs.
fn indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).fold(0, |column, c| if c == '\t' { column / 8 * 8 + 8 } else { column + 1 })
}

/// Any line at column 0 opens a section; consul's headings carry no colon.
fn section_of(heading: &str) -> Section {
    let lower = heading.trim().to_lowercase();
    if lower.ends_with("commands:") || lower.ends_with("commands are:") {
        Section::Commands
    } else {
        Section::Other { global: lower.starts_with("global") }
    }
}

fn command_entry(line: &str) -> Option<(String, Option<String>)> {
    let entry = line.trim();
    if entry.starts_with('-') {
        return None;
    }
    let (name, synopsis) = match entry.split_once("  ") {
        Some((name, synopsis)) => (name, synopsis.trim()),
        None => (entry, ""),
    };
    if name.contains(char::is_whitespace) {
        return None;
    }
    Some((name.to_string(), (!synopsis.is_empty()).then(|| synopsis.to_string())))
}

/// A whitespace-separated token, except that a bracketed placeholder keeps its inner
/// spaces: `<variable spec file reference>`, `-pgp-keys=<keybase:a, keybase:b>`.
fn next_token(text: &str) -> (&str, &str) {
    let mut depth = 0usize;
    let mut quote = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '<' | '[' | '(') => depth += 1,
            (None, '>' | ']' | ')') => depth = depth.saturating_sub(1),
            (None, c) if c.is_whitespace() && depth == 0 => return (&text[..i], &text[i..]),
            _ => {}
        }
    }
    (text, "")
}

fn tokens(text: &str) -> Vec<&str> {
    let mut rest = text.trim_start();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (token, tail) = next_token(rest);
        out.push(token);
        rest = tail.trim_start();
    }
    out
}

fn is_metavar(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_uppercase()) && token.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn value_arg(flag_name: &str, token: &str) -> SpecArg {
    let bare = super::placeholder_name(token.trim_matches(|c| matches!(c, '\'' | '"' | '(' | ')')));
    let options: Vec<String> = bare.split('|').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    if matches!(bare.as_str(), "true" | "false") {
        let mut arg = SpecArg::builder().name("bool").build();
        arg.choices = Some(SpecChoices::new(["true", "false"].map(String::from).to_vec()));
        return arg;
    }
    if options.len() > 1 {
        let mut arg = SpecArg::builder().name(flag_name).build();
        arg.choices = Some(SpecChoices::new(options));
        return arg;
    }
    let name = if bare.is_empty() || bare.contains(char::is_whitespace) { flag_name.to_string() } else { bare };
    SpecArg::builder().name(name).build()
}

/// `-name`, `-name=<type>`, `-name=VALUE`, `-name 'key=value'`, `-name <value>`,
/// `-name (hcl | json)` or `-name key1,...,keyN`, then the description, if it shares
/// the line. A name directly followed by anything else is prose that happens to open
/// with one: `-state, state-out, and -backup are …`.
fn flag_line(line: &str) -> Option<SpecFlag> {
    let body = line.trim_start().strip_prefix('-')?;
    let end = body.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))).unwrap_or(body.len());
    let name = &body[..end];
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return None;
    }
    let rest = &body[end..];
    let (value, help) = if let Some(value) = rest.strip_prefix('=') {
        let (token, help) = next_token(value);
        (Some(token), help)
    } else if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        let after = rest.strip_prefix(' ').unwrap_or(rest);
        let (token, tail) = next_token(after);
        // `-x [EXPERIMENTAL]` is a tag: a value spaced off its flag is never bracketed so.
        let spaced_value = !token.is_empty()
            && !after.starts_with(char::is_whitespace)
            && !token.starts_with('[')
            && (token.starts_with(['<', '\'', '"', '(']) || is_metavar(token) || tail.trim().is_empty());
        if spaced_value {
            (Some(token), tail)
        } else {
            (None, rest)
        }
    } else {
        return None;
    };

    let mut flag = SpecFlag::builder().name(name).long(name);
    if let Some(token) = value {
        flag = flag.arg(value_arg(name, token));
    }
    let help = help.trim();
    if !help.is_empty() {
        flag = flag.help(help);
    }
    Some(flag.build())
}

struct Usage<'a> {
    bin: Option<&'a str>,
    path: Vec<&'a str>,
    args: Vec<SpecArg>,
}

fn read_usage(line: &str) -> Usage<'_> {
    let mut usage = Usage { bin: None, path: Vec::new(), args: Vec::new() };
    let mut taking_args = false;
    for token in tokens(line) {
        if usage.bin.is_none() {
            usage.bin = Some(token);
            continue;
        }
        // A sample such as `K=V` or `[<key>=<value>]` shows what to type, not what it is called.
        let name = super::placeholder_name(token).split_whitespace().filter(|w| !w.contains('=')).collect::<Vec<_>>().join(" ");
        let lower = name.to_lowercase().replace('-', " ");
        if matches!(lower.as_str(), "options" | "flags") {
            taking_args = true;
            continue;
        }
        // `[global options]`, `[target-selection-option]` and `([-kind …] | [-f FILENAME])`
        // are flags, however the page groups them.
        if name.is_empty() || name.starts_with(['-', '(']) || lower.ends_with("option") || lower.ends_with("options") {
            continue;
        }
        if !token.starts_with(['<', '[']) && !is_metavar(token) && !taking_args {
            usage.path.push(token);
            continue;
        }
        taking_args = true;
        let mut arg = SpecArg::builder().name(name).build();
        arg.required = !token.starts_with('[');
        usage.args.push(arg);
    }
    usage
}

fn strip_usage_label(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("Usage")?;
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

fn dedent(lines: &[&str]) -> Option<String> {
    let start = lines.iter().position(|l| !l.trim().is_empty())?;
    let end = lines.iter().rposition(|l| !l.trim().is_empty())? + 1;
    let lines = &lines[start..end];
    let margin = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    Some(lines.iter().map(|l| l.get(margin..).unwrap_or("").trim_end()).collect::<Vec<_>>().join("\n"))
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());

    let generated = lines.get(i).is_some_and(|l| l.trim() == GENERATED_PARENT);
    let mut usage_line = None;
    if generated {
        i += 1;
    } else if let Some(rest) = lines.get(i).and_then(|l| strip_usage_label(l)) {
        i += 1;
        usage_line = if rest.is_empty() { lines.get(i).map(|l| l.trim()) } else { Some(rest) };
        while lines.get(i).is_some_and(|l| !l.trim().is_empty()) {
            i += 1;
        }
    }
    let usage = read_usage(usage_line.unwrap_or(""));

    let mut section = Section::Description;
    let mut description = Vec::new();
    let mut children: Vec<(String, Option<String>)> = Vec::new();
    let mut flags: Vec<SpecFlag> = Vec::new();
    let mut open_flag: Option<usize> = None;
    // A flag line opens a block; one continuing a paragraph is prose that wrapped onto a
    // flag's name: `… using the` / `-path option.`
    let mut after_prose = false;

    for line in &lines[i..] {
        if line.trim().is_empty() {
            if matches!(section, Section::Description) {
                description.push("");
            }
            after_prose = false;
            continue;
        }
        if indent(line) == 0 {
            section = section_of(line);
            open_flag = None;
            after_prose = false;
            continue;
        }
        let flag = (indent(line) <= FLAG_INDENT && !after_prose).then(|| flag_line(line)).flatten();
        after_prose = indent(line) <= FLAG_INDENT && flag.is_none();
        let global = match section {
            Section::Commands => {
                children.extend(command_entry(line));
                continue;
            }
            // nomad lists some flags under no heading, and vault indents one heading
            // with a tab: a flag line ends the description wherever it stands.
            Section::Description if flag.is_none() => {
                description.push(line);
                continue;
            }
            Section::Description => false,
            Section::Other { global } => global,
        };
        section = Section::Other { global };
        if let Some(mut flag) = flag {
            flag.global = global;
            open_flag = None;
            if !flags.iter().any(|f| f.name == flag.name) {
                flags.push(flag);
                open_flag = Some(flags.len() - 1);
            }
        } else if indent(line) <= FLAG_INDENT {
            open_flag = None;
        } else if let Some(flag) = open_flag.and_then(|n| flags.get_mut(n)) {
            let help = flag.help.get_or_insert_with(String::new);
            if !help.is_empty() {
                help.push(' ');
            }
            help.push_str(line.trim());
        }
    }

    let subcommands: Vec<SpecCommand> = children
        .into_iter()
        .map(|(name, synopsis)| {
            let mut cmd = SpecCommand::builder().name(name).build();
            cmd.help = synopsis;
            cmd
        })
        .collect();

    let bin = usage.bin.unwrap_or_default().to_string();
    let name = usage.path.last().map(|n| n.to_string()).unwrap_or_else(|| bin.clone());
    // On a command with subcommands, `<subcommand> [args]` stands for the tree below; on
    // a leaf, `nomad alloc exec <allocation> <command>` takes a command to run.
    let mut args = usage.args;
    let mut required = generated;
    if !subcommands.is_empty() {
        args.retain(|arg| {
            let lower = arg.name.to_lowercase();
            required |= arg.required && matches!(lower.as_str(), "command" | "subcommand");
            !matches!(lower.as_str(), "command" | "subcommand" | "args")
        });
    }
    let mut cmd = SpecCommand::builder().name(name.clone()).flags(flags).args(args).subcommands(subcommands).build();
    cmd.subcommand_required = required;
    cmd.help_long = if generated { None } else { dedent(&description) };

    let mut spec = Spec::default();
    spec.name = name;
    spec.bin = bin;
    spec.cmd = cmd;
    Ok(spec)
}
