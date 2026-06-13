mod common;

use helptext_parser::InputFormat;

#[test]
fn mani_0_32_0_root() {
    let spec = common::parse_fixture(
        InputFormat::CobraHelptext,
        "cobra-helptext",
        "mani_0.32.0_root.txt",
    );

    assert_eq!(spec.cmd.subcommands.len(), 12);
    assert_eq!(spec.cmd.subcommands["run"].help.as_deref(), Some("Run tasks"));
    assert_eq!(
        spec.cmd.subcommands["list"].help.as_deref(),
        Some("List projects, tasks and tags"),
    );
    assert_eq!(
        spec.cmd.subcommands["check"].help.as_deref(),
        Some("Validate config"),
    );

    let config_flag = spec.cmd.flags.iter().find(|f| f.long == vec!["config"]).unwrap();
    assert_eq!(config_flag.short, vec!['c']);
    assert_eq!(config_flag.help.as_deref(), Some("specify config"));
    assert!(config_flag.arg.is_some());

    let color_flag = spec.cmd.flags.iter().find(|f| f.long == vec!["color"]).unwrap();
    assert!(color_flag.short.is_empty());
    assert_eq!(color_flag.help.as_deref(), Some("enable color"));
    assert_eq!(color_flag.default, vec!["true"]);

    let version_flag = spec.cmd.flags.iter().find(|f| f.long == vec!["version"]).unwrap();
    assert_eq!(version_flag.short, vec!['v']);
    assert!(version_flag.arg.is_none());
}

#[test]
fn mani_0_32_0_run_flags_and_description() {
    let spec = common::parse_fixture(
        InputFormat::CobraHelptext,
        "cobra-helptext",
        "mani_0.32.0_run.txt",
    );

    assert_eq!(
        spec.cmd.help.as_deref(),
        Some("Run tasks."),
    );
    assert_eq!(
        spec.cmd.help_long.as_deref(),
        Some("Run tasks.\n\nThe tasks are specified in a mani.yaml file along with the projects you can target."),
    );

    assert!(spec.cmd.subcommands.is_empty());

    let forks = spec.cmd.flags.iter().find(|f| f.long == vec!["forks"]).unwrap();
    assert_eq!(forks.short, vec!['f']);
    assert!(forks.arg.is_some());
    assert_eq!(forks.default, vec!["4"]);

    let projects = spec.cmd.flags.iter().find(|f| f.long == vec!["projects"]).unwrap();
    assert_eq!(projects.short, vec!['p']);
    assert!(projects.arg.is_some());

    let global_flags: Vec<_> = spec.cmd.flags.iter().filter(|f| f.global).collect();
    assert_eq!(global_flags.len(), 3);
    let global_config = global_flags.iter().find(|f| f.long == vec!["config"]).unwrap();
    assert_eq!(global_config.short, vec!['c']);
}

#[test]
fn mani_0_32_0_list_projects_aliases() {
    let spec = common::parse_fixture(
        InputFormat::CobraHelptext,
        "cobra-helptext",
        "mani_0.32.0_list-projects.txt",
    );

    assert_eq!(spec.cmd.aliases, vec!["project", "proj", "pr"]);

    let headers = spec.cmd.flags.iter().find(|f| f.long == vec!["headers"]).unwrap();
    assert!(headers.arg.is_some());
    assert_eq!(headers.default, vec!["[project,tag,description]"]);
}

// The GitHub CLI ships a bespoke uppercase Cobra template (CORE COMMANDS,
// TARGETED COMMANDS, `name:` entries). The parser must read it like any other.
#[test]
fn cli_2_92_0_root_parses_uppercase_template() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "gh_2.92.0_root.txt");

    assert!(spec.cmd.subcommands.contains_key("repo"));
    assert!(spec.cmd.subcommands.contains_key("auth"));
    assert!(spec.cmd.subcommands.contains_key("pr"));
    assert_eq!(spec.cmd.subcommands["repo"].help.as_deref(), Some("Manage repositories"));

    // HELP TOPICS entries (accessibility, environment, formatting, …) are prose,
    // not commands, and must not leak into the tree.
    assert!(!spec.cmd.subcommands.contains_key("environment"));
    assert!(!spec.cmd.subcommands.contains_key("formatting"));
}

#[test]
fn cli_2_92_0_repo_lists_its_subcommands() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "gh_2.92.0_repo.txt");

    assert!(spec.cmd.subcommands.contains_key("clone"));
    assert!(spec.cmd.subcommands.contains_key("view"));
    assert_eq!(
        spec.cmd.subcommands["clone"].help.as_deref(),
        Some("Clone a repository locally"),
    );
}

#[test]
fn cli_2_92_0_repo_view_is_a_leaf_with_flags() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "gh_2.92.0_repo_view.txt");

    assert!(spec.cmd.subcommands.is_empty(), "a leaf command has no subcommands");
    assert!(spec.cmd.flags.iter().any(|f| f.long == vec!["json"]), "leaf flags are parsed");
}

// A command whose help only offers a `[command]` dispatch form is a pure group:
// it requires a subcommand and cannot be invoked on its own.
#[test]
fn mani_0_32_0_describe_is_a_pure_group() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "mani_0.32.0_describe.txt");
    assert!(!spec.cmd.subcommands.is_empty(), "describe has subcommands");
    assert!(spec.cmd.subcommand_required, "a pure group requires a subcommand");
}

// A command with both its own `[flags]` run form *and* subcommands is dual: it is
// runnable in its own right, so it must not be marked subcommand-required.
#[test]
fn mani_0_32_0_edit_is_runnable_despite_having_subcommands() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "mani_0.32.0_edit.txt");
    assert!(!spec.cmd.subcommands.is_empty(), "edit has subcommands");
    assert!(!spec.cmd.subcommand_required, "edit has its own run form, so it stays runnable");
}

// A plain leaf is runnable and never subcommand-required.
#[test]
fn mani_0_32_0_sync_leaf_is_not_subcommand_required() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "mani_0.32.0_sync.txt");
    assert!(spec.cmd.subcommands.is_empty());
    assert!(!spec.cmd.subcommand_required);
}

// On a leaf command, a positional named `<command>` is a real argument — not the
// generic subcommand placeholder we filter on groups (`mani [command]`).
#[test]
fn mani_0_32_0_exec_keeps_its_command_positional() {
    let spec = common::parse_fixture(InputFormat::CobraHelptext, "cobra-helptext", "mani_0.32.0_exec.txt");

    assert!(spec.cmd.subcommands.is_empty(), "exec is a leaf");
    let command = spec.cmd.args.iter().find(|a| a.name == "command");
    assert!(command.is_some(), "the <command> positional must be exposed");
    assert!(command.unwrap().required, "<command> is required");
}
