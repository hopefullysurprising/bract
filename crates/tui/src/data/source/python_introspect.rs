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

/// Read the shebang and return the interpreter path if it is a Python script.
fn python_interpreter(binary_path: &Path) -> Option<PathBuf> {
    let file = fs::File::open(binary_path).ok()?;
    let mut first_line = String::new();
    BufReader::new(file).read_line(&mut first_line).ok()?;

    let shebang = first_line.strip_prefix("#!")?.trim();
    // shebang may be `/usr/bin/env python3` or a direct interpreter path.
    let interpreter = shebang.split_whitespace().next()?;
    if !interpreter.contains("python") {
        return None;
    }
    Some(PathBuf::from(interpreter))
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
