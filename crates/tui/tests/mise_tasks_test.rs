use std::path::PathBuf;

use bract::data::node::{Children, Node};
use bract::data::source::mise_tasks::nodes_from_spec;
use helptext_parser::InputFormat;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mise-usage")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

fn collect_ids<'a>(nodes: &'a [Node], out: &mut Vec<&'a str>) {
    for node in nodes {
        out.push(&node.id);
        if let Children::Loaded(children) = &node.children {
            collect_ids(children, out);
        }
    }
}

fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Children::Loaded(children) = &node.children {
            if let Some(found) = find(children, id) {
                return Some(found);
            }
        }
    }
    None
}

fn task_nodes() -> Vec<Node> {
    let content = fixture("app_0.1.0_collisions.kdl");
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).expect("parse");
    nodes_from_spec(&spec, "mise_tasks")
}

// A mise task can be both runnable and a parent (e.g. `app:check` alongside
// `app:check:be`). The tree must not emit two nodes with the same id — that is
// what crashed the old tree widget on real task sets.
#[test]
fn leaf_and_group_collisions_produce_unique_ids() {
    let nodes = task_nodes();
    let mut ids = Vec::new();
    collect_ids(&nodes, &mut ids);
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "duplicate node ids: {ids:?}");
}

#[test]
fn runnable_parent_keeps_metadata_children_and_runnability() {
    let nodes = task_nodes();

    let check = find(&nodes, "mise_tasks/app/check").expect("app:check node");
    assert_eq!(
        check.description,
        "Run App Check locally (single origin on :8080 via service)"
    );
    assert!(check.runnable, "a task that is also a parent stays runnable");
    assert!(matches!(check.children, Children::Loaded(ref c) if !c.is_empty()));

    // `be` is a pure namespace (no `app:check:be` task), so it must not be runnable.
    let be = find(&nodes, "mise_tasks/app/check/be").expect("be namespace node");
    assert!(!be.runnable, "synthetic namespace must not be runnable");
}

#[test]
fn task_command_path_is_segmented_for_separator_joining() {
    let nodes = task_nodes();
    let cov = find(&nodes, "mise_tasks/app/check/be/test/cov").expect("deep leaf");
    assert_eq!(cov.command_path, vec!["app", "check", "be", "test", "cov"]);
}
