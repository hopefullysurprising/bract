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
    // Section headers sit at column 0; indented lines are entries/continuations.
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    // Match case-insensitively so the many Cobra template dialects all parse:
    // standard ("Available Commands:", "Flags:"), gh's uppercase ("CORE COMMANDS",
    // "USAGE"), kubectl's grouped Title Case ("Deploy Commands:"), and rclone's
    // lowercase ("Available commands:").
    let header = line.trim_end().to_ascii_lowercase();
    match header.as_str() {
        "usage:" | "usage" => Some(Section::Usage),
        "aliases:" | "aliases" => Some(Section::Aliases),
        "examples:" | "examples" => Some(Section::Examples),
        "flags:" | "flags" | "options:" | "options" => Some(Section::Flags),
        "global flags:" | "global flags" | "inherited flags" => Some(Section::GlobalFlags),
        "help topics" | "arguments" | "learn more" | "environment variables" => Some(Section::Done),
        // Grouped flag sections (rclone: "Flags for … (flag group Copy):",
        // "Important flags useful for most commands (flag group Important):").
        // Checked before the command rule so a header that merely *mentions*
        // "commands" in prose isn't mistaken for a command group.
        s if s.starts_with("flags ") || s.contains("(flag group") => Some(Section::Flags),
        // Any header naming a command group: "Available Commands:", "CORE COMMANDS",
        // and kubectl's grouped "Basic Commands (Beginner):" / "Deploy Commands:".
        s if is_command_header(s) => Some(Section::Commands),
        _ => None,
    }
}

/// Whether a (lowercased, column-0) header names a command group — it ends in the
/// word "commands", allowing a trailing `:` and/or a parenthetical qualifier such
/// as kubectl's `Basic Commands (Beginner):`.
fn is_command_header(header: &str) -> bool {
    let base = header.strip_suffix(':').unwrap_or(header).trim_end();
    let base = match base.rfind('(') {
        Some(open) if base.ends_with(')') => base[..open].trim_end(),
        _ => base,
    };
    base.ends_with("commands")
}

/// Process one line of a flags section into `target`, supporting both standard
/// Cobra (one flag per line) and kubectl's `Options:` format (a `--flag=DEFAULT:`
/// line followed by tab-indented description lines, with blank separators).
/// Returns true when the section has ended (column-0 trailer prose like
/// `Use "tool [command] --help" …`).
fn flag_section_line(line: &str, target: &mut Vec<SpecFlag>, global: bool) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false; // blank: a separator between kubectl options, not the end
    }
    if !line.starts_with(char::is_whitespace) {
        return true; // unindented prose after the flags = trailer
    }
    if trimmed.starts_with('-') {
        if let Some(mut flag) = parse_flag_line(line) {
            flag.global = global;
            target.push(flag);
        }
    } else if let Some(last) = target.last_mut() {
        // Indented non-flag line: a wrapped / kubectl multi-line description.
        match last.help.as_mut() {
            Some(help) => {
                help.push(' ');
                help.push_str(trimmed);
            }
            None => last.help = Some(trimmed.to_string()),
        }
    }
    false
}

fn parse_flag_line(line: &str) -> Option<SpecFlag> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (def_part, help_text) = split_flag_and_description(trimmed);
    // kubectl's `Options:` format writes the definition as `--flag=DEFAULT:` with a
    // trailing colon and an inline default; drop the colon before tokenising.
    let def_part = def_part.strip_suffix(':').map(str::to_string).unwrap_or(def_part);

    let mut short = Vec::new();
    let mut long = Vec::new();
    let mut arg: Option<SpecArg> = None;
    let mut default = Vec::new();

    for token in def_part.split_whitespace() {
        if let Some(c) = token.strip_prefix('-').and_then(|s| s.strip_suffix(',')) {
            if c.len() == 1 {
                short.push(c.chars().next().unwrap());
            }
        } else if let Some(rest) = token.strip_prefix("--") {
            match rest.split_once('=') {
                // kubectl style `--flag=DEFAULT` — a value flag unless the default
                // is a bool, in which case it's a switch.
                Some((lname, dval)) => {
                    long.push(lname.to_string());
                    if dval != "true" && dval != "false" {
                        arg = Some(SpecArg::builder().name(lname.to_string()).build());
                        let cleaned = dval.trim_matches(|c| matches!(c, '\'' | '"' | '[' | ']'));
                        if !cleaned.is_empty() {
                            default.push(cleaned.to_string());
                        }
                    }
                }
                None => long.push(rest.to_string()),
            }
        } else if !token.starts_with('-') && !long.is_empty() && arg.is_none() {
            // standard cobra: a trailing metavar (`--config string`) marks a value.
            arg = Some(SpecArg::builder().name(long.last().unwrap().clone()).build());
        }
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

/// A bare UPPERCASE token (e.g. `NAME`, `TYPE`, `FILE`) — Cobra's convention for
/// a positional placeholder. Requires at least one uppercase letter so `--` and
/// short flags don't qualify; allows `_`/`-` and a trailing `...`.
fn is_metavar(token: &str) -> bool {
    let core = token.trim_end_matches("...");
    core.len() >= 2
        && core.chars().any(|c| c.is_ascii_uppercase())
        && core.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c == '-')
}

fn parse_usage_args(usage_line: &str, has_subcommands: bool) -> Vec<SpecArg> {
    // `[flags]` is never a real argument. `command`/`subcommand` are Cobra's
    // generic subcommand placeholders (`mani [command]`, `gh <command> <subcommand>`)
    // *only* on commands that have subcommands — on a leaf, `<command>` is a real
    // positional (e.g. `mani exec <command>`), so we must keep it.
    let reserved: &[&str] = if has_subcommands {
        &["flags", "options", "command", "subcommand"]
    } else {
        &["flags", "options"]
    };

    usage_line
        .split_whitespace()
        .filter_map(|token| {
            let (raw, required) = if let Some(inner) = token.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                (inner, true)
            } else if let Some(inner) = token.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                (inner, false)
            } else if is_metavar(token) {
                // Bare UPPERCASE metavar — Cobra's convention for a required
                // positional (e.g. `kubectl create deployment NAME --image=…`).
                (token, true)
            } else {
                return None;
            };
            // Strip a variadic marker so `FILE...` / `[args...]` read cleanly.
            let name = raw.trim_end_matches("...");
            if name.is_empty() || reserved.contains(&name.to_lowercase().as_str()) {
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
                if flag_section_line(line, &mut flags, false) {
                    section = Section::Done;
                }
            }
            Section::GlobalFlags => {
                if flag_section_line(line, &mut global_flags, true) {
                    section = Section::Done;
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

    /// A rule of banner characters, whether bare (`#####`) or wrapped around a
    /// title (`##### devspace run ####`). Three or more in a row is decoration, not
    /// prose — one or two could open a real sentence ("-- a note", "# comment").
    fn is_decoration(line: &str) -> bool {
        let mut chars = line.trim_start().chars();
        let Some(first) = chars.next().filter(|c| "#=*~".contains(*c)) else {
            return false;
        };
        chars.take_while(|c| *c == first).count() >= 2
    }

    // Cobra puts the description first, so the first line is the summary — unless
    // the tool decorates its help. devspace banners every subcommand with three
    // rows of `#`, which would otherwise become the command's description and
    // replace its real summary in the tree.
    let help = preamble.lines().find(|l| !is_decoration(l)).map(|l| l.to_string());
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
