//! Fingerprint-keyed `--help` cache. Spawning `mise exec -- tool --help` for
//! every node on every launch is the slow part of discovery; parsing is cheap.
//! This decorator caches the raw help text on disk under a key identifying the
//! program it came from, so a tool (or subtree) is re-fetched once that program
//! is replaced.
//!
//! The key is **opaque** — never parsed, so nothing here assumes what identifies
//! a program. When the caller supplies a different one the old text is simply
//! bypassed. See `fingerprint` for how one is derived, and why not from a version.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::HelpProvider;

/// Distinguishes concurrent writers inside one process. The pid alone stopped
/// being enough once fetches became a pool of threads rather than one worker.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct CachingHelpProvider {
    inner: Box<dyn HelpProvider>,
    dir: PathBuf,
    fingerprint: String,
}

impl CachingHelpProvider {
    pub fn new(inner: Box<dyn HelpProvider>, dir: PathBuf, fingerprint: String) -> Self {
        Self { inner, dir, fingerprint }
    }

    fn cache_path(&self, binary: &str, subcommand_path: &[&str]) -> PathBuf {
        let leaf = if subcommand_path.is_empty() {
            "__root".to_string()
        } else {
            subcommand_path.join("__")
        };
        self.dir
            .join(sanitize(binary))
            .join(sanitize(&self.fingerprint))
            .join(format!("{}.txt", sanitize(&leaf)))
    }
}

impl HelpProvider for CachingHelpProvider {
    fn fetch_help(
        &self,
        binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let path = self.cache_path(binary, subcommand_path);
        if let Ok(cached) = std::fs::read_to_string(&path) {
            return Ok(cached);
        }
        // Miss: fetch fresh, then persist best-effort. A failed write (read-only
        // cache dir, race) must not fail the fetch — we just don't cache it.
        let fresh = self.inner.fetch_help(binary, subcommand_path)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Publish atomically: write to a unique temp file, then rename into place.
        // A crash or interrupt mid-write must never leave a truncated help text
        // that would parse wrong (and stay wrong until the program changes).
        let tmp = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        if std::fs::write(&tmp, &fresh).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(fresh)
    }

    fn is_cached(&self, binary: &str, subcommand_path: &[&str]) -> bool {
        self.cache_path(binary, subcommand_path).exists()
    }
}

/// `~/Library/Caches/bract/helptext` (macOS), `~/.cache/bract/helptext` (Linux),
/// `%LOCALAPPDATA%\bract\helptext` (Windows). `None` if no cache dir resolves.
pub fn default_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("bract").join("helptext"))
}

/// Map a path segment to a filesystem-safe form: keep alphanumerics, `.`, `-`,
/// `_`; replace everything else (`:`, `/`, `@`, spaces in tool keys like
/// `aqua:smallstep/cli`) with `_`.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Counts how often the real fetch runs, so tests can assert cache hits.
    struct CountingProvider {
        calls: Arc<AtomicUsize>,
        response: String,
    }

    impl HelpProvider for CountingProvider {
        fn fetch_help(
            &self,
            _binary: &str,
            _path: &[&str],
        ) -> Result<String, Box<dyn std::error::Error>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.response.clone())
        }
    }

    fn caching(dir: &Path, fingerprint: &str, response: &str) -> (CachingHelpProvider, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Box::new(CountingProvider { calls: calls.clone(), response: response.to_string() });
        (CachingHelpProvider::new(inner, dir.to_path_buf(), fingerprint.to_string()), calls)
    }

    #[test]
    fn second_fetch_of_the_same_program_hits_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let (provider, calls) = caching(tmp.path(), "1.0", "HELP");

        let a = provider.fetch_help("kubectl", &["create"]).unwrap();
        let b = provider.fetch_help("kubectl", &["create"]).unwrap();

        assert_eq!((a.as_str(), b.as_str()), ("HELP", "HELP"));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second fetch must come from cache");
    }

    #[test]
    fn a_replaced_program_misses_the_old_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let (before, _) = caching(tmp.path(), "1.0", "OLD");
        let first = before.fetch_help("kubectl", &[]).unwrap();
        let (after, after_calls) = caching(tmp.path(), "2.0", "NEW");
        let second = after.fetch_help("kubectl", &[]).unwrap();

        assert_eq!(first, "OLD");
        assert_eq!(second, "NEW", "a replaced program must bypass the stale cached help");
        assert_eq!(after_calls.load(Ordering::SeqCst), 1, "the replacement is fetched fresh");
    }

    #[test]
    fn atomic_write_leaves_only_the_final_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (provider, _) = caching(tmp.path(), "1.0", "HELP");
        provider.fetch_help("kubectl", &["create"]).unwrap();
        // The temp file used for the atomic publish must be renamed away, not left
        // beside the final help — a stray .tmp would never be read but shouldn't linger.
        let leaf_dir = tmp.path().join("kubectl").join("1.0");
        let entries: Vec<String> = std::fs::read_dir(&leaf_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["create.txt"], "only the renamed final file remains");
    }

    #[test]
    fn distinct_subcommands_cache_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, _) = caching(tmp.path(), "1.0", "ROOTHELP");
        let root_help = root.fetch_help("kubectl", &[]).unwrap();
        let (child, _) = caching(tmp.path(), "1.0", "CREATEHELP");
        let child_help = child.fetch_help("kubectl", &["create"]).unwrap();

        assert_eq!(root_help, "ROOTHELP");
        assert_eq!(child_help, "CREATEHELP", "subcommand path is part of the key");
    }
}
