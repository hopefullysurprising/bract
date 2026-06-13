use crate::error::ParseError;
use usage::{Spec, SpecArg, SpecCommand, SpecFlag};

// Knack (the framework behind Azure CLI `az`) renders help in fixed sections:
//
//     Group | Command
//         az <path> : <description>
//     Subgroups:
//         <name> [<tag>] : <description>
//     Commands:
//         <name> : <description>
//     Arguments
//         --long --alias -s [Required] : <description>
//     Global Arguments
//         ...
//     Examples
//         ...
//
// `Group` headers describe a branch (a subcommand is required to run it);
// `Command` headers describe a runnable leaf. We carry that distinction onto
// each child via `SpecCommand::subcommand_required` (subgroups require one,
// commands do not), so the discovery layer knows a child's expandability
// without fetching its help — which is what makes lazy loading of a CLI the
// size of `az` tractable.

#[derive(PartialEq)]
enum Section {
    Preamble,
    Header,
    Subgroups,
    Commands,
    Arguments { global: bool },
    Ignored,
}

const ITEM_INDENT: usize = 4;

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn detect_section(line: &str) -> Option<Section> {
    match line.trim_end() {
        "Group" | "Command" => Some(Section::Header),
        "Subgroups:" => Some(Section::Subgroups),
        "Commands:" => Some(Section::Commands),
        "Arguments" => Some(Section::Arguments { global: false }),
        "Examples" => Some(Section::Ignored),
        s if s.ends_with("Arguments") && leading_spaces(line) == 0 => {
            Some(Section::Arguments { global: s.starts_with("Global") })
        }
        _ => None,
    }
}

/// Split an entry line into its name part and description on the aligned `:`
/// separator. The name part never contains `" : "`, so the first occurrence is
/// always the column separator (descriptions like `http://x` or `Default: y`
/// have no space before the colon and are left intact).
fn split_name_desc(line: &str) -> Option<(String, String)> {
    let idx = line.find(" : ")?;
    let name = line[..idx].trim().to_string();
    let desc = line[idx + 3..].trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some((name, desc))
}

fn strip_tags(name_part: &str) -> &str {
    // `compute-fleet [Preview]` / `config   [Experimental]` -> the leading token.
    name_part.split_whitespace().next().unwrap_or(name_part)
}

fn is_continuation(line: &str) -> bool {
    !line.trim().is_empty() && leading_spaces(line) > ITEM_INDENT
}

fn parse_arg_line(line: &str, global: bool) -> Option<SpecFlag> {
    let (name_part, help) = split_name_desc(line)?;

    let mut short = Vec::new();
    let mut long = Vec::new();
    let mut required = false;
    for token in name_part.split_whitespace() {
        if token == "[Required]" {
            required = true;
        } else if token.starts_with('[') {
            // status tag such as [Deprecated]; ignore
        } else if let Some(l) = token.strip_prefix("--") {
            long.push(l.to_string());
        } else if let Some(s) = token.strip_prefix('-')
            && let Some(c) = s.chars().next() {
                short.push(c);
            }
    }

    let name = long.first().cloned().or_else(|| short.first().map(|c| c.to_string()))?;

    let mut flag = SpecFlag::builder().name(name);
    for s in &short {
        flag = flag.short(*s);
    }
    for l in &long {
        flag = flag.long(l.clone());
    }
    if !help.is_empty() {
        flag = flag.help(help);
    }
    // Knack help never shows a metavar, so we cannot tell store_true flags from
    // value flags. Treat every option as value-taking (a text field) — the common
    // case for `az`. Inline choices/default are recovered in a finalisation pass,
    // once any wrapped continuation lines have been stitched onto the help text.
    let arg = SpecArg::builder().name(long.first().cloned().unwrap_or_else(|| "value".into())).build();
    flag = flag.arg(arg);

    let mut built = flag.build();
    built.global = global;
    built.required = required;
    Some(built)
}

/// Recover inline `Allowed values:` / `Default:` metadata from a flag's fully
/// assembled help text. Must run after continuation lines are joined.
fn enrich_flag(flag: &mut SpecFlag) {
    let Some(help) = flag.help.clone() else { return };
    if let Some(choices) = extract_allowed_values(&help)
        && let Some(arg) = flag.arg.as_mut() {
            arg.choices = Some(usage::SpecChoices { choices });
        }
    if let Some(default) = extract_default(&help) {
        flag.default = vec![default];
    }
}

