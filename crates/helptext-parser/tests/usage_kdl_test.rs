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
