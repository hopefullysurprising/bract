use crate::error::ParseError;
use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

#[derive(Debug, PartialEq, Clone, Copy)]
enum Section {
    Preamble,
    Usage,
    Arguments,
    Commands,
    Options,
    // Sections we recognise but don't model (e.g. "Environment variables:"), plus
    // any column-0 trailer prose after the parsed sections.
    Other,
}

/// Recognise a column-0 section header. Clap's default template renders fixed
/// headers ("Usage:", "Arguments:", "Options:", "Commands:"); custom templates
/// occasionally add their own (zoxide's "Environment variables:"), which we read
/// as `Other` so they neither end up in the preamble nor pollute the tree.
fn detect_header(line: &str) -> Option<Section> {
    let header = line.trim_end().to_ascii_lowercase();
    match header.as_str() {
        "usage:" | "usage" => Some(Section::Usage),
        "arguments:" | "arguments" => Some(Section::Arguments),
        "options:" | "options" => Some(Section::Options),
        "commands:" | "commands" => Some(Section::Commands),
        _ if header.ends_with("commands:") || header.ends_with("commands") => Some(Section::Commands),
        "environment variables:" | "environment variables" => Some(Section::Other),
        _ => None,
    }
}

/// Pull clap's bracket annotations (`[default: x]`, `[possible values: a, b]`,
/// `[aliases: y]`, …) out of an assembled help string, returning the help with
/// them removed and the recognised annotations as (key, value) pairs. Unknown
/// brackets are left in place so help text like `fd -- '-foo'` survives. Runs as a
/// finalisation pass so annotations clap wraps across several lines still match.
fn extract_annotations(help: &str) -> (String, Vec<(String, String)>) {
    let mut anns = Vec::new();
    let mut cleaned = String::new();
    let mut rest = help;

    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        let inside = &rest[open + 1..close];
        if let Some((key, value)) = inside.split_once(": ") {
            let key = key.trim().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "default" | "possible values" | "aliases" | "alias" | "short aliases" | "env"
            ) {
                anns.push((key, value.trim().to_string()));
                cleaned.push_str(&rest[..open]);
                rest = &rest[close + 1..];
                continue;
            }
        }
        // Not a recognised annotation — keep the bracketed text verbatim.
        cleaned.push_str(&rest[..close + 1]);
        rest = &rest[close + 1..];
    }
    cleaned.push_str(rest);
    (cleaned.trim().to_string(), anns)
}

fn ann_default(anns: &[(String, String)]) -> Vec<String> {
    anns.iter()
        .find(|(k, _)| k == "default")
        .map(|(_, v)| vec![v.clone()])
        .unwrap_or_default()
}

fn ann_choices(anns: &[(String, String)]) -> Option<SpecChoices> {
    let (_, list) = anns.iter().find(|(k, _)| k == "possible values")?;
    let choices: Vec<String> = list
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (!choices.is_empty()).then_some(SpecChoices { choices })
}

fn ann_aliases(anns: &[(String, String)]) -> Vec<String> {
    anns.iter()
        .filter(|(k, _)| k == "aliases" || k == "alias")
        .flat_map(|(_, v)| v.split(',').map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split an entry line into its definition (flag/arg/command head) and inline
/// description, which clap separates by two or more spaces. In clap's next-line
/// layout the description is absent here and arrives on following indented lines.
fn split_def_and_description(line: &str) -> (String, Option<String>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            if i - start >= 2 && i < bytes.len() {
                return (line[..start].to_string(), Some(line[i..].to_string()));
            }
        } else {
            i += 1;
        }
    }
    (line.to_string(), None)
}

/// Strip the surrounding `<>`/`[]` brackets and any variadic `...` marker from a
/// metavar token, yielding the bare placeholder name.
fn metavar_name(token: &str) -> String {
    token
        .replace("...", "")
        .trim_matches(|c| matches!(c, '<' | '>' | '[' | ']'))
        .to_string()
}

/// Append a wrapped/next-line continuation onto a running help string.
fn append_help(help: &mut Option<String>, text: &str) {
    if text.is_empty() {
        return;
    }
    match help {
        Some(existing) => {
            existing.push(' ');
            existing.push_str(text);
        }
        None => *help = Some(text.to_string()),
    }
}

