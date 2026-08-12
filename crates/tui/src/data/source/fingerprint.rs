//! Identify the program a cached `--help` was captured from, so the cache can
//! tell "still the same program" from "replaced since".
//!
//! The obvious keys don't survive contact with reality. A tool manager's version
//! describes the *tool*, and a bin directory can hold binaries the manager never
//! installed — `mise bin-paths rust` returns `~/.cargo/bin`, so every
//! cargo-installed binary would inherit rust's version and a real upgrade would
//! pass unnoticed. Asking the app (`--version`) costs a subprocess, and for the
//! tool with the largest tree it is the wrong way round: `az --version` takes
//! 4.7s against its own `--help` at 0.15s.
//!
//! So the program identifies itself, by its own bytes — with the whole file
//! deliberately not read. Measured over the ~120 MB reachable from `~/.cargo/bin`
//! after proxy resolution, hashing every byte costs ~0.5s and adds ~40% to
//! discovery; the cost is the read, so a faster hash does not help. Sampling the
//! head and tail costs ~6ms.
//!
//! A rebuild moves all three sampled facts: length, the header and load commands
//! at the front, and the symbol table and signature at the back. A mid-file edit
//! preserving length would be missed — an acceptable bound for a help cache, and
//! one that (unlike a timestamp) no installer can defeat by restoring metadata.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes sampled from each end. Comfortably covers a Mach-O/ELF header and its
/// load commands, and the trailing symbol table or code signature.
const SAMPLE: usize = 64 * 1024;

/// A short, filesystem-safe identity for the program at `path`, or `None` when it
/// cannot be read — in which case the caller should decline to cache rather than
/// key on something it cannot verify.
pub fn of(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();

    let sample = SAMPLE.min(len as usize);
    let mut head = vec![0u8; sample];
    file.read_exact(&mut head).ok()?;

    // Overlaps the head on files smaller than 2×SAMPLE, which costs nothing and
    // keeps the short-file case free of a special path.
    let mut tail = vec![0u8; sample];
    file.seek(SeekFrom::End(-(sample as i64))).ok()?;
    file.read_exact(&mut tail).ok()?;

    let mut hash = FNV_OFFSET;
    for byte in len.to_le_bytes().iter().chain(&head).chain(&tail) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    Some(format!("{len:x}-{hash:016x}"))
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        File::create(&path).unwrap().write_all(bytes).unwrap();
        path
    }

    #[test]
    fn the_same_bytes_fingerprint_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a", b"identical contents");
        let b = write(dir.path(), "b", b"identical contents");
        assert_eq!(of(&a), of(&b));
    }

    // What makes a second launch hit the cache rather than re-fetch: an untouched
    // program must fingerprint identically every time it is computed. Nothing here
    // may depend on when, or on how many times, `of` has run.
    #[test]
    fn recomputing_for_an_untouched_program_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "tool", b"a program that nobody has replaced");
        assert_eq!(of(&path), of(&path));
    }

    // The case the cache exists to catch: a tool replaced in place.
    #[test]
    fn rewriting_a_program_changes_its_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "tool", b"version one of the program");
        let before = of(&path).unwrap();
        write(dir.path(), "tool", b"version two of the program");
        assert_ne!(before, of(&path).unwrap());
    }

    // Same length, so the length term cannot be what distinguishes them.
    #[test]
    fn an_edit_that_preserves_length_still_changes_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "tool", b"aaaaaaaaaaaaaaaa");
        let before = of(&path).unwrap();
        write(dir.path(), "tool", b"aaaaaaaaaaaaaaab");
        assert_ne!(before, of(&path).unwrap());
    }

    #[test]
    fn a_file_shorter_than_one_sample_is_fingerprintable() {
        let dir = tempfile::tempdir().unwrap();
        // Shims are tiny: `az` is a 118-byte shell script.
        let path = write(dir.path(), "shim", b"#!/bin/sh\nexec real-tool \"$@\"\n");
        assert!(of(&path).is_some());
    }

    #[test]
    fn an_unreadable_path_has_no_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(of(&dir.path().join("absent")), None);
    }
}
