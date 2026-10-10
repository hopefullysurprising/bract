mod common;

use helptext_parser::InputFormat;

#[test]
fn mise_2026_1_7_tasks_with_choices() {
    let spec = common::parse_fixture(
        InputFormat::UsageKdl,
        "usage-kdl",
        "mise_2026.1.7_tasks-with-choices.kdl",
    );

    assert!(spec.cmd.subcommands.len() > 10);

    let claude = &spec.cmd.subcommands["claude"];
    assert_eq!(
        claude.help.as_deref(),
        Some("It runs Claude Code and configures for use in this particular project")
    );
    assert_eq!(claude.args[0].name, "claude_license");
    let choices = claude.args[0].choices.as_ref().unwrap();
    assert!(choices.choices.contains(&"personal".to_string()));
    assert!(choices.choices.contains(&"company".to_string()));
}

#[test]
fn usage_3_5_0_nested_subcommands() {
    // A usage-lib CLI's own spec (`usage --usage-spec`): unlike mise tasks, its
    // subcommands nest directly rather than via colon-joined names.
    let spec = common::parse_fixture(InputFormat::UsageKdl, "usage-kdl", "usage_3.5.0.kdl");

    let generate = &spec.cmd.subcommands["generate"];
    assert!(
        generate.subcommands.contains_key("completion"),
        "generate nests a 'completion' subcommand"
    );
    assert!(
        spec.cmd.subcommands["lint"].subcommands.is_empty(),
        "lint is a leaf with no subcommands"
    );
}

fn read_fixture(name: &str) -> helptext_parser::Parsed {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage-kdl").join(name);
    helptext_parser::read(InputFormat::UsageKdl, &std::fs::read_to_string(path).unwrap()).expect("the spec reads")
}

// mise 2026.9.7 describes itself with features from usage 6 and later —
// `unknown_flags`, a flag's `conflicts=` and `overrides`, `choices` written as a
// block among them. It reads in full: nothing left out, and `mise activate
// --shell` offers the choices its block lists.
#[test]
fn todays_mise_spec_reads_in_full() {
    let parsed = read_fixture("mise_2026.9.7_usage.kdl");

    assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);
    let activate = &parsed.spec.cmd.subcommands["activate"];
    let shell = activate.flags.iter().find(|f| f.long == vec!["shell"]).expect("--shell");
    let choices = &shell.arg.as_ref().and_then(|a| a.choices.as_ref()).expect("shell offers choices").choices;
    assert!(choices.contains(&"bash".to_string()));
}

// A tool newer than this usage-lib used to cost its whole spec: mise 2026.9.7's own
// went missing from every project until usage-lib caught up. What usage-lib does not
// know is left out and named; the rest reads. `from_the_future` is invented — no
// released usage has a key this usage-lib rejects, which is the situation this
// guards.
#[test]
fn a_spec_newer_than_this_usage_lib_reads_without_what_it_does_not_know() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage-kdl/mise_2026.9.7_usage.kdl");
    let spec = std::fs::read_to_string(path).unwrap() + "from_the_future \"a key no usage-lib knows\"\n";

    let parsed = helptext_parser::read(InputFormat::UsageKdl, &spec).expect("the spec reads");
    assert_eq!(parsed.skipped, ["from_the_future"]);
    for command in ["run", "use", "install", "tasks"] {
        assert!(parsed.spec.cmd.subcommands.contains_key(command), "mise {command} is there");
    }
}
