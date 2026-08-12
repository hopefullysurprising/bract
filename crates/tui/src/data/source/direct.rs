//! Tools named on the command line, browsed without mise.
//!
//! Mise supplies four separate things to the default mode: which tools exist,
//! where each binary lives, a wrapper to run it under, and a version to key the
//! help cache by. Here the first comes from the arguments, the second from PATH,
//! the third is dropped, and the fourth was never mise's to give — the cache keys
//! on the program's own bytes in both modes, so nothing about caching, parsing,
//! classification or the tree differs between them.
//!
//! Two paths are kept apart throughout, as the dispatcher work established:
//! **what we run** is the argument exactly as given, so the command bract prints
//! is one you could have typed; **what we introspect** is the program that
//! argument resolves to, which for a multi-call proxy is a different file
//! entirely.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{classify, fingerprint, help_cache, is_executable, mise_tools::HelpToolSource};
use super::{HelpProvider, Source};

/// Runs a tool straight from PATH. `MiseHelpProvider` without the `mise exec`
/// wrapper — same contract, so a tool's help is parsed identically in both modes.
pub struct DirectHelpProvider;

impl HelpProvider for DirectHelpProvider {
    fn fetch_help(
        &self,
        binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(binary).args(subcommand_path).arg("--help").output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("help failed: {stderr}").into());
        }

        Ok(String::from_utf8(output.stdout)?)
    }
}

/// Find the executable an argument names. An argument containing a separator is
/// a path; anything else is a bare name looked up in `path_var`, the way a shell
/// would. Taking PATH as an argument keeps the search testable without mutating
/// the process environment.
fn locate_in(arg: &str, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if arg.contains(std::path::MAIN_SEPARATOR) {
        let candidate = PathBuf::from(arg);
        return (candidate.is_file() && is_executable(&candidate)).then_some(candidate);
    }
    std::env::split_paths(path_var?)
        .map(|dir| dir.join(arg))
        .find(|candidate| candidate.is_file() && is_executable(candidate))
}

fn locate(arg: &str) -> Option<PathBuf> {
    locate_in(arg, std::env::var_os("PATH").as_deref())
}

/// Build a source per named tool, or explain which name failed and why.
///
/// Failures are reported rather than skipped: a tool the user asked for by name
/// and did not get is a mistake worth hearing about, unlike a directory sweep
/// where most entries are expected to be uninteresting.
pub fn sources_from(tools: &[String]) -> Result<Vec<Box<dyn Source>>, String> {
    let cache_dir = help_cache::default_cache_dir();
    tools.iter().map(|arg| one_source(arg, cache_dir.as_deref())).collect()
}

fn one_source(arg: &str, cache_dir: Option<&Path>) -> Result<Box<dyn Source>, String> {
    let invocation = locate(arg)
        .ok_or_else(|| format!("no executable named '{arg}' on PATH"))?;

    let (format, program) = classify::program_and_format(&invocation).ok_or_else(|| {
        format!("'{arg}' is not a CLI bract can introspect — no supported framework detected")
    })?;

    let provider: Box<dyn HelpProvider> = match (cache_dir, fingerprint::of(&program)) {
        (Some(dir), Some(fingerprint)) => Box::new(help_cache::CachingHelpProvider::new(
            Box::new(DirectHelpProvider),
            dir.to_path_buf(),
            fingerprint,
        )),
        _ => Box::new(DirectHelpProvider),
    };

    // Identified by where it was found, so naming two same-named tools at
    // different paths yields two independently browsable tools rather than a
    // collision. Displayed and run as given, so the printed command is the one the
    // user could have typed.
    let tool_id = invocation.to_string_lossy().into_owned();
    Ok(Box::new(HelpToolSource::with_tool_id(tool_id, arg.to_string(), format, provider)))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn executable(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_bare_name_is_found_on_the_path() {
        let dir = tempfile::tempdir().unwrap();
        executable(dir.path(), "mytool", "#!/bin/sh\n");
        let path_var = OsString::from(dir.path());
        assert_eq!(locate_in("mytool", Some(&path_var)).as_deref(), Some(dir.path().join("mytool").as_path()));
    }

    #[test]
    fn a_name_absent_from_the_path_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path_var = OsString::from(dir.path());
        assert_eq!(locate_in("mytool", Some(&path_var)), None);
    }

    // A non-executable file of the right name must not shadow the search.
    #[test]
    fn a_non_executable_file_is_not_a_tool() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mytool"), "not executable").unwrap();
        let path_var = OsString::from(dir.path());
        assert_eq!(locate_in("mytool", Some(&path_var)), None);
    }

    // An argument with a separator addresses a file directly, so it resolves even
    // though PATH is empty.
    #[test]
    fn an_argument_with_a_separator_is_a_path_not_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = executable(dir.path(), "mytool", "#!/bin/sh\n");
        assert_eq!(locate_in(path.to_str().unwrap(), None).as_deref(), Some(path.as_path()));
    }

    #[test]
    fn an_unknown_name_is_reported_rather_than_skipped() {
        let err = sources_from(&["definitely-not-installed-anywhere".to_string()])
            .err()
            .expect("an unknown name must not silently yield no tools");
        assert!(err.contains("definitely-not-installed-anywhere"), "names the tool: {err}");
        assert!(err.contains("PATH"), "says where it looked: {err}");
    }

    // A real executable of no recognised framework is a different failure, and must
    // read as one — silently opening an empty tree would be worse.
    #[test]
    fn an_unrecognised_framework_is_reported_distinctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = executable(dir.path(), "plain", "#!/bin/sh\necho nothing\n");
        let err = sources_from(&[path.to_string_lossy().into_owned()])
            .err()
            .expect("an unrecognised framework must not silently yield no tools");
        assert!(err.contains("framework"), "explains what was missing: {err}");
    }

    #[test]
    fn direct_help_runs_the_tool_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = executable(dir.path(), "greeter", "#!/bin/sh\necho \"help for $*\"\n");
        let help = DirectHelpProvider
            .fetch_help(path.to_str().unwrap(), &["sub"])
            .expect("help is fetched");
        assert_eq!(help.trim(), "help for sub --help");
    }

    #[test]
    fn a_failing_help_call_is_an_error_not_empty_help() {
        let dir = tempfile::tempdir().unwrap();
        let path = executable(dir.path(), "broken", "#!/bin/sh\necho 'boom' >&2\nexit 1\n");
        let err = DirectHelpProvider
            .fetch_help(path.to_str().unwrap(), &[])
            .expect_err("a non-zero exit must not read as help");
        assert!(err.to_string().contains("boom"), "surfaces the tool's own message: {err}");
    }
}

