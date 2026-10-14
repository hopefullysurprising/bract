//! Which framework built a program, and therefore how to parse its `--help`.
//!
//! Independent of how the program was found: mise enumerates a bin directory,
//! `--tool` names one on the command line, and both arrive here.

use std::path::{Path, PathBuf};

use helptext_parser::InputFormat;

use super::node_introspect::OclifPackage;
use super::{dispatcher, go_buildinfo, python_introspect, rust_clap_introspect};

/// The framework of the program at `path`, together with the file the framework
/// was read from.
///
/// Callers need that path as well as the format: it is what a fingerprint must
/// cover, and it is not the path passed in whenever a multi-call dispatcher
/// stands between the two — nor for a Node CLI, whose launcher stays byte for
/// byte the same across releases while its `package.json` does not. `None`
/// means nothing browsable is here — an unrecognised framework, or a name the
/// dispatcher cannot resolve at all.
pub fn program_and_format(path: &Path) -> Option<(InputFormat, PathBuf)> {
    // Introspect the program that actually runs. A multi-call dispatcher would
    // otherwise lend its own framework to every name it serves, and a name it
    // cannot resolve is dropped here rather than failing later at `--help`.
    let program = dispatcher::program_for(path)?;
    if let Some(format) = detect_format(&program) {
        return Some((format, program));
    }
    let package = OclifPackage::of(path.file_name()?, &program)?;
    Some((InputFormat::OclifHelptext, package.manifest))
}

/// Go binaries are introspected via buildinfo (Cobra, styled by fang where it is
/// linked); Rust binaries via clap's embedded registry-path strings (Clap); Python
/// entry points via their virtualenv's installed packages.
fn detect_format(program: &Path) -> Option<InputFormat> {
    if let Some(deps) = go_buildinfo::read_deps(program)
        && deps.iter().any(|d| d.path == "github.com/spf13/cobra") {
            let fang = deps.iter().any(|d| is_fang(&d.path));
            return Some(if fang { InputFormat::FangHelptext } else { InputFormat::CobraHelptext });
        }
    if rust_clap_introspect::detect_clap_version(program).is_some() {
        return Some(InputFormat::ClapHelptext);
    }
    python_introspect::detect_format(program)
}

/// fang's module at any major version.
fn is_fang(module: &str) -> bool {
    let base = module.rsplit_once("/v").filter(|(_, major)| major.parse::<u32>().is_ok()).map_or(module, |(base, _)| base);
    matches!(base, "github.com/charmbracelet/fang" | "charm.land/fang")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    /// A stand-in for `~/.cargo/bin`: one multi-call dispatcher carrying a clap
    /// marker of its own (as the real rustup does), reached under two names — one
    /// it can resolve to a real clap binary, one it cannot resolve at all.
    fn dispatcher_farm() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let served = dir.path().join("served-binary");
        fs::write(&served, "padding clap_builder-4.6.0 padding").expect("write served");

        let dispatcher = dir.path().join("rustup");
        fs::write(
            &dispatcher,
            format!(
                "#!/bin/sh\n# clap_builder-4.5.60\n[ \"$2\" = served ] && echo '{}' || exit 1\n",
                served.display()
            ),
        )
        .expect("write dispatcher");
        fs::set_permissions(&dispatcher, fs::Permissions::from_mode(0o755)).expect("chmod");

        symlink("rustup", dir.path().join("served")).expect("symlink served");
        symlink("rustup", dir.path().join("absent")).expect("symlink absent");
        dir
    }

    // The `rust-gdb` case: a proxy for a program that was never installed must not
    // become a browsable tool. Classifying the dispatcher's bytes instead lends it
    // rustup's own framework, and the failure only surfaces later as `help failed`.
    #[test]
    fn a_proxy_the_dispatcher_cannot_resolve_is_not_classified() {
        let dir = dispatcher_farm();
        assert_eq!(program_and_format(&dir.path().join("absent")), None);
    }

    // The `cargo` case: a resolvable proxy is classified from the program that
    // actually runs, not from the router that dispatches to it. Both markers say
    // clap, so the format alone cannot tell the two apart — assert the binary
    // reached, or this passes on the dispatcher's marker exactly as it did before
    // the resolution step existed.
    #[test]
    fn a_resolvable_proxy_is_classified_from_the_program_it_serves() {
        let dir = dispatcher_farm();
        let proxy = dir.path().join("served");
        let (format, program) = program_and_format(&proxy).expect("the proxy resolves");
        assert_eq!(program.file_name(), Some(OsStr::new("served-binary")));
        assert_eq!(format, InputFormat::ClapHelptext);
    }

    // fang moved from GitHub to charm.land at 2.0.
    #[test]
    fn fang_is_recognised_at_either_home_and_nothing_else_is() {
        assert!(is_fang("github.com/charmbracelet/fang"));
        assert!(is_fang("charm.land/fang/v2"));
        assert!(!is_fang("charm.land/lipgloss/v2"));
        assert!(!is_fang("github.com/charmbracelet/fang-extras"));
    }

    fn mise_install(path: &str) -> Option<std::path::PathBuf> {
        let path = std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".local/share/mise/installs").join(path);
        path.exists().then_some(path)
    }

    // glab 1.120 links fang 2 from charm.land; skate 1.1 links fang 0.4 from GitHub.
    #[test]
    fn a_cobra_cli_linking_fang_is_read_as_fang() {
        for path in ["glab/1.120.0/bin/glab", "skate/1.1.0/skate_1.1.0_Darwin_arm64/skate"] {
            if let Some(program) = mise_install(path) {
                assert_eq!(detect_format(&program), Some(InputFormat::FangHelptext), "{path}");
            }
        }
    }

    // ipsw 3.1.737 names fang in its go.mod, as an indirect dependency, but its
    // binary links Cobra alone.
    #[test]
    fn a_cobra_cli_not_linking_fang_stays_cobra() {
        if let Some(program) = mise_install("github-blacktop-ipsw/3.1.737/ipsw") {
            assert_eq!(detect_format(&program), Some(InputFormat::CobraHelptext));
        }
    }
}
