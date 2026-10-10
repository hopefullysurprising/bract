//! Running the programs Bract reads, with a time limit. Help comes from programs Bract
//! does not control, and one that never answers — a tool that runs a subcommand instead
//! of describing it, as `watchexec run --help` does — would hold a loader thread for the
//! rest of the session and keep a `--spec` walk from ever ending.
//!
//! On timeout the whole process group goes, not just the child: under `mise exec` the
//! child is mise, and killing it alone would leave the tool running.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one help page may take. The slowest `--help` measured, sf's, is about half a
/// second; twenty times that leaves room for a cold start.
pub const HELP_LIMIT: Duration = Duration::from_secs(10);

/// How long a whole-tree dump may take: sf's `commands --json` reads every command in
/// about three seconds.
pub const DUMP_LIMIT: Duration = Duration::from_secs(30);

/// `command`'s output, or an error once `limit` passes without it.
///
/// The output is complete when the program has closed both pipes and exited. Both pipes
/// are read as they fill, so a page longer than a pipe's buffer cannot stall the program
/// it is waiting on.
pub fn output_within(command: &mut Command, limit: Duration) -> Result<Output, Box<dyn std::error::Error>> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    let mut child = command.spawn()?;
    let deadline = Instant::now() + limit;

    let (tx, rx) = mpsc::channel();
    let pipes: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
        child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
    ];
    for (index, pipe) in pipes.into_iter().enumerate() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            let _ = tx.send((index, bytes));
        });
    }
    drop(tx);

    let mut streams: [Option<Vec<u8>>; 2] = [None, None];
    while streams.iter().any(Option::is_none) {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((index, bytes)) => streams[index] = Some(bytes),
            Err(_) => return Err(stop(&mut child, limit)),
        }
    }
    // Both pipes closed. A program that closes them and keeps running is no better
    // than one that never answers.
    loop {
        if let Some(status) = child.try_wait()? {
            let [stdout, stderr] = streams.map(Option::unwrap_or_default);
            return Ok(Output { status, stdout, stderr });
        }
        if Instant::now() >= deadline {
            return Err(stop(&mut child, limit));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn stop(child: &mut Child, limit: Duration) -> Box<dyn std::error::Error> {
    #[cfg(unix)]
    if let Ok(group) = i32::try_from(child.id()) {
        // SAFETY: kill(2) with a negative pid signals that process group and touches no
        // memory; the group is the one this child was started to lead.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    format!("no answer within {}s", limit.as_secs()).into()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn script(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("tool");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn alive(pid: &str) -> bool {
        Command::new("kill").args(["-0", pid.trim()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    // A tool that runs instead of describing itself, and starts a child of its own: both
    // must be gone when the limit passes, not just the process Bract started.
    #[test]
    fn a_program_that_never_answers_is_stopped_with_everything_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pids = dir.path().display().to_string();
        let tool = script(dir.path(), &format!("echo $$ > {pids}/shell; sleep 30 & echo $! > {pids}/child; wait"));

        let started = Instant::now();
        // Long enough for the script to have started its child even on a loaded
        // machine; well short of the sleep it would otherwise finish.
        let error = output_within(&mut Command::new(&tool), Duration::from_secs(5)).expect_err("never answers");
        assert!(started.elapsed() < Duration::from_secs(15), "stopped at the limit, not at the end of the sleep");
        assert!(error.to_string().contains("no answer"));

        std::thread::sleep(Duration::from_millis(100));
        for name in ["shell", "child"] {
            let pid = fs::read_to_string(dir.path().join(name)).unwrap();
            assert!(!alive(&pid), "{name} ({}) outlived the limit", pid.trim());
        }
    }

    // Larger than any pipe buffer: read as it fills, it never stalls the program.
    #[test]
    fn a_long_page_arrives_whole() {
        let dir = tempfile::tempdir().unwrap();
        let tool = script(dir.path(), "i=0; while [ $i -lt 20000 ]; do echo \"line $i of a long help page\"; i=$((i+1)); done; echo done >&2");

        let output = output_within(&mut Command::new(&tool), Duration::from_secs(10)).unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 20000);
        assert_eq!(String::from_utf8(output.stderr).unwrap().trim(), "done");
    }

    #[test]
    fn a_failing_program_keeps_its_exit_status() {
        let dir = tempfile::tempdir().unwrap();
        let tool = script(dir.path(), "echo partial; exit 3");
        let output = output_within(&mut Command::new(&tool), Duration::from_secs(10)).unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "partial");
    }
}
