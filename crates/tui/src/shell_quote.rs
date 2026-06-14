//! Render a built command's argv as a single, re-runnable shell line. bract
//! execs commands via argv (no shell), so this quoting is only for what we
//! *show* and *copy* — but it must be correct, since the user pastes it back
//! into a shell. Uses POSIX single-quote escaping, understood by bash/zsh/fish.

/// Join `tokens` (program first, then arguments) into one shell-safe line.
pub fn quote_command(tokens: &[String]) -> String {
    tokens.iter().map(|t| quote_token(t)).collect::<Vec<_>>().join(" ")
}

fn quote_token(token: &str) -> String {
    if !token.is_empty() && token.chars().all(is_safe) {
        return token.to_string();
    }
    // Wrap in single quotes; a literal `'` is closed, escaped, and reopened.
    let mut out = String::with_capacity(token.len() + 2);
    out.push('\'');
    for c in token.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Characters that never need quoting in a POSIX shell. Anything else (spaces,
/// globs, `$`, quotes, `;`, `~`, …) forces the token to be quoted so it's passed
/// through literally.
fn is_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ',')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(tokens: &[&str]) -> String {
        quote_command(&tokens.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn leaves_plain_tokens_unquoted() {
        assert_eq!(line(&["kubectl", "create", "--image=nginx", "-o", "yaml"]),
            "kubectl create --image=nginx -o yaml");
    }

    #[test]
    fn quotes_tokens_with_spaces() {
        assert_eq!(line(&["git", "commit", "-m", "hello world"]), "git commit -m 'hello world'");
    }

    #[test]
    fn escapes_embedded_single_quotes() {
        assert_eq!(line(&["echo", "it's"]), r"echo 'it'\''s'");
    }

    #[test]
    fn quotes_shell_metacharacters() {
        assert_eq!(line(&["x", "a;b|c", "$HOME", "*.rs"]), "x 'a;b|c' '$HOME' '*.rs'");
    }

    #[test]
    fn empty_token_becomes_empty_quotes() {
        assert_eq!(line(&["cmd", ""]), "cmd ''");
    }

    #[test]
    fn keeps_common_value_punctuation_unquoted() {
        // path-/url-/csv-like values stay readable rather than over-quoted.
        assert_eq!(line(&["x", "a/b:c", "k=v,w", "user@host"]), "x a/b:c k=v,w user@host");
    }
}