fn parse_flag_def(def: &str) -> Option<SpecFlag> {
    let mut short = Vec::new();
    let mut long = Vec::new();
    let mut arg: Option<SpecArg> = None;
    let mut count = false;

    for token in def.split_whitespace() {
        if let Some(rest) = token.strip_prefix("--") {
            // Clap attaches an optional value to the name itself:
            // `--include-args[=<INCLUDE_ARGS>]`. Split it off, or the bracketed
            // spec becomes part of the flag's name.
            let (rest, optional_value) = match rest.split_once("[=") {
                Some((name, value)) => (name, Some(value.trim_end_matches(']'))),
                None => (rest, None),
            };
            if let Some(value) = optional_value {
                let mut a = SpecArg::builder().name(metavar_name(value)).build();
                a.required = false;
                a.var = value.contains("...");
                arg = Some(a);
            }
            // A trailing `...` on a switch (`-u, --unrestricted...`) marks a
            // repeatable count flag, not a value flag.
            match rest.strip_suffix("...") {
                Some(name) => {
                    long.push(name.to_string());
                    count = true;
                }
                None => long.push(rest.to_string()),
            }
        } else if let Some(rest) = token.strip_prefix('-') {
            // Short flag, possibly with a trailing comma (`-w,`) and/or `...`.
            let body = rest.trim_end_matches(',');
            if let Some(stem) = body.strip_suffix("...") {
                count = true;
                if let Some(c) = stem.chars().next()
                    && stem.chars().count() == 1
                {
                    short.push(c);
                }
            } else if let Some(c) = body.chars().next()
                && body.chars().count() == 1
            {
                short.push(c);
            }
        } else if token.starts_with('<') || token.starts_with('[') {
            // A metavar after the flag names marks a value-taking option.
            let mut a = SpecArg::builder().name(metavar_name(token)).build();
            a.var = token.contains("...");
            arg = Some(a);
        }
    }

    if long.is_empty() && short.is_empty() {
        return None;
    }

    let name = long
        .first()
        .cloned()
        .or_else(|| short.first().map(|c| c.to_string()))?;

    let mut builder = SpecFlag::builder().name(name);
    for s in &short {
        builder = builder.short(*s);
    }
    for l in &long {
        builder = builder.long(l.clone());
    }
    let mut flag = builder.build();
    flag.count = count && arg.is_none();
    flag.arg = arg;
    Some(flag)
}

/// Recover clap's verbose "Possible values:" block — used instead of the inline
/// `[possible values: …]` form when values carry per-value help (e.g. bat). Two
/// shapes appear: a bullet list (`* name (desc)` / `* name: desc`) and an inline
/// comma list (`a, b, c.`), the latter marking its default by wrapping it in
/// `*…*`. Returns the help with the block removed, plus the choices and any
/// default. `(default)` in a bullet also marks the default.
fn extract_possible_values_block(help: &str) -> Option<(String, Vec<String>, Option<String>)> {
    const LABEL: &str = "Possible values:";
    let idx = help.find(LABEL)?;
    let cleaned = help[..idx].trim_end().to_string();
    let after = help[idx + LABEL.len()..].trim();

    let mut choices = Vec::new();
    let mut default = None;

    if after.contains("* ") {
        // Bullet list: each item starts with "* " and names a value before its
        // optional `(…)` / `:` description.
        for item in after.split("* ").map(str::trim).filter(|s| !s.is_empty()) {
            let name = item
                .split(['(', ':'])
                .next()
                .unwrap_or(item)
                .trim()
                .trim_end_matches('.')
                .trim();
            if name.is_empty() {
                continue;
            }
            if item.contains("(default)") {
                default = Some(name.to_string());
            }
            choices.push(name.to_string());
        }
    } else {
        // Inline comma list, e.g. "*auto*, never, always."
        for token in after.trim_end_matches('.').split(',').map(str::trim) {
            let is_default = token.len() > 1 && token.starts_with('*') && token.ends_with('*');
            let name = token.trim_matches('*').trim();
            if name.is_empty() {
                continue;
            }
            if is_default {
                default = Some(name.to_string());
            }
            choices.push(name.to_string());
        }
    }

    (!choices.is_empty()).then_some((cleaned, choices, default))
}

