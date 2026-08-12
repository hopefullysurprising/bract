mod classify;
pub mod direct;
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

/// The tools to browse this run: the ones named on the command line, or — when
/// none were — whatever mise makes active here.
///
/// The two modes are exclusive. Naming a tool means mise is never consulted, so
/// the Mise kernel entries do not appear either; they belong to mise's view of
/// the world, not to a tool the user pointed at.
pub fn sources_for(tools: &[String]) -> Result<Vec<Box<dyn Source>>, String> {
    if tools.is_empty() {
        Ok(discover_sources())
    } else {
        direct::sources_from(tools)
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

/// Interpret what a `--help` invocation produced.
///
/// A non-zero exit does not mean there is no help. `devspace run --help` prints
/// its full help to stdout and *then* fails, because `run` also demands an
/// argument — judging by exit status alone discards a page of usable help and
/// reports the tool as broken. What the tool printed is the better evidence; the
/// exit status only decides the case where it printed nothing.
pub(crate) fn help_from_output(
    output: std::process::Output,
) -> Result<String, Box<dyn std::error::Error>> {
    let stdout = String::from_utf8(output.stdout)?;
    if !stdout.trim().is_empty() || output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("help failed: {}", strip_ansi(stderr.trim())).into())
}

/// Drop ANSI escape sequences. A tool's failure message is shown as plain text in
/// the status line, where colour codes would render as literal junk.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI (`ESC [`) runs until a byte in `@`..=`~`; any other escape is a
        // two-character sequence.
        if chars.next() == Some('[') {
            for terminator in chars.by_ref() {
                if ('@'..='~').contains(&terminator) {
                    break;
                }
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(unix)]
pub(crate) fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(crate) fn is_executable(path: &std::path::Path) -> bool {
    path.extension().map(|ext| ext == "exe").unwrap_or(false)
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
