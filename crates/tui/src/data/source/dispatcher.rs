//! Resolve an executable to the program that actually runs when it is invoked by
//! name, so framework detection reads the right binary.
//!
//! Most executables are what they appear to be. Some are *multi-call
//! dispatchers*: one binary serving many names, choosing behaviour from
//! `argv[0]`. `~/.cargo/bin` is thirteen symlinks to a single `rustup`, which
//! then execs a real `cargo`, a real `rustc`, or a `/bin/sh` script from the
//! active toolchain. Reading the bytes at such a symlink describes the router,
//! not the tool — every proxy would report rustup's own framework and version.
//!
//! A dispatcher picks its target at runtime, from `argv[0]` and (for rustup) the
//! toolchain active in the current directory, so no static rule can derive it.
//! Ask the dispatcher instead — the same move `mise bin-paths` makes for tool
//! directories. Only the dispatcher is asked, never the tool: nothing here runs a
//! tool's `--help` to find out what it is.
//!
//! Every decision uses only the executable in hand. Neighbouring files are never
//! consulted, so a binary classifies the same way wherever it is found.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A multi-call dispatcher, and the subcommand that maps one of its proxy names
/// to the real binary behind it.
struct Dispatcher {
    binary: &'static str,
    resolve_subcommand: &'static str,
}

static DISPATCHERS: &[Dispatcher] = &[Dispatcher { binary: "rustup", resolve_subcommand: "which" }];

/// The binary to introspect for the executable at `path`, or `None` when the name
/// leads nowhere runnable — a proxy for a component that was never installed.
///
/// Only classification follows this redirection. Execution still goes through the
/// invoked name, which is what makes the dispatcher resolve the proxy at run time.
pub fn program_for(path: &Path) -> Option<PathBuf> {
    let invoked = path.file_name()?;
    let target = std::fs::canonicalize(path).ok()?;
    match dispatcher_for(&target, invoked) {
        Some(dispatcher) => ask(&target, dispatcher, invoked),
        None => Some(target),
    }
}

/// The dispatcher `target` is, when reached under the name `invoked`.
///
/// `None` means `target` is the program itself: either a plain binary, or an
/// alias such as `tar -> bsdtar` or `slogin -> ssh`, where the target really is
/// what runs. Names carry no signal here — `tar`/`bsdtar` share nothing yet are
/// the same program, while `code-insiders`/`code` share a prefix and are not — so
/// the target's own identity is what decides.
fn dispatcher_for(target: &Path, invoked: &OsStr) -> Option<&'static Dispatcher> {
    let name = target.file_name()?;
    if name == invoked {
        return None;
    }
    DISPATCHERS.iter().find(|d| name == OsStr::new(d.binary))
}

/// Ask the dispatcher binary itself, rather than whichever one PATH would find,
/// so a proxy farm always resolves against the toolchain that owns it.
fn ask(dispatcher: &Path, spec: &Dispatcher, invoked: &OsStr) -> Option<PathBuf> {
    let output = Command::new(dispatcher)
        .arg(spec.resolve_subcommand)
        .arg(invoked)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    resolved.is_file().then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_dispatcher(invoked: &str, target: &str) -> bool {
        dispatcher_for(Path::new(target), OsStr::new(invoked)).is_some()
    }

    #[test]
    fn a_plain_binary_is_its_own_program() {
        assert!(!is_dispatcher("cargo-sweep", "/x/cargo-sweep"));
    }

    // `argv[0]` dispatch is the whole signal: same bytes, different name.
    #[test]
    fn a_known_dispatcher_reached_under_another_name_is_a_proxy() {
        assert!(is_dispatcher("cargo", "/x/rustup"));
        assert!(is_dispatcher("rust-gdb", "/x/rustup"));
    }

    // Reached under its own name, rustup is just rustup — a tool worth browsing.
    #[test]
    fn a_dispatcher_reached_under_its_own_name_is_not_a_proxy() {
        assert!(!is_dispatcher("rustup", "/x/rustup"));
    }

    // Renames and version aliases must keep classifying their target. Erring the
    // other way would silently drop ordinary tools.
    #[test]
    fn an_alias_to_a_differently_named_program_is_not_a_proxy() {
        assert!(!is_dispatcher("tar", "/x/bsdtar"));
        assert!(!is_dispatcher("slogin", "/x/ssh"));
        assert!(!is_dispatcher("python3", "/x/python3.11"));
        assert!(!is_dispatcher("claude", "/x/2.1.228"));
    }

    // An unregistered dispatcher falls back to classifying its target, which is
    // no worse than not resolving at all — and such routers are rarely built with
    // a framework we recognise, so classification drops them anyway.
    #[test]
    fn an_unregistered_dispatcher_falls_back_to_its_target() {
        assert!(!is_dispatcher("mailq", "/x/sendmail"));
        assert!(!is_dispatcher("prlctl", "/x/parallels_wrapper"));
    }
}

// Checks against the real rustup proxy farm, gated on it being installed so CI
// without it is unaffected — mirroring the gated tests in `rust_clap_introspect`.
#[cfg(all(test, unix))]
mod installed_rustup_tests {
    use super::*;

    fn cargo_bin(name: &str) -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap()).join(".cargo/bin").join(name)
    }

    #[test]
    fn cargo_resolves_past_rustup_to_the_toolchains_real_cargo() {
        let path = cargo_bin("cargo");
        if !path.exists() {
            return;
        }
        let resolved = program_for(&path).expect("cargo resolves");
        assert_eq!(resolved.file_name().unwrap(), "cargo");
        assert!(
            resolved.components().any(|c| c.as_os_str() == "toolchains"),
            "expected a toolchain binary, got {}",
            resolved.display()
        );
    }

    #[test]
    fn a_proxy_for_an_uninstalled_component_resolves_to_nothing() {
        let path = cargo_bin("rust-analyzer");
        if !path.exists() {
            return;
        }
        assert_eq!(program_for(&path), None);
    }
}
