//! Form-fill memory: how often each `(tool, command, field)` has been filled and
//! the last value entered. Used to float frequently-filled fields up the form and
//! to offer the previous value for one-key recall.
//!
//! Values are currently stored as **plaintext** in a `0600` db under the OS data
//! dir — the sensitive/non-sensitive split is intentionally deferred until a
//! proper release. Don't fill a field with a secret you wouldn't want on disk.
//!
//! Like [`crate::data::source::Source`], this is a trait so the form logic stays
//! pure and testable: tests use an in-memory fake, production uses redb, and a
//! null implementation is the graceful fallback when the db is unavailable.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use redb::{ReadableTable, TableDefinition};

const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("field_memory");
/// ASCII unit separator — cannot appear in a tool id, command, or field name.
const SEP: char = '\u{1f}';
/// Stored value layout: an 8-byte little-endian count, then the UTF-8 value.
const COUNT_LEN: usize = 8;

pub struct FieldStat {
    pub count: u64,
    pub last_value: Option<String>,
}

pub trait FormMemory: Send + Sync {
    /// Note that `field` was filled with `value` for `tool_id`'s `command`,
    /// incrementing its count and replacing its remembered value.
    fn record(&self, tool_id: &str, command: &str, field: &str, value: &str);
    /// Per-field counts and last values for one command, keyed by field name.
    fn stats(&self, tool_id: &str, command: &str) -> HashMap<String, FieldStat>;
}

/// Remembers nothing. The default when no db is available, and the fake the pure
/// form tests run against unless they inject their own.
pub struct NullFormMemory;

impl FormMemory for NullFormMemory {
    fn record(&self, _tool_id: &str, _command: &str, _field: &str, _value: &str) {}
    fn stats(&self, _tool_id: &str, _command: &str) -> HashMap<String, FieldStat> {
        HashMap::new()
    }
}

pub struct RedbFormMemory {
    db: redb::Database,
}

impl RedbFormMemory {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = redb::Database::create(path)?;
        // The values are plaintext, and the field/command names reveal usage —
        // keep the whole db owner-only on unix.
        restrict_to_owner(path);
        // Materialise the table so first-run reads don't error on a missing table.
        let txn = db.begin_write()?;
        txn.open_table(TABLE)?;
        txn.commit()?;
        Ok(Self { db })
    }

    fn key(tool_id: &str, command: &str, field: &str) -> String {
        format!("{tool_id}{SEP}{command}{SEP}{field}")
    }

    fn prefix(tool_id: &str, command: &str) -> String {
        format!("{tool_id}{SEP}{command}{SEP}")
    }
}

impl FormMemory for RedbFormMemory {
    fn record(&self, tool_id: &str, command: &str, field: &str, value: &str) {
        let key = Self::key(tool_id, command, field);

        // All best-effort: a write failure must never break a command run.
        let result: Result<(), Box<dyn std::error::Error>> = (|| {
            let txn = self.db.begin_write()?;
            {
                let mut table = txn.open_table(TABLE)?;
                let prev = table
                    .get(key.as_str())?
                    .map(|g| decode_count(g.value()))
                    .unwrap_or(0);
                let mut buf = (prev + 1).to_le_bytes().to_vec();
                buf.extend_from_slice(value.as_bytes());
                table.insert(key.as_str(), buf.as_slice())?;
            }
            txn.commit()?;
            Ok(())
        })();
        let _ = result;
    }

    fn stats(&self, tool_id: &str, command: &str) -> HashMap<String, FieldStat> {
        let mut out = HashMap::new();
        let prefix = Self::prefix(tool_id, command);

        let Ok(txn) = self.db.begin_read() else { return out };
        let Ok(table) = txn.open_table(TABLE) else { return out };
        let Ok(iter) = table.range::<&str>(prefix.as_str()..) else { return out };

        for entry in iter {
            let Ok((k, v)) = entry else { continue };
            let key = k.value();
            // The range is open-ended; stop once we walk past this command's keys.
            if !key.starts_with(&prefix) {
                break;
            }
            let field = key[prefix.len()..].to_string();
            let bytes = v.value();
            if bytes.len() < COUNT_LEN {
                continue;
            }
            out.insert(
                field,
                FieldStat {
                    count: decode_count(bytes),
                    last_value: String::from_utf8(bytes[COUNT_LEN..].to_vec()).ok(),
                },
            );
        }
        out
    }
}

/// The production memory: a redb store under the OS data dir. Falls back to
/// [`NullFormMemory`] if the db can't be opened, so a launch never fails over
/// form memory.
pub fn default_form_memory() -> Arc<dyn FormMemory> {
    // Opt out (demos, CI, privacy): no persistence, so field order and recall are
    // the deterministic defaults and nothing is written to disk.
    if std::env::var_os("BRACT_NO_MEMORY").is_some() {
        return Arc::new(NullFormMemory);
    }
    let path = dirs::data_dir().map(|d| d.join("bract").join("form-memory.redb"));
    if let Some(path) = path
        && let Ok(mem) = RedbFormMemory::open(&path)
    {
        return Arc::new(mem);
    }
    Arc::new(NullFormMemory)
}

fn decode_count(bytes: &[u8]) -> u64 {
    bytes
        .get(..COUNT_LEN)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0)
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(dir: &Path) -> RedbFormMemory {
        RedbFormMemory::open(&dir.join("mem.redb")).unwrap()
    }

    #[test]
    fn counts_and_last_value_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mem = memory(tmp.path());
        mem.record("kubectl", "create deployment", "--image", "nginx");
        mem.record("kubectl", "create deployment", "--image", "redis");

        let stats = mem.stats("kubectl", "create deployment");
        let image = stats.get("--image").expect("--image was recorded");
        assert_eq!(image.count, 2, "two fills counted");
        assert_eq!(image.last_value.as_deref(), Some("redis"), "latest value remembered");
    }

    #[test]
    fn frequency_distinguishes_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let mem = memory(tmp.path());
        for _ in 0..3 {
            mem.record("kubectl", "create deployment", "--image", "nginx");
        }
        mem.record("kubectl", "create deployment", "--port", "80");

        let stats = mem.stats("kubectl", "create deployment");
        assert_eq!(stats["--image"].count, 3);
        assert_eq!(stats["--port"].count, 1);
    }

    #[test]
    fn stats_are_scoped_to_one_command() {
        let tmp = tempfile::tempdir().unwrap();
        let mem = memory(tmp.path());
        mem.record("kubectl", "create deployment", "--image", "nginx");
        mem.record("kubectl", "create service", "--tcp", "80:80");

        let deploy = mem.stats("kubectl", "create deployment");
        assert!(deploy.contains_key("--image"));
        assert!(!deploy.contains_key("--tcp"), "another command's fields must not leak in");
    }

    #[test]
    fn value_survives_reopening_the_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("mem.redb");
        {
            let mem = RedbFormMemory::open(&db_path).unwrap();
            mem.record("gh", "repo view", "--json", "name");
        }
        let mem = RedbFormMemory::open(&db_path).unwrap();
        let stats = mem.stats("gh", "repo view");
        assert_eq!(stats["--json"].count, 1);
        assert_eq!(stats["--json"].last_value.as_deref(), Some("name"));
    }

    #[cfg(unix)]
    #[test]
    fn db_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mem.redb");
        let _mem = RedbFormMemory::open(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the memory db must be readable/writable only by its owner");
    }
}
