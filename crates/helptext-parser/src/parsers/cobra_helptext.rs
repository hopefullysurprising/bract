use crate::error::ParseError;
use usage::{Spec, SpecCommand, SpecFlag, SpecArg};

#[derive(Debug, PartialEq)]
enum Section {
    Preamble,
    Usage,
    Aliases,
    Examples,
    Commands,
    Flags,
    GlobalFlags,
    Done,
}

fn detect_section(line: &str) -> Option<Section> {
    let trimmed = line.trim_end();
    match trimmed {
        "Usage:" => return Some(Section::Usage),
        "Aliases:" => return Some(Section::Aliases),
        "Examples:" => return Some(Section::Examples),
        "Available Commands:" | "Additional Commands:" => return Some(Section::Commands),
        "Flags:" => return Some(Section::Flags),
        "Global Flags:" => return Some(Section::GlobalFlags),
        _ => {}
    }

    // Some Cobra tools ship a custom uppercase template (notably `gh`): section
    // headers sit at column 0 in all-caps, and command groups are named ending in
    // "COMMANDS" (CORE/GENERAL/TARGETED/ADDITIONAL/ALIAS …). Prose sections
    // (ARGUMENTS, HELP TOPICS, LEARN MORE …) are skipped.
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    match trimmed {
        "USAGE" => Some(Section::Usage),
        "FLAGS" => Some(Section::Flags),
        "INHERITED FLAGS" | "GLOBAL FLAGS" => Some(Section::GlobalFlags),
        "HELP TOPICS" | "ARGUMENTS" | "EXAMPLES" | "LEARN MORE" | "ENVIRONMENT VARIABLES" => {
            Some(Section::Done)
        }
        s if s.ends_with(" COMMANDS") => Some(Section::Commands),
        _ => None,
    }
}

fn parse_flag_line(line: &str) -> Option<SpecFlag> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (def_part, help_text) = split_flag_and_description(trimmed);

    let mut short = Vec::new();
    let mut long = Vec::new();
    let mut arg: Option<SpecArg> = None;
    let mut default = Vec::new();

    let tokens: Vec<&str> = def_part.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if let Some(c) = token.strip_prefix('-').and_then(|s| s.strip_suffix(',')) {
            if c.len() == 1 {
                short.push(c.chars().next().unwrap());
            }
        } else if let Some(name) = token.strip_prefix("--") {
            long.push(name.to_string());
        } else if !token.starts_with('-') && !long.is_empty() {
            arg = Some(SpecArg::builder()
                .name(long.last().unwrap().clone())
                .build());
        }
        i += 1;
    }

    if let Some(help) = &help_text
        && let Some(start) = help.rfind("(default ")
            && let Some(end) = help[start..].find(')') {
                let val = help[start + 9..start + end]
                    .trim_matches('"');
                default.push(val.to_string());
            }

    let clean_help = help_text.map(|h| {
        if let Some(start) = h.rfind(" (default ") {
            h[..start].to_string()
        } else {
            h
        }
    });

    if long.is_empty() {
        return None;
    }

    let name = long[0].clone();

    let mut flag = SpecFlag::builder()
        .name(name);

    for s in &short {
        flag = flag.short(*s);
    }
    for l in &long {
        flag = flag.long(l.clone());
    }
    if let Some(h) = clean_help {
        flag = flag.help(h);
    }
    if let Some(a) = arg {
        flag = flag.arg(a);
    }

    let mut built = flag.build();
    built.default = default;
    Some(built)
}

fn parse_usage_args(usage_line: &str, has_subcommands: bool) -> Vec<SpecArg> {
    // `[flags]` is never a real argument. `command`/`subcommand` are Cobra's
    // generic subcommand placeholders (`mani [command]`, `gh <command> <subcommand>`)
    // *only* on commands that have subcommands — on a leaf, `<command>` is a real
    // positional (e.g. `mani exec <command>`), so we must keep it.
    let reserved: &[&str] =
        if has_subcommands { &["flags", "command", "subcommand"] } else { &["flags"] };

    usage_line
        .split_whitespace()
        .filter_map(|token| {
            let (name, required) = if let Some(inner) = token.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                (inner, true)
            } else if let Some(inner) = token.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                (inner, false)
            } else {
                return None;
            };
            if reserved.contains(&name.to_lowercase().as_str()) {
                return None;
            }
            let mut arg = SpecArg::builder().name(name.to_string()).build();
            arg.required = required;
            Some(arg)
        })
        .collect()
}

