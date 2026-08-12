//! Framework detection for Python CLIs, mirroring `go_buildinfo` for Go binaries.
//!
//! A Python entry point is a small text script with a `#!.../python` shebang. We
//! follow the shebang to the interpreter, locate its virtualenv `site-packages`,
//! and look for a known CLI framework package. Azure CLI (`az`) ships `knack`
//! (github.com/microsoft/knack), so its presence classifies the tool as Knack.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use helptext_parser::InputFormat;

pub fn detect_format(binary_path: &Path) -> Option<InputFormat> {
    let interpreter = python_interpreter(binary_path)?;
    let site_packages = site_packages_dir(&interpreter)?;
    if site_packages.join("knack").is_dir() {
        return Some(InputFormat::KnackHelptext);
    }
    None
}

/// How far into a script to look for an interpreter. Entry points are tiny —
/// Homebrew's `az` is two lines — so anything further in is not one.
const WRAPPER_LINES: usize = 20;

/// The Python an entry point runs under.
///
/// Usually the shebang names it: pipx and venv rewrite it to their own
/// interpreter. But an entry point may instead be a shell wrapper that names the
/// interpreter in its body — Homebrew ships `az` as `#!/usr/bin/env bash` with
/// the real invocation on the next line — so fall back to reading the script.
fn python_interpreter(binary_path: &Path) -> Option<PathBuf> {
    let file = fs::File::open(binary_path).ok()?;
    // A binary's bytes are not UTF-8, so `lines` stops and this yields nothing.
    let mut lines = BufReader::new(file).lines().take(WRAPPER_LINES).map_while(Result::ok);

    let shebang = lines.next()?.strip_prefix("#!")?.trim().to_string();
    // A bare `python3` (or `/usr/bin/env python3`) names no environment we can
    // inspect: whatever PATH resolves shares its site-packages with everything
    // else installed there, so finding a framework would say nothing about *this*
    // tool. Only an interpreter we can point at counts, wherever it is named.
    if let Some(interpreter) = shebang.split_whitespace().find_map(interpreter_path) {
        return Some(interpreter);
    }
    lines.find_map(|line| line.split_whitespace().find_map(interpreter_path))
}

/// A token naming a Python interpreter by path. Requiring the file to exist keeps
/// a passing mention out; a wrong guess costs nothing anyway, since the venv it
/// leads to simply won't hold the framework.
fn interpreter_path(token: &str) -> Option<PathBuf> {
    let token = token.trim_matches(['"', '\'']);
    if !token.contains(std::path::MAIN_SEPARATOR) {
        return None;
    }
    let path = Path::new(token);
    let name = path.file_name()?.to_str()?;
    (name.starts_with("python") && path.is_file()).then(|| path.to_path_buf())
}

/// From `<venv>/bin/python`, find `<venv>/lib/python*/site-packages`.
fn site_packages_dir(interpreter: &Path) -> Option<PathBuf> {
    let venv = interpreter.parent()?.parent()?;
    let lib = venv.join("lib");
    for entry in fs::read_dir(&lib).ok()?.flatten() {
        let candidate = entry.path().join("site-packages");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Build a Python CLI install: an interpreter, a `site-packages` holding the
    /// named packages, and an entry point whose body is `script`.
    fn install(entry_point: &str, script: &str, packages: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let venv = dir.path().join("libexec");
        fs::create_dir_all(venv.join("bin")).unwrap();
        fs::write(venv.join("bin/python"), "").unwrap();
        let site = venv.join("lib/python3.13/site-packages");
        for package in packages {
            fs::create_dir_all(site.join(package)).unwrap();
        }

        fs::create_dir_all(dir.path().join("bin")).unwrap();
        let entry = dir.path().join("bin").join(entry_point);
        fs::write(&entry, script.replace("{venv}", venv.to_str().unwrap())).unwrap();
        fs::set_permissions(&entry, fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    fn detect(dir: &tempfile::TempDir, entry_point: &str) -> Option<InputFormat> {
        detect_format(&dir.path().join("bin").join(entry_point))
    }

    // pipx and venv rewrite the shebang to their own interpreter.
    #[test]
    fn an_interpreter_named_in_the_shebang_is_followed() {
        let dir = install("sqlfluff", "#!{venv}/bin/python\n# -*- coding: utf-8 -*-\n", &["knack"]);
        assert_eq!(detect(&dir, "sqlfluff"), Some(InputFormat::KnackHelptext));
    }

    // Homebrew ships `az` as a bash wrapper that names the interpreter on line 2,
    // so a shebang-only search never sees it and the tool reads as unclassifiable.
    #[test]
    fn an_interpreter_named_in_a_wrapper_body_is_followed() {
        let dir = install(
            "az",
            "#!/usr/bin/env bash\nAZ_INSTALLER=HOMEBREW {venv}/bin/python -Im azure.cli \"$@\"\n",
            &["knack"],
        );
        assert_eq!(detect(&dir, "az"), Some(InputFormat::KnackHelptext));
    }

    #[test]
    fn a_python_cli_without_the_framework_is_not_claimed() {
        let dir = install(
            "other",
            "#!/usr/bin/env bash\n{venv}/bin/python -Im other \"$@\"\n",
            &["click"],
        );
        assert_eq!(detect(&dir, "other"), None);
    }

    // A bare `python3` names no environment we can inspect: whatever PATH resolves
    // shares its site-packages with everything else installed there, so a hit would
    // say nothing about this tool. Declining beats guessing.
    #[test]
    fn a_wrapper_naming_no_interpreter_path_is_declined() {
        let dir = install("vague", "#!/usr/bin/env bash\npython3 -Im azure.cli \"$@\"\n", &["knack"]);
        assert_eq!(detect(&dir, "vague"), None);
    }

    #[test]
    fn a_native_binary_is_not_mistaken_for_a_script() {
        let dir = install("native", "\x7fELF\x02\x01\x01\0not a script at all", &["knack"]);
        assert_eq!(detect(&dir, "native"), None);
    }
}
