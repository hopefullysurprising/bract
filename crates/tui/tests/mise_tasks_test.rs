use std::path::PathBuf;

use bract::data::commands::Command;
use bract::data::source::mise_tasks::commands_from_spec;
use bract::ui::browse::BrowseView;
use helptext_parser::InputFormat;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mise-usage")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

fn collect_ids<'a>(commands: &'a [Command], out: &mut Vec<&'a str>) {
    for cmd in commands {
        out.push(&cmd.id);
        collect_ids(&cmd.subcommands, out);
    }
}

fn find<'a>(commands: &'a [Command], id: &str) -> Option<&'a Command> {
    for cmd in commands {
        if cmd.id == id {
            return Some(cmd);
        }
        if let Some(found) = find(&cmd.subcommands, id) {
            return Some(found);
        }
    }
    None
}

// A mise task can be both runnable and a parent of subtasks (e.g. `eiq:analyse`
// alongside `eiq:analyse:be`). The hierarchy must not emit two nodes with the
// same id, otherwise tui-tree-widget rejects the tree with a duplicate-identifier
// error and the whole app fails to start.
#[test]
fn leaf_and_group_collisions_produce_unique_ids() {
    let content = fixture("lrqa_0.1.0_collisions.kdl");
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).expect("parse");
    let commands = commands_from_spec(&spec, "mise");

    let mut ids = Vec::new();
    collect_ids(&commands, &mut ids);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "duplicate command ids: {ids:?}");
}

#[test]
fn runnable_parent_task_keeps_its_metadata_and_children() {
    let content = fixture("lrqa_0.1.0_collisions.kdl");
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).expect("parse");
    let commands = commands_from_spec(&spec, "mise");

    let analyse = find(&commands, "mise:eiq:analyse").expect("eiq:analyse node");
    assert_eq!(
        analyse.description,
        "Run EiQ Analyse locally (single origin on :8080 via HAProxy)"
    );
    assert!(
        !analyse.subcommands.is_empty(),
        "merged node should keep its subtasks"
    );
    assert!(analyse.runnable, "a task that is also a parent stays runnable");

    let test = find(&commands, "mise:eiq:analyse:be:test").expect("be:test node");
    assert_eq!(test.description, "BE: parallel_rspec across workers");
    assert!(test.runnable);
    assert!(find(&test.subcommands, "mise:eiq:analyse:be:test:cov").is_some());

    // `be` is a pure namespace — there is no `eiq:analyse:be` task — so it must not
    // be runnable; Enter on it should expand rather than try to run nothing.
    let be = find(&commands, "mise:eiq:analyse:be").expect("be namespace node");
    assert!(!be.runnable, "synthetic namespace must not be runnable");
    assert!(be.description.is_empty());
}

// The browse tree is what actually rejected duplicate identifiers in the wild;
// assert it builds successfully from a colliding task set.
#[test]
fn browse_view_builds_from_colliding_tasks() {
    use bract::data::commands::Tool;

    let content = fixture("lrqa_0.1.0_collisions.kdl");
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).expect("parse");
    let commands = commands_from_spec(&spec, "mise");

    let tool = Tool {
        id: "mise_tasks".to_string(),
        name: "Mise Tasks".to_string(),
        bin: vec!["mise".to_string(), "run".to_string()],
        path_separator: ":".to_string(),
        description: String::new(),
        flags: vec![],
        args: vec![],
        commands,
    };

    BrowseView::new(&[tool]).expect("browse view should build without duplicate ids");
}
