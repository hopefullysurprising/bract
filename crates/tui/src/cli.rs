use std::ffi::OsStr;

use clap::Parser;

pub const NO_CLIPBOARD_ENV: &str = "BRACT_NO_CLIPBOARD";

/// bract's own command line. `about` renders the crate description, so it stays
/// in one place (Cargo.toml).
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    /// Don't copy the built command to the system clipboard [env: BRACT_NO_CLIPBOARD]
    #[arg(long)]
    pub no_clipboard: bool,
}

impl Cli {
    pub fn clipboard_enabled(&self) -> bool {
        clipboard_enabled(self.no_clipboard, std::env::var_os(NO_CLIPBOARD_ENV).as_deref())
    }
}

/// `BRACT_NO_CLIPBOARD` is presence-based: any value — empty included — turns
/// copying off. Deliberately not wired as clap's `env`, which value-parses a bool
/// and would turn the habitual `BRACT_NO_CLIPBOARD=1` into a startup error
/// instead of a disable.
fn clipboard_enabled(no_clipboard_flag: bool, env: Option<&OsStr>) -> bool {
    !no_clipboard_flag && env.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn copying_is_on_by_default() {
        assert!(clipboard_enabled(false, None));
    }

    #[test]
    fn the_flag_turns_copying_off() {
        assert!(!clipboard_enabled(true, None));
        assert!(Cli::parse_from(["bract", "--no-clipboard"]).no_clipboard);
    }

    // The env var predates the flag and is the only way existing users have to turn
    // copying off — every value, including empty and "0", must keep disabling it.
    #[test]
    fn any_env_value_turns_copying_off() {
        for value in ["", "0", "1", "false", "banana"] {
            assert!(
                !clipboard_enabled(false, Some(OsStr::new(value))),
                "{NO_CLIPBOARD_ENV}={value:?} should disable copying"
            );
        }
    }
}