fn extract_allowed_values(help: &str) -> Option<Vec<String>> {
    let start = help.find("Allowed values:")? + "Allowed values:".len();
    let rest = &help[start..];
    let end = rest.find(". ").map(|e| e + 1).unwrap_or(rest.len());
    let list = &rest[..end];
    let values: Vec<String> = list
        .split(',')
        .map(|v| v.trim().trim_end_matches('.').trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    (!values.is_empty()).then_some(values)
}

fn extract_default(help: &str) -> Option<String> {
    let start = help.find("Default:")? + "Default:".len();
    let rest = help[start..].trim_start();
    let value = rest.split_whitespace().next()?.trim_end_matches('.');
    (!value.is_empty()).then(|| value.to_string())
}

struct Child {
    name: String,
    help: String,
    is_group: bool,
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut section = Section::Preamble;
    let mut is_group = false;

    let mut header_lines: Vec<String> = Vec::new();
    let mut children: Vec<Child> = Vec::new();
    let mut flags: Vec<SpecFlag> = Vec::new();

    for line in content.lines() {
        if let Some(new_section) = detect_section(line) {
            if matches!(new_section, Section::Header) {
                is_group = line.trim_end() == "Group";
            }
            section = new_section;
            continue;
        }

        match &section {
            Section::Preamble | Section::Ignored => {}
            Section::Header => {
                if !line.trim().is_empty() {
                    header_lines.push(line.trim().to_string());
                }
            }
            Section::Subgroups | Section::Commands => {
                if is_continuation(line) {
                    if let Some(last) = children.last_mut() {
                        last.help.push(' ');
                        last.help.push_str(line.trim());
                    }
                } else if let Some((name_part, help)) = split_name_desc(line) {
                    children.push(Child {
                        name: strip_tags(&name_part).to_string(),
                        help,
                        is_group: matches!(section, Section::Subgroups),
                    });
                }
            }
            Section::Arguments { global } => {
                if is_continuation(line) {
                    if let Some(last) = flags.last_mut()
                        && let Some(help) = last.help.as_mut() {
                            help.push(' ');
                            help.push_str(line.trim());
                        }
                } else if let Some(flag) = parse_arg_line(line, *global) {
                    flags.push(flag);
                }
            }
        }
    }

    for flag in &mut flags {
        enrich_flag(flag);
    }

    let (path, description) = parse_header(&header_lines);
    let name = path.last().cloned().unwrap_or_else(|| "az".to_string());

    let subcommands: Vec<SpecCommand> = children
        .into_iter()
        .map(|child| {
            let mut cmd = SpecCommand::builder().name(child.name);
            if !child.help.is_empty() {
                cmd = cmd.help(child.help);
            }
            let mut built = cmd.build();
            built.subcommand_required = child.is_group;
            built
        })
        .collect();

    let mut cmd_builder = SpecCommand::builder().name(name.clone());
    if !description.is_empty() {
        cmd_builder = cmd_builder.help(description.clone());
    }
    cmd_builder = cmd_builder.flags(flags);
    cmd_builder = cmd_builder.subcommands(subcommands);
    let mut cmd = cmd_builder.build();
    cmd.subcommand_required = is_group;

    let mut spec = Spec::default();
    spec.name = name.clone();
    spec.bin = path.first().cloned().unwrap_or(name);
    spec.cmd = cmd;
    if !description.is_empty() {
        spec.about = Some(description);
    }
    Ok(spec)
}

/// Parse the header block (`az <path> : <description>`, possibly wrapped) into
/// the command path segments and the joined description.
fn parse_header(lines: &[String]) -> (Vec<String>, String) {
    if lines.is_empty() {
        return (vec![], String::new());
    }
    let first = &lines[0];
    let after_az = first.strip_prefix("az").map(|s| s.trim()).unwrap_or(first);

    let (path_part, mut description) = match after_az.find(" : ") {
        Some(idx) => (after_az[..idx].trim(), after_az[idx + 3..].trim().to_string()),
        None => (after_az.trim(), String::new()),
    };

    let mut path: Vec<String> = vec!["az".to_string()];
    path.extend(path_part.split_whitespace().map(|s| s.to_string()));

    for cont in &lines[1..] {
        if !description.is_empty() {
            description.push(' ');
        }
        description.push_str(cont.trim());
    }

    (path, description)
}
