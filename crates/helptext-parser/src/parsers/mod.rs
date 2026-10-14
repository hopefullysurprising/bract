pub mod clap_helptext;
pub mod cobra_helptext;
pub mod fang_helptext;
pub mod knack_helptext;
pub mod oclif_commands_json;
pub mod oclif_helptext;
pub mod usage_kdl;

/// A placeholder named without the usage syntax around and inside it:
/// `[<skill[@version]>]` is `skill@version`, `[CRATE[@<VER>]]...` is `CRATE@VER`.
pub(crate) fn placeholder_name(token: &str) -> String {
    token.replace("...", "").chars().filter(|c| !matches!(c, '<' | '>' | '[' | ']')).collect()
}