/// Once a flag's help is fully assembled, recover its `[default:]`/`[possible
/// values:]` annotations (and clap's verbose "Possible values:" block) and clean
/// them out of the visible help.
fn enrich_flag(flag: &mut SpecFlag) {
    let Some(help) = flag.help.take() else { return };
    let (mut help, anns) = extract_annotations(&help);
    flag.default = ann_default(&anns);
    let mut choices = ann_choices(&anns);
    if choices.is_none()
        && let Some((cleaned, values, default)) = extract_possible_values_block(&help)
    {
        help = cleaned;
        choices = Some(SpecChoices { choices: values });
        if flag.default.is_empty()
            && let Some(d) = default
        {
            flag.default = vec![d];
        }
    }
    flag.help = (!help.is_empty()).then_some(help);
    // Possible values only attach to a value-taking flag.
    if let (Some(c), Some(arg)) = (choices, flag.arg.as_mut()) {
        arg.choices = Some(c);
    }
}

fn enrich_arg(arg: &mut SpecArg) {
    let Some(help) = arg.help.take() else { return };
    let (mut help, anns) = extract_annotations(&help);
    arg.default = ann_default(&anns);
    let mut choices = ann_choices(&anns);
    if choices.is_none()
        && let Some((cleaned, values, default)) = extract_possible_values_block(&help)
    {
        help = cleaned;
        choices = Some(SpecChoices { choices: values });
        if arg.default.is_empty()
            && let Some(d) = default
        {
            arg.default = vec![d];
        }
    }
    arg.help = (!help.is_empty()).then_some(help);
    arg.choices = choices;
}

/// Split a command row's definition into its name and aliases. Clap renders a
/// command with aliases as `build, b` — one command, not two, and never a command
/// literally called `build,`.
///
/// Returns `None` for a row that names no command: cargo closes its curated list
/// with `...  See all commands with --list`, an elision marker that fails with
/// "no such command" if invoked. Anything without an alphanumeric character is
/// punctuation standing in for commands, not a command.
fn split_command_names(def: &str) -> Option<(String, Vec<String>)> {
    let mut names = def
        .split(',')
        .filter_map(|n| n.split_whitespace().next())
        .filter(|n| n.chars().any(char::is_alphanumeric));
    let name = names.next()?.to_string();
    Some((name, names.map(str::to_string).collect()))
}

/// A usage token that is clap's generic subcommand placeholder (`<COMMAND>` /
/// `[COMMAND]`), as opposed to a real positional.
fn is_command_placeholder(token: &str) -> bool {
    let bare = token.trim_matches(|c| matches!(c, '<' | '>' | '[' | ']'));
    bare.eq_ignore_ascii_case("command") || bare.eq_ignore_ascii_case("subcommand")
}

