//! Framework detection for Node CLIs, read from the package that ships them.
//!
//! A Node CLI's launcher is a few lines of script or a symlink to one; what it
//! runs is decided by the `package.json` that declares it under `bin`. oclif
//! loads its own configuration from that file's `oclif` section, so every oclif
//! CLI has one — a dependency check would miss Shopify's, which bundles oclif
//! instead of declaring it.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub struct OclifPackage {
    pub manifest: PathBuf,
    /// How commands are joined when typed: `sf org list`, `heroku apps:create`.
    pub separator: String,
    /// Whether `@oclif/plugin-commands` ships with the CLI, and with it
    /// `commands --json`: every command in one call.
    pub lists_commands: bool,
}

impl OclifPackage {
    /// The oclif CLI that `program`, reached as `invoked`, belongs to.
    pub fn of(invoked: &OsStr, program: &Path) -> Option<Self> {
        Self::read(&owning_manifest(invoked, program)?)
    }

    pub fn read(manifest: &Path) -> Option<Self> {
        let package = read_json(manifest)?;
        let oclif = package.get("oclif")?.as_object()?;
        let plugins = oclif.get("plugins").and_then(Value::as_array);
        Some(Self {
            manifest: manifest.to_path_buf(),
            separator: oclif.get("topicSeparator").and_then(Value::as_str).unwrap_or(":").to_string(),
            lists_commands: plugins.is_some_and(|p| p.iter().any(|p| p == "@oclif/plugin-commands")),
        })
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// A string `bin` is installed under the package's own name, unscoped.
fn declares_bin(package: &Value, invoked: &OsStr) -> bool {
    let Some(invoked) = invoked.to_str() else { return false };
    match package.get("bin") {
        Some(Value::Object(bins)) => bins.contains_key(invoked),
        Some(Value::String(_)) => package
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.rsplit('/').next() == Some(invoked)),
        _ => false,
    }
}

/// The `package.json` declaring `invoked`: the nearest one above the program —
/// where npm's symlinks and oclif's standalone tarballs lead — or, for a package
/// manager's `node_modules/.bin` shim, the dependency beside it that declares it.
fn owning_manifest(invoked: &OsStr, program: &Path) -> Option<PathBuf> {
    let nearest = program.ancestors().skip(1).map(|dir| dir.join("package.json")).find(|p| p.is_file())?;
    let package = read_json(&nearest)?;
    if declares_bin(&package, invoked) {
        return Some(nearest);
    }

    let bin_dir = program.parent()?;
    let modules = bin_dir.parent()?;
    if bin_dir.file_name()? != ".bin" || modules.file_name()? != "node_modules" {
        return None;
    }
    package
        .get("dependencies")?
        .as_object()?
        .keys()
        .map(|dependency| modules.join(dependency).join("package.json"))
        .find(|manifest| read_json(manifest).is_some_and(|p| declares_bin(&p, invoked)))
}

/// npm's names for the host, as platform packages declare them in `os` and `cpu`.
fn host_platform() -> (&'static str, &'static str) {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    };
    let cpu = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    (os, cpu)
}

fn declares_host(package: &Value, field: &str, host: &str) -> bool {
    package.get(field).and_then(Value::as_array).is_some_and(|values| values.iter().any(|v| v == host))
}

