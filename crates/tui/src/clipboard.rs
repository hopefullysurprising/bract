//! Copy the built command to the system clipboard so re-running it is a single
//! paste. Two layers, because neither alone covers every case:
//!
//! - A native clipboard tool (`pbcopy`/`clip`/`wl-copy`/`xclip`/`xsel`) sets the
//!   *local* clipboard reliably — including in terminals like iTerm2 that gate
//!   the OSC 52 escape behind an off-by-default preference.
//! - The OSC 52 escape sequence asks the *terminal* to set the clipboard, which
//!   is what reaches your laptop when bract runs over SSH (where a native tool
//!   would set the remote machine's clipboard, or not exist at all).
//!
//! We emit OSC 52 and then try a native tool, reporting which (if any) we could
//! actually confirm — so the hint never claims a copy that didn't happen.

use std::io::Write;
use std::process::{Command, Stdio};

const ESC: char = '\x1b';
const BEL: char = '\x07';

pub enum CopyOutcome {
    /// A native tool set the local clipboard — definitely worked.
    Confirmed,
    /// Only OSC 52 was emitted; it works if the terminal honors it (and is how
    /// SSH sessions reach the local clipboard), but we can't confirm it.
    BestEffort,
    /// Copying was turned off via `--no-clipboard`.
    Disabled,
}

/// Copy `line` to the clipboard via OSC 52 plus a native tool, returning what we
/// could confirm.
pub fn copy_command(line: &str, enabled: bool) -> CopyOutcome {
    if !enabled {
        return CopyOutcome::Disabled;
    }
    // OSC 52 first (cheap, and the only thing that reaches a local terminal over
    // SSH); then a native tool, which wins locally and is what we can verify.
    let tmux = std::env::var_os("TMUX").is_some();
    let seq = osc52_sequence(line, tmux);
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();

    if native_copy(line) {
        CopyOutcome::Confirmed
    } else {
        CopyOutcome::BestEffort
    }
}

/// Pipe `line` into the first available platform clipboard tool. Returns true
/// only if a tool ran and exited cleanly.
fn native_copy(line: &str) -> bool {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    candidates.iter().any(|(cmd, args)| pipe_into(cmd, args, line))
}

fn pipe_into(cmd: &str, args: &[&str], line: &str) -> bool {
    let Ok(mut child) = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take()
        && stdin.write_all(line.as_bytes()).is_err()
    {
        return false;
    }
    // `stdin` dropped above closes the pipe, so the tool sees EOF and exits.
    matches!(child.wait(), Ok(status) if status.success())
}

/// Build the OSC 52 "set clipboard" sequence for `text`. When `tmux` is true,
/// wrap it in tmux's DCS passthrough (doubling inner ESCs) so it reaches the
/// outer terminal rather than being swallowed by tmux.
fn osc52_sequence(text: &str, tmux: bool) -> String {
    let payload = base64_encode(text.as_bytes());
    let inner = format!("{ESC}]52;c;{payload}{BEL}");
    if tmux {
        let escaped = inner.replace(ESC, "\x1b\x1b");
        format!("{ESC}Ptmux;{escaped}{ESC}\\")
    } else {
        inner
    }
}

/// Standard base64 (RFC 4648) with `=` padding — OSC 52 carries the clipboard
/// text base64-encoded.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 0x3f) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 test vectors.
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input.as_bytes()), expected, "base64({input:?})");
        }
    }

    #[test]
    fn plain_sequence_wraps_clipboard_payload() {
        let seq = osc52_sequence("foobar", false);
        assert_eq!(seq, "\x1b]52;c;Zm9vYmFy\x07");
    }

    #[test]
    fn tmux_sequence_uses_passthrough_with_doubled_escapes() {
        let seq = osc52_sequence("foobar", true);
        // ESC P tmux ; <inner, ESCs doubled> ESC backslash
        assert_eq!(seq, "\x1bPtmux;\x1b\x1b]52;c;Zm9vYmFy\x07\x1b\\");
    }

    #[test]
    fn payload_encodes_the_exact_command() {
        let cmd = "kubectl create deployment web --image=nginx";
        let seq = osc52_sequence(cmd, false);
        let expected = format!("\x1b]52;c;{}\x07", base64_encode(cmd.as_bytes()));
        assert_eq!(seq, expected);
    }
}
