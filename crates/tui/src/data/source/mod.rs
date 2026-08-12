mod dispatcher;
mod fingerprint;
mod go_buildinfo;
pub mod help_cache;
mod python_introspect;
mod rust_clap_introspect;
pub mod mise_tasks;
pub mod mise_tools;
pub mod usage_source;

use helptext_parser::{SpecArg, SpecFlag};

use crate::data::node::{Arg, Flag, FlagKind, Node};

/// The details fetched for a single node from one `--help` invocation: the
/// node's own description, flags/args, child nodes, and whether the node is
/// itself directly runnable (false for a pure group that only dispatches to
/// subcommands).
pub struct Loaded {
    pub description: String,
    pub runnable: bool,
    pub flags: Vec<Flag>,
    pub args: Vec<Arg>,
    pub children: Vec<Node>,
}

/// A discovered tool. Sources are lazy: nothing is fetched until
/// [`Source::load_children`] is called for a given command path, and the result
/// is cached in the navigation tree. `load_children(&[])` returns the tool's
/// top-level commands.
///
/// Sources run on a background thread, hence `Send + Sync`.
pub trait Source: Send + Sync {
    fn tool_id(&self) -> &str;
    fn tool_name(&self) -> &str;
    fn tool_bin(&self) -> Vec<String>;
    fn tool_path_separator(&self) -> &str {
        " "
    }
    /// Fetch the details for the node at `command_path` (`&[]` = the tool's top
    /// level): its flags/args and its child nodes.
    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>>;

    /// Whether [`load`] for this path would be served entirely from cache (no
    /// subprocess). When true the UI resolves it synchronously instead of on the
    /// background thread. Defaults to false for sources without a cache.
    fn cached(&self, _command_path: &[String]) -> bool {
        false
    }
}

pub trait HelpProvider: Send + Sync {
    fn fetch_help(
        &self,
        binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>>;

    /// Whether [`fetch_help`] for this path is already on disk. Defaults to false
    /// (no cache); the caching decorator overrides it.
    fn is_cached(&self, _binary: &str, _subcommand_path: &[&str]) -> bool {
        false
    }
}

pub fn discover_sources() -> Vec<Box<dyn Source>> {
    // The Mise kernel leads: Tasks, then Mise's own CLI, then the toolchain.
    let mut sources: Vec<Box<dyn Source>> = vec![
        Box::new(mise_tasks::MiseTasksSource),
        Box::new(usage_source::UsageSpecSource::mise_self()),
    ];
    sources.extend(mise_tools::discover_sources());
    sources
}

pub(crate) fn convert_flags(spec_flags: &[SpecFlag]) -> Vec<Flag> {
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

pub(crate) fn convert_args(spec_args: &[SpecArg]) -> Vec<Arg> {
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
