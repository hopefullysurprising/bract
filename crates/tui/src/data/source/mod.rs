pub mod mise_tasks;
pub mod mise_tools;

use std::collections::BTreeMap;

use helptext_parser::{SpecArg, SpecFlag};

use super::commands::{Arg, Command, Flag, FlagKind, Tool};

pub struct DiscoveryResult {
    pub description: String,
    pub flags: Vec<Flag>,
    pub args: Vec<Arg>,
    pub commands: Vec<Command>,
}

pub trait Source {
    fn tool_id(&self) -> &str;
    fn tool_name(&self) -> &str;
    fn tool_bin(&self) -> &str;
    fn tool_path_separator(&self) -> &str { " " }
    fn discover(&self) -> Result<DiscoveryResult, Box<dyn std::error::Error>>;
}

pub fn assemble_tools(sources: Vec<Box<dyn Source>>) -> Result<Vec<Tool>, Box<dyn std::error::Error>> {
    let mut tool_map: BTreeMap<String, Tool> = BTreeMap::new();

    for source in &sources {
        let result = source.discover()?;
        let tool = tool_map
            .entry(source.tool_id().to_string())
            .or_insert_with(|| Tool {
                id: source.tool_id().to_string(),
                name: source.tool_name().to_string(),
                bin: source.tool_bin().to_string(),
                path_separator: source.tool_path_separator().to_string(),
                description: String::new(),
                flags: vec![],
                args: vec![],
                commands: vec![],
            });
        if !result.description.is_empty() {
            tool.description = result.description;
        }
        if tool.flags.is_empty() {
            tool.flags = result.flags;
        }
        if tool.args.is_empty() {
            tool.args = result.args;
        }
        tool.commands.extend(result.commands);
    }

    Ok(tool_map.into_values().collect())
}

fn convert_flags(spec_flags: &[SpecFlag]) -> Vec<Flag> {
    spec_flags
        .iter()
        .filter(|f| !f.hide && !f.global && f.name != "help" && f.name != "version")
        .map(|f| {
            let display_name = f
                .long
                .first()
                .map(|l| format!("--{l}"))
                .or_else(|| f.short.first().map(|s| format!("-{s}")))
                .unwrap_or_else(|| f.name.clone());

            let kind = match &f.arg {
                None => FlagKind::Boolean,
                Some(arg) => FlagKind::Value {
                    arg_name: arg.name.clone(),
                    default: f.default.first().cloned().unwrap_or_default(),
                    choices: arg
                        .choices
                        .as_ref()
                        .map(|c| c.choices.clone())
                        .unwrap_or_default(),
                },
            };

            Flag {
                name: display_name,
                short: f.short.first().copied(),
                long: f.long.first().cloned(),
                description: f.help.clone().unwrap_or_default(),
                required: f.required,
                kind,
            }
        })
        .collect()
}

fn convert_args(spec_args: &[SpecArg]) -> Vec<Arg> {
    spec_args
        .iter()
        .filter(|a| !a.hide)
        .map(|a| Arg {
            name: a.name.clone(),
            description: a.help.clone().unwrap_or_default(),
            required: a.required,
            default: a.default.first().cloned().unwrap_or_default(),
            choices: a
                .choices
                .as_ref()
                .map(|c| c.choices.clone())
                .unwrap_or_default(),
        })
        .collect()
}
