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
// `unknown_flags` first among them. A usage-lib older than the tool rejected the
// whole spec, and mise's own CLI went missing from every project. What this
// usage-lib does not know is left out and named; the commands, flags and arguments
// it does know still read.
#[test]
fn a_spec_newer_than_this_usage_lib_reads_without_what_it_does_not_know() {
    let parsed = read_fixture("mise_2026.9.7_usage.kdl");

    for command in ["run", "use", "install", "tasks"] {
        assert!(parsed.spec.cmd.subcommands.contains_key(command), "mise {command} is there");
    }
    assert!(!parsed.spec.cmd.subcommands["use"].flags.is_empty(), "mise use keeps its flags");
    assert!(parsed.skipped.contains(&"unknown_flags".to_string()), "{:?}", parsed.skipped);
    for structural in ["cmd", "flag", "arg"] {
        assert!(!parsed.skipped.contains(&structural.to_string()), "{structural} is never left out: {:?}", parsed.skipped);
    }
}