/// The native binary a launcher script hands over to. Such a package ships one
/// optional dependency per platform, each declaring the `os` and `cpu` it runs
/// on; the one matching this host holds the binary under the launcher's own name.
pub fn native_binary(invoked: &OsStr, program: &Path) -> Option<PathBuf> {
    let manifest = owning_manifest(invoked, program)?;
    let root = manifest.parent()?;
    let (os, cpu) = host_platform();
    let name = invoked.to_str()?;
    read_json(&manifest)?
        .get("optionalDependencies")?
        .as_object()?
        .keys()
        .filter_map(|dependency| {
            root.ancestors().map(|dir| dir.join("node_modules").join(dependency)).find(|dir| dir.join("package.json").is_file())
        })
        .filter(|dir| read_json(&dir.join("package.json")).is_some_and(|p| declares_host(&p, "os", os) && declares_host(&p, "cpu", cpu)))
        .flat_map(|dir| [dir.join("bin").join(name), dir.join(name), dir.join("bin").join(format!("{name}.exe")), dir.join(format!("{name}.exe"))])
        .find(|candidate| candidate.is_file())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    use crate::data::source::classify;
    use helptext_parser::InputFormat;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn executable(path: &Path, content: &str) {
        write(path, content);
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn detect(launcher: &Path) -> Option<OclifPackage> {
        let (format, manifest) = classify::program_and_format(launcher)?;
        assert_eq!(format, InputFormat::OclifHelptext);
        OclifPackage::read(&manifest)
    }

    // `npm install -g` links `bin/sf` to the package's own `bin/run.js`.
    #[test]
    fn an_npm_symlink_leads_to_its_package() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("lib/node_modules/@salesforce/cli");
        write(&package.join("package.json"), r#"{"name":"@salesforce/cli","bin":{"sf":"bin/run.js"},"oclif":{"topicSeparator":" "}}"#);
        executable(&package.join("bin/run.js"), "#!/usr/bin/env node\n");
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        symlink(package.join("bin/run.js"), dir.path().join("bin/sf")).unwrap();

        let found = detect(&dir.path().join("bin/sf")).expect("sf is an oclif CLI");
        assert_eq!(found.separator, " ");
        assert!(!found.lists_commands);
    }

    // oclif's standalone tarball puts its launcher in `bin/` of the package itself.
    #[test]
    fn a_tarball_launcher_sits_inside_its_package() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("package.json"), r#"{"name":"heroku","bin":{"heroku":"bin/run.js"},"oclif":{"plugins":["@oclif/plugin-commands"]}}"#);
        executable(&dir.path().join("bin/heroku"), "#!/usr/bin/env bash\n\"$DIR/node\" \"$DIR/run\" \"$@\"\n");

        let found = detect(&dir.path().join("bin/heroku")).expect("heroku is an oclif CLI");
        assert_eq!(found.separator, ":", "oclif's default separator");
        assert!(found.lists_commands);
    }

    // mise installs each npm tool into a wrapper package whose `node_modules/.bin`
    // holds a shell shim; the CLI is the dependency that declares the shim's name.
    #[test]
    fn a_package_manager_shim_leads_to_the_dependency_declaring_it() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("package.json"), r#"{"name":"mise-npm-install","dependencies":{"@shopify/cli":"4.8.5"}}"#);
        write(
            &dir.path().join("node_modules/@shopify/cli/package.json"),
            r#"{"name":"@shopify/cli","bin":{"shopify":"bin/run.js"},"oclif":{"topicSeparator":" "}}"#,
        );
        executable(&dir.path().join("node_modules/.bin/shopify"), "#!/bin/sh\nexec node \"$basedir/../@shopify/cli/bin/run.js\" \"$@\"\n");

        assert!(detect(&dir.path().join("node_modules/.bin/shopify")).is_some());
    }

    const CLAP: &str = "padding clap_builder-4.6.0 padding";

    fn platform_package(root: &Path, name: &str, os: &str, cpu: &str, binary: &str) {
        write(&root.join("node_modules").join(name).join("package.json"), &format!(r#"{{"name":"{name}","os":["{os}"],"cpu":["{cpu}"]}}"#));
        executable(&root.join("node_modules").join(name).join("bin").join(binary), CLAP);
    }

    fn native_launcher(root: &Path) -> PathBuf {
        write(
            &root.join("package.json"),
            r#"{"name":"search-cli","bin":{"sx":"bin/sx.js"},"optionalDependencies":{"search-cli-plan9":"1","search-cli-host":"1"}}"#,
        );
        executable(&root.join("bin/sx.js"), "#!/usr/bin/env node\n");
        symlink(root.join("bin/sx.js"), root.join("bin/sx")).unwrap();
        root.join("bin/sx")
    }

    // bx, esbuild, biome: the launcher spawns a native binary shipped by whichever
    // optional dependency matches the host. The framework is the binary's.
    #[test]
    fn a_launcher_for_a_native_binary_is_classified_from_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = native_launcher(dir.path());
        let (os, cpu) = host_platform();
        platform_package(dir.path(), "search-cli-plan9", "plan9", "mips", "sx");
        platform_package(dir.path(), "search-cli-host", os, cpu, "sx");

        let (format, program) = classify::program_and_format(&launcher).expect("the binary behind the launcher is clap");
        assert_eq!(format, InputFormat::ClapHelptext);
        assert_eq!(program, dir.path().join("node_modules/search-cli-host/bin/sx").canonicalize().unwrap());
    }

    // A platform package for another machine holds a binary that cannot run here.
    #[test]
    fn a_launcher_whose_only_binary_is_for_another_platform_is_not_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = native_launcher(dir.path());
        platform_package(dir.path(), "search-cli-plan9", "plan9", "mips", "sx");

        assert!(classify::program_and_format(&launcher).is_none());
    }

    #[test]
    fn a_node_cli_on_another_framework_is_not_claimed() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("package.json"), r#"{"name":"tool","bin":{"tool":"bin/tool.js"},"dependencies":{"commander":"12"}}"#);
        executable(&dir.path().join("bin/tool.js"), "#!/usr/bin/env node\n");

        assert!(classify::program_and_format(&dir.path().join("bin/tool.js")).is_none());
    }

    // A helper script in an oclif project's tree is not the CLI the project ships.
    #[test]
    fn a_script_its_package_does_not_declare_is_not_claimed() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("package.json"), r#"{"name":"heroku","bin":{"heroku":"bin/run.js"},"oclif":{}}"#);
        executable(&dir.path().join("scripts/release"), "#!/bin/sh\n");

        assert!(classify::program_and_format(&dir.path().join("scripts/release")).is_none());
    }
}
