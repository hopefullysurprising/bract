use std::path::Path;

/// Detect a statically-linked Clap CLI and recover its clap version.
///
/// Rust binaries carry no dependency manifest the way Go binaries do (no
/// buildinfo, no equivalent section), so we cannot enumerate crates. What clap
/// *does* leave behind are cargo registry source paths embedded as panic /
/// `#[track_caller]` location strings in read-only data, e.g.
/// `.../clap_builder-4.6.0/src/builder/command.rs`. These:
///   - identify clap unambiguously (it links the internal `clap_builder` crate),
///   - carry the exact version, and
///   - survive symbol stripping (they live in rodata, not the symbol table).
///
/// Limitation: a build using cargo's `trim-paths` profile option (or
/// `--remap-path-prefix`) erases these paths. Such a binary reads as "not Clap"
/// rather than being misclassified — a false negative we accept, never a false
/// positive.
pub fn detect_clap_version(binary_path: &Path) -> Option<String> {
    let data = std::fs::read(binary_path).ok()?;
    find_crate_version(&data, b"clap_builder-")
}

/// Scan `data` for `<needle><semver>` and return the version string. The needle is
/// a cargo registry crate-dir prefix (`clap_builder-`); the version that follows
/// is `[0-9.]+`, requiring at least one `.` so we don't latch onto stray text.
fn find_crate_version(data: &[u8], needle: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + needle.len() <= data.len() {
        if &data[i..i + needle.len()] == needle {
            let start = i + needle.len();
            let mut end = start;
            while end < data.len() && (data[end].is_ascii_digit() || data[end] == b'.') {
                end += 1;
            }
            let ver = &data[start..end];
            if ver.first().is_some_and(u8::is_ascii_digit) && ver.contains(&b'.') {
                return std::str::from_utf8(ver).ok().map(str::to_string);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_version_from_registry_path() {
        let data = b"junk/registry/src/index-xyz/clap_builder-4.6.0/src/builder/command.rs\0more";
        assert_eq!(find_crate_version(data, b"clap_builder-").as_deref(), Some("4.6.0"));
    }

    #[test]
    fn ignores_prefix_without_a_version() {
        let data = b"see the clap_builder- docs for details";
        assert_eq!(find_crate_version(data, b"clap_builder-"), None);
    }

    #[test]
    fn absent_in_non_clap_data() {
        let data = b"github.com/spf13/cobra and other deps";
        assert_eq!(find_crate_version(data, b"clap_builder-"), None);
    }

    // --- Environment-gated checks against real mise-installed binaries ----------
    // These mirror go_buildinfo's gated tests: they only run where the tool is
    // installed, so CI without it is unaffected.

    use std::path::PathBuf;

    fn mise_install_bin(tool: &str, version: &str, bin: &str) -> PathBuf {
        let home = std::env::var("HOME").unwrap();
        PathBuf::from(home)
            .join(".local/share/mise/installs")
            .join(tool)
            .join(version)
            .join("bin")
            .join(bin)
    }

    // A clap-derived CLI (the tool that motivated Clap support) is detected, with
    // its exact clap version recovered from the embedded registry path.
    #[test]
    fn detects_clap_in_atlassian_cli() {
        let path = mise_install_bin("cargo-atlassian-cli", "0.4.2", "atlassian-cli");
        if !path.exists() {
            return;
        }
        let version = detect_clap_version(&path).expect("atlassian-cli is clap-based");
        assert!(version.starts_with("4."), "recovered clap 4.x version, got {version}");
    }

    // ripgrep dropped clap for a hand-written parser in v13 — it must NOT be
    // misclassified as Clap. The signal's precision is the whole point.
    #[test]
    fn rejects_ripgrep_which_is_not_clap() {
        let path = mise_install_bin("cargo-ripgrep", "15.1.0", "rg");
        if !path.exists() {
            return;
        }
        assert_eq!(detect_clap_version(&path), None, "ripgrep does not link clap");
    }
}
