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