fn split_flag_and_description(line: &str) -> (String, Option<String>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let space_start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            if i - space_start >= 2 && i < bytes.len() {
                let def = line[..space_start].to_string();
                let desc = line[i..].to_string();
                return (def, Some(desc));
            }
        } else {
            i += 1;
        }
    }
    (line.to_string(), None)
}

fn parse_command_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (name_part, desc) = split_flag_and_description(trimmed);
    // gh's template renders entries as `name:   description`; drop the trailing
    // colon so the command name matches what you actually invoke.
    let name = name_part.trim_end_matches(':').to_string();
    Some((name, desc?))
}

/// A usage line whose only non-flag tokens are the generic `command`/`subcommand`
/// placeholders is a subcommand-dispatch form (e.g. `mani describe [command]`),
/// not a way to invoke the command itself.
fn is_dispatch_form(usage_line: &str) -> bool {
    usage_line.split_whitespace().any(|token| {
        let bare = token.trim_matches(|c| c == '<' || c == '>' || c == '[' || c == ']');
        bare == "command" || bare == "subcommand"
    })
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut section = Section::Preamble;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut usage_lines: Vec<String> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut subcommands: Vec<SpecCommand> = Vec::new();
    let mut flags: Vec<SpecFlag> = Vec::new();
    let mut global_flags: Vec<SpecFlag> = Vec::new();

    for line in content.lines() {
        if let Some(new_section) = detect_section(line) {
            section = new_section;
            continue;
        }

        match section {
            Section::Preamble => {
                preamble_lines.push(line.to_string());
            }
            Section::Usage => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    usage_lines.push(trimmed.to_string());
                }
            }
            Section::Aliases => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    aliases = trimmed.split(", ").map(|s| s.to_string()).collect();
                }
            }
            Section::Examples => {}
            Section::Commands => {
                if let Some((name, desc)) = parse_command_line(line) {
                    let cmd = SpecCommand::builder()
                        .name(name)
                        .help(desc)
                        .build();
                    subcommands.push(cmd);
                }
            }
            Section::Flags => {
                if line.trim().is_empty() {
                    section = Section::Done;
                } else if let Some(flag) = parse_flag_line(line) {
                    flags.push(flag);
                }
            }
            Section::GlobalFlags => {
                if line.trim().is_empty() {
                    section = Section::Done;
                } else if let Some(mut flag) = parse_flag_line(line) {
                    flag.global = true;
                    global_flags.push(flag);
                }
            }
            Section::Done => {}
        }
    }

    let preamble = preamble_lines
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    let help = preamble.lines().next().map(|l| l.to_string());
    let help_long = if preamble.contains('\n') {
        Some(preamble.clone())
    } else {
        None
    };

    let bin = usage_lines
        .first()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("")
        .to_string();

    let has_subcommands = !subcommands.is_empty();

    // The command is a runnable command in its own right when it offers a usage
    // form that isn't just subcommand dispatch. A pure group (only `[command]`
    // forms) requires a subcommand and cannot be run directly — this is what keeps
    // groups like `mani describe`/`mani list` from rendering as runnable commands,
    // while genuinely dual commands like `mani edit` (own `[flags]` form *and*
    // subcommands) stay runnable.
    let run_form = usage_lines.iter().find(|l| !is_dispatch_form(l));
    let subcommand_required = has_subcommands && run_form.is_none();

    // Parse positional args from the run form (where real arguments live), falling
    // back to the first usage line.
    let args_line = run_form.or_else(|| usage_lines.first()).cloned().unwrap_or_default();
    let args = parse_usage_args(&args_line, has_subcommands);

    if !aliases.is_empty() {
        aliases.remove(0);
    }

    let mut all_flags = flags;
    all_flags.extend(global_flags);

    let mut cmd_builder = SpecCommand::builder();
    cmd_builder = cmd_builder.name(bin.clone());
    if let Some(h) = &help {
        cmd_builder = cmd_builder.help(h.clone());
    }
    if let Some(h) = &help_long {
        cmd_builder = cmd_builder.help_long(h.clone());
    }
    cmd_builder = cmd_builder.aliases(aliases);
    cmd_builder = cmd_builder.flags(all_flags);
    cmd_builder = cmd_builder.args(args);
    cmd_builder = cmd_builder.subcommands(subcommands);

    let mut spec = Spec::default();
    spec.name = bin.clone();
    spec.bin = bin;
    spec.cmd = cmd_builder.build();
    spec.cmd.subcommand_required = subcommand_required;
    if !preamble.is_empty() {
        spec.about = Some(preamble);
    }
    Ok(spec)
}
