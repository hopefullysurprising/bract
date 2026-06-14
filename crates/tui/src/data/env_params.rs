//! Environment-variable parameter provisioning. A CLI often can't take a given
//! flag from the environment (e.g. Azure CLI's `--org`); bract bridges that gap.
//!
//! Convention: `BRACT_<PATH>__<PARAM>`, where `<PATH>` is the command's tool +
//! subcommand segments and `<PARAM>` is the flag/arg name, each upper-cased with
//! non-alphanumerics folded to `_`. The `<PATH>` part is matched as a **prefix**
//! of the command being run — so `BRACT_AZ_DEVOPS__ORG` fills `--org` for every
//! command under `az devops` — and the most specific (longest) prefix wins.
//!
//! Crucially we never parse a variable name back into a path (which would be
//! ambiguous, since both separators and hyphens fold to `_`). Instead we *generate*
//! the candidate names from the path and param we already know, and look them up.

const PREFIX: &str = "BRACT_";
const BOUNDARY: &str = "__";

/// Source of environment values, abstracted so tests inject a fake instead of
/// the real process environment.
pub trait EnvSource: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}

/// The real process environment.
pub struct SystemEnv;
impl EnvSource for SystemEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// An always-empty environment — the default in tests so they never depend on
/// the machine's real env.
pub struct NullEnv;
impl EnvSource for NullEnv {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

pub struct EnvValue {
    pub value: String,
    /// The variable that supplied it, shown in the form for transparency.
    pub var_name: String,
}

/// Look for a value for `param` on the command at `full_path` (tool first, e.g.
/// `["az", "devops", "project"]`). Tries the full path, then successively
/// shorter prefixes down to the tool alone, returning the most specific match.
pub fn resolve(env: &dyn EnvSource, full_path: &[String], param: &str) -> Option<EnvValue> {
    let param_canon = canon(param);
    if param_canon.is_empty() || full_path.is_empty() {
        return None;
    }
    for len in (1..=full_path.len()).rev() {
        let path_canon =
            full_path[..len].iter().map(|s| canon(s)).collect::<Vec<_>>().join("_");
        if path_canon.is_empty() {
            continue;
        }
        let var_name = format!("{PREFIX}{path_canon}{BOUNDARY}{param_canon}");
        if let Some(value) = env.get(&var_name) {
            return Some(EnvValue { value, var_name });
        }
    }
    None
}

/// Interpret an env-var value as a boolean, for toggle flags. `1`/`true`/`yes`/
/// `on` (any case) are on; everything else — including `0`/`false`/empty — is off.
pub fn is_truthy(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Fold a path segment or param name to the env-var alphabet: upper-case ASCII
/// alphanumerics kept, `-`/`_` become `_` (runs collapsed, ends trimmed),
/// everything else (brackets, angle brackets) dropped. So `<NAME>` → `NAME`,
/// `--field-manager` → `FIELD_MANAGER`, `--org` → `ORG`.
fn canon(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_underscore = false;
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                if pending_underscore && !out.is_empty() {
                    out.push('_');
                }
                pending_underscore = false;
                out.push(c.to_ascii_uppercase());
            }
            '-' | '_' => pending_underscore = true,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeEnv(HashMap<String, String>);
    impl EnvSource for FakeEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }
    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }
    fn path(segs: &[&str]) -> Vec<String> {
        segs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn matches_an_exact_path_and_param() {
        let e = env(&[("BRACT_AZ_DEVOPS__ORG", "myorg")]);
        let got = resolve(&e, &path(&["az", "devops"]), "--org").unwrap();
        assert_eq!(got.value, "myorg");
        assert_eq!(got.var_name, "BRACT_AZ_DEVOPS__ORG");
    }

    #[test]
    fn applies_to_the_whole_subtree_as_a_prefix() {
        // Set once on `az devops`; it fills `org` for a deeper descendant.
        let e = env(&[("BRACT_AZ_DEVOPS__ORG", "myorg")]);
        let got = resolve(&e, &path(&["az", "devops", "project", "create"]), "org");
        assert_eq!(got.unwrap().value, "myorg");
    }

    #[test]
    fn the_most_specific_prefix_wins() {
        let e = env(&[
            ("BRACT_AZ__ORG", "tool-wide"),
            ("BRACT_AZ_DEVOPS__ORG", "devops-specific"),
        ]);
        let got = resolve(&e, &path(&["az", "devops", "project"]), "org").unwrap();
        assert_eq!(got.value, "devops-specific");
    }

    #[test]
    fn falls_back_to_a_tool_wide_value() {
        let e = env(&[("BRACT_AZ__ORG", "tool-wide")]);
        let got = resolve(&e, &path(&["az", "devops", "project"]), "org").unwrap();
        assert_eq!(got.value, "tool-wide");
    }

    #[test]
    fn no_match_returns_none() {
        let e = env(&[("BRACT_GH_REPO__JSON", "name")]);
        assert!(resolve(&e, &path(&["az", "devops"]), "org").is_none());
    }

    #[test]
    fn normalizes_hyphenated_flags_and_bracketed_args() {
        let e = env(&[
            ("BRACT_KUBECTL_CREATE__FIELD_MANAGER", "bract"),
            ("BRACT_MANI__NAME", "web"),
        ]);
        assert_eq!(
            resolve(&e, &path(&["kubectl", "create"]), "--field-manager").unwrap().value,
            "bract"
        );
        assert_eq!(resolve(&e, &path(&["mani"]), "<NAME>").unwrap().value, "web");
    }

    #[test]
    fn a_shorter_prefix_does_not_match_a_different_sibling() {
        // BRACT_AZ_DEVOPS must not fill a command under az_account.
        let e = env(&[("BRACT_AZ_DEVOPS__ORG", "x")]);
        assert!(resolve(&e, &path(&["az", "account"]), "org").is_none());
    }

    #[test]
    fn truthiness_covers_common_forms() {
        for on in ["1", "true", "TRUE", "Yes", "on", " on "] {
            assert!(is_truthy(on), "{on:?} should be on");
        }
        for off in ["0", "false", "no", "off", "", "anything"] {
            assert!(!is_truthy(off), "{off:?} should be off");
        }
    }
}
