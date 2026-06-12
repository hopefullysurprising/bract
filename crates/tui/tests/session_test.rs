mod common;

use common::{build_mani_tools, build_task_tools, Session};

#[test]
fn mani_sync_form_renders() {
    let tools = build_mani_tools();
    let mut session = Session::new(tools, 80, 60);
    session.select_command(&["mani", "sync"]);
    insta::assert_snapshot!(session.screen());
}

// A task that is both runnable and a parent (`eiq:analyse`) must open its form on
// Enter and run with the full colon-joined task path — not `mise run eiq analyse`.
#[test]
fn runnable_branch_task_runs_with_joined_path() {
    let tools = build_task_tools();
    let mut session = Session::new(tools, 80, 40);
    session.select_command(&["Mise Tasks", "eiq", "analyse"]);
    let spec = session.run();
    assert_eq!(spec.bin, vec!["mise", "run"]);
    assert_eq!(spec.args, vec!["eiq:analyse"]);
}

// A deeper runnable leaf confirms multi-segment paths join correctly too.
#[test]
fn deep_leaf_task_runs_with_joined_path() {
    let tools = build_task_tools();
    let mut session = Session::new(tools, 80, 40);
    session.select_command(&["Mise Tasks", "eiq", "analyse", "be", "test", "cov"]);
    let spec = session.run();
    assert_eq!(spec.args, vec!["eiq:analyse:be:test:cov"]);
}
