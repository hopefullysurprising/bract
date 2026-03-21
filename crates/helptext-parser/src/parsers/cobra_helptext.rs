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
    match line.trim_end() {
        "Usage:" => Some(Section::Usage),
        "Aliases:" => Some(Section::Aliases),
        "Examples:" => Some(Section::Examples),
        "Available Commands:" | "Additional Commands:" => Some(Section::Commands),
        "Flags:" => Some(Section::Flags),
        "Global Flags:" => Some(Section::GlobalFlags),
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

    if let Some(help) = &help_text {
        if let Some(start) = help.rfind("(default ") {
            if let Some(end) = help[start..].find(')') {
                let val = &help[start + 9..start + end];
                default.push(val.to_string());
            }
        }
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

fn parse_usage_args(usage_line: &str) -> Vec<SpecArg> {
    const RESERVED: &[&str] = &["flags", "command"];

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
            if RESERVED.contains(&name.to_lowercase().as_str()) {
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
    Some((name_part, desc?))
}

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    let mut section = Section::Preamble;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut usage_line = String::new();
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
                    usage_line = trimmed.to_string();
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

    let bin = usage_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();

    let args = parse_usage_args(&usage_line);

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
    if !preamble.is_empty() {
        spec.about = Some(preamble);
    }
    Ok(spec)
}
