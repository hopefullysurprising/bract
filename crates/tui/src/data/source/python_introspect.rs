//! Framework detection for Python CLIs, mirroring `go_buildinfo` for Go binaries.
//!
//! A Python entry point is a small text script with a `#!.../python` shebang. We
//! follow the shebang to the interpreter, locate its virtualenv `site-packages`,
//! and look for a known CLI framework package. Azure CLI (`az`) ships `knack`
//! (github.com/microsoft/knack), so its presence classifies the tool as Knack.
//! argparse ships with Python itself, so no package marks it: the tool's own entry
//! module has to be read for it.

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
    let module = entry_module(binary_path)?;
    reaches_argparse(&site_packages, &module).then_some(InputFormat::ArgparseHelptext)
}

/// The module a generated entry point imports its function from: pip, uv and
/// Homebrew all write `from ansible.cli.galaxy import main`.
fn entry_module(binary_path: &Path) -> Option<String> {
    let file = fs::File::open(binary_path).ok()?;
    BufReader::new(file).lines().take(WRAPPER_LINES).map_while(Result::ok).find_map(|line| {
        let (module, _) = line.trim().strip_prefix("from ")?.split_once(" import ")?;
        (module != "__future__").then(|| module.trim().to_string())
    })
}

/// Help from these is not argparse's, even where argparse is imported beside them.
const OTHER_FRAMEWORKS: [&str; 4] = ["click", "typer", "cleo", "docopt"];

/// How many imports away from the entry module argparse may sit: ansible's entry
/// modules reach it through `ansible.cli.arguments.option_helpers`, one away.
const IMPORT_HOPS: usize = 2;

/// Whether the entry module, or a module of its own package that it imports, imports
/// argparse while none of them imports another framework. Only what the entry module
/// reaches counts: a package can carry an argparse script beside a Click CLI.
fn reaches_argparse(site_packages: &Path, entry: &str) -> bool {
    let package = entry.split('.').next().unwrap_or(entry);
    let mut seen = vec![entry.to_string()];
    let mut frontier = vec![entry.to_string()];
    let mut argparse = false;
    for _ in 0..=IMPORT_HOPS {
        let mut next = Vec::new();
        for module in &frontier {
            let Some(source) = module_source(site_packages, module) else { continue };
            for imported in imports(&source) {
                let root = imported.split('.').next().unwrap_or(&imported);
                if OTHER_FRAMEWORKS.contains(&root) {
                    return false;
                }
                argparse |= root == "argparse";
                if root == package && !seen.contains(&imported) {
                    seen.push(imported.clone());
                    next.push(imported);
                }
            }
        }
        if argparse {
            return true;
        }
        frontier = next;
    }
    false
}

fn module_source(site_packages: &Path, module: &str) -> Option<String> {
    let path = module.split('.').fold(site_packages.to_path_buf(), |path, part| path.join(part));
    fs::read_to_string(path.with_extension("py")).or_else(|_| fs::read_to_string(path.join("__init__.py"))).ok()
}

/// Every module an import statement names, wherever it stands — argparse is often
/// imported inside `main`. `from a.b import c` names `a.b`, and `a.b.c` in case `c` is
/// a submodule; relative imports are not followed.
fn imports(source: &str) -> Vec<String> {
    let mut modules = Vec::new();
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("import ") {
            modules.extend(rest.split(',').filter_map(|m| m.split_whitespace().next()).map(String::from));
        } else if let Some((module, names)) = line.strip_prefix("from ").and_then(|rest| rest.split_once(" import ")) {
            let module = module.trim();
            if module.starts_with('.') {
                continue;
            }
            modules.push(module.to_string());
            let names = names.trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace());
            modules.extend(names.split(',').filter_map(|n| n.split_whitespace().next()).map(|n| format!("{module}.{n}")));
        }
    }
    modules
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

    /// An install whose entry point is the script pip writes, importing `main` from
    /// `entry`, with each module's source in site-packages.
    fn generated(entry: &str, modules: &[(&str, &str)]) -> tempfile::TempDir {
        let script = format!("#!{{venv}}/bin/python\n# -*- coding: utf-8 -*-\nimport re\nimport sys\nfrom {entry} import main\n");
        let dir = install("tool", &script, &[]);
        let site = dir.path().join("libexec/lib/python3.13/site-packages");
        for (module, source) in modules {
            let path = site.join(module);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        dir
    }

    // pre-commit and yq import argparse in the module their entry point names.
    #[test]
    fn an_entry_module_importing_argparse_is_argparse() {
        let dir = generated("tool.main", &[("tool/__init__.py", ""), ("tool/main.py", "from __future__ import annotations\n\nimport argparse\n")]);
        assert_eq!(detect(&dir, "tool"), Some(InputFormat::ArgparseHelptext));
    }

    // ansible-playbook's module imports `ansible.cli.arguments.option_helpers`, and only
    // that module imports argparse.
    #[test]
    fn argparse_one_import_away_within_the_package_is_argparse() {
        let dir = generated(
            "tool.cli.playbook",
            &[
                ("tool/cli/playbook.py", "from tool.cli.arguments import option_helpers as opt_help\n"),
                ("tool/cli/arguments/__init__.py", ""),
                ("tool/cli/arguments/option_helpers.py", "import copy\nimport argparse\n"),
            ],
        );
        assert_eq!(detect(&dir, "tool"), Some(InputFormat::ArgparseHelptext));
    }

    // Click pages are not argparse's, however much argparse a Click tool imports.
    #[test]
    fn an_entry_module_importing_click_is_not_claimed() {
        let dir = generated("tool.cli", &[("tool/cli.py", "import argparse\nimport click\n")]);
        assert_eq!(detect(&dir, "tool"), None);
    }

    // A package can carry an argparse script beside a CLI built on something else.
    #[test]
    fn argparse_the_entry_module_never_reaches_is_not_claimed() {
        let dir = generated("tool.cli", &[("tool/cli.py", "import sys\n"), ("tool/devscript.py", "import argparse\n")]);
        assert_eq!(detect(&dir, "tool"), None);
    }

    // `az` imports argparse through knack, and its pages are Knack's.
    #[test]
    fn knack_installed_beside_the_tool_wins_over_argparse() {
        let dir = generated("tool.cli", &[("tool/cli.py", "import argparse\n"), ("knack/__init__.py", "")]);
        assert_eq!(detect(&dir, "tool"), Some(InputFormat::KnackHelptext));
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
