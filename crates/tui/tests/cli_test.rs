use std::process::Command;

/// `BRACT_NO_CLIPBOARD` is presence-based, so *any* value must leave the command
/// line parseable. Wiring it as clap's `env` instead would value-parse it as a
/// bool, and this habitual value would exit 2 with "invalid value" rather than
/// starting bract — a unit test of the flag helper cannot see that, because such a
/// rewiring bypasses the helper entirely.
#[test]
fn an_arbitrary_no_clipboard_env_value_still_starts_bract() {
    let output = Command::new(env!("CARGO_BIN_EXE_bract"))
        .env("BRACT_NO_CLIPBOARD", "1")
        .output()
        .expect("bract runs");

    // `output()` pipes stdout, so bract clears argv parsing and stops at its
    // terminal guard — reaching that guard is the proof that the parse succeeded.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("interactive terminal"),
        "expected the terminal guard, got: {stderr}"
    );
}