/// A usage line is a pure dispatch form when, after the binary path, its only
/// non-option token is the subcommand placeholder (`zoxide <COMMAND>`). A real
/// positional (`bat [OPTIONS] [FILE]...`) makes the command runnable on its own.
fn is_dispatch_form(usage_line: &str) -> bool {
    let mut saw_placeholder = false;
    for token in usage_line.split_whitespace() {
        if is_command_placeholder(token) {
            saw_placeholder = true;
        } else if token == "[OPTIONS]" {
            continue;
        } else if token.starts_with('<') || token.starts_with('[') {
            return false;
        }
    }
    saw_placeholder
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut section = Section::Preamble;
    let mut preamble: Vec<String> = Vec::new();
    let mut usage_lines: Vec<String> = Vec::new();
    let mut args: Vec<SpecArg> = Vec::new();
    let mut flags: Vec<SpecFlag> = Vec::new();
    // Commands are collected as (name, raw help, aliases) and built after their
    // help is fully assembled and annotations stripped.
    let mut commands: Vec<(String, Vec<String>, Option<String>)> = Vec::new();

    for line in content.lines() {
        let is_col0 = !line.is_empty() && !line.starts_with(char::is_whitespace);

        if is_col0 {
            let stripped = line.trim_end();
            // Inline usage: "Usage: <bin> [OPTIONS] <COMMAND>".
            if stripped.len() >= 6 && stripped[..6].eq_ignore_ascii_case("usage:") {
                section = Section::Usage;
                let rest = stripped[6..].trim();
                if !rest.is_empty() {
                    usage_lines.push(rest.to_string());
                }
                continue;
            }
            if let Some(new_section) = detect_header(stripped) {
                section = new_section;
                continue;
            }
            // An unrecognised column-0 line ends the structured sections; while
            // still in the preamble it is just more preamble.
            if section != Section::Preamble {
                section = Section::Other;
                continue;
            }
        }

        match section {
            Section::Preamble => preamble.push(line.trim().to_string()),
            Section::Usage => {
                let t = line.trim();
                if !t.is_empty() {
                    usage_lines.push(t.to_string());
                }
            }
            Section::Arguments => {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                if t.starts_with('<') || t.starts_with('[') {
                    let (def, desc) = split_def_and_description(t);
                    let token = def.split_whitespace().next().unwrap_or(&def);
                    let mut arg = SpecArg::builder().name(metavar_name(token)).build();
                    arg.required = token.starts_with('<');
                    arg.var = token.contains("...");
                    arg.help = desc;
                    args.push(arg);
                } else if let Some(last) = args.last_mut() {
                    append_help(&mut last.help, t);
                }
            }
            Section::Options => {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                if t.starts_with('-') {
                    let (def, desc) = split_def_and_description(t);
                    if let Some(mut flag) = parse_flag_def(&def) {
                        flag.help = desc;
                        flags.push(flag);
                    }
                } else if let Some(last) = flags.last_mut() {
                    append_help(&mut last.help, t);
                }
            }
            Section::Commands => {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                let (def, desc) = split_def_and_description(t);
                match desc {
                    Some(d) => {
                        let Some((name, aliases)) = split_command_names(&def) else {
                            continue;
                        };
                        // The auto-generated `help` subcommand is a clap artifact,
                        // never a real workflow — keep it out of the tree.
                        if name == "help" && d.starts_with("Print this message") {
                            continue;
                        }
                        commands.push((name, aliases, Some(d)));
                    }
                    None => {
                        if let Some((_, _, last)) = commands.last_mut() {
                            append_help(last, &def);
                        }
                    }
                }
            }
            Section::Other => {}
        }
    }

    for flag in &mut flags {
        enrich_flag(flag);
    }
    for arg in &mut args {
        enrich_arg(arg);
    }

    let subcommands: Vec<SpecCommand> = commands
        .into_iter()
        .map(|(name, mut aliases, raw_help)| {
            let mut builder = SpecCommand::builder().name(name);
            if let Some(help) = raw_help {
                let (clean, anns) = extract_annotations(&help);
                if !clean.is_empty() {
                    builder = builder.help(clean);
                }
                // Aliases reach us two ways: inline in the command row (`build, b`)
                // and as a help annotation (`[aliases: b]`). Keep both.
                aliases.extend(ann_aliases(&anns));
            }
            builder.aliases(aliases).build()
        })
        .collect();

    // Clap's `{about}` sits at the end of the preamble, right before "Usage:" —
    // even when a custom template prepends name/version/author (zoxide). Take the
    // last non-empty preamble line as the short help.
    let preamble_nonempty: Vec<&String> = preamble.iter().filter(|l| !l.is_empty()).collect();
    let help = preamble_nonempty.last().map(|l| l.to_string());
    let help_long = (preamble_nonempty.len() > 1).then(|| {
        preamble_nonempty
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    });

    let bin = usage_lines
        .first()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("")
        .to_string();

    let has_commands = !subcommands.is_empty();
    // A command is runnable when it offers a usage form that isn't pure subcommand
    // dispatch (`bat [OPTIONS] [FILE]...` alongside `bat <COMMAND>`). A group whose
    // only form is `<COMMAND>` requires a subcommand.
    let run_form = usage_lines.iter().any(|l| !is_dispatch_form(l));
    let subcommand_required = has_commands && !run_form;

    let mut cmd_builder = SpecCommand::builder().name(bin.clone());
    if let Some(h) = &help {
        cmd_builder = cmd_builder.help(h.clone());
    }
    if let Some(h) = &help_long {
        cmd_builder = cmd_builder.help_long(h.clone());
    }
    cmd_builder = cmd_builder.flags(flags);
    cmd_builder = cmd_builder.args(args);
    cmd_builder = cmd_builder.subcommands(subcommands);

    let mut spec = Spec::default();
    spec.name = bin.clone();
    spec.bin = bin;
    spec.cmd = cmd_builder.build();
    spec.cmd.subcommand_required = subcommand_required;
    if let Some(h) = help {
        spec.about = Some(h);
    }
    Ok(spec)
}
