mod common;

use helptext_parser::InputFormat;

fn clap(fixture: &str) -> helptext_parser::Spec {
    common::parse_fixture(InputFormat::ClapHelptext, "clap-helptext", fixture)
}

// --- atlassian-cli: the tool that motivated Clap support ----------------------

#[test]
fn atlassian_cli_0_4_2_root_is_a_group() {
    let spec = clap("atlassian-cli_0.4.2_root.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("Unified Atlassian Cloud CLI"));

    // Real subcommands are exposed; clap's auto-generated `help` entry is not.
    for cmd in ["jira", "confluence", "bitbucket", "jsm", "opsgenie", "bamboo", "auth"] {
        assert!(spec.cmd.subcommands.contains_key(cmd), "root has `{cmd}`");
    }
    assert!(!spec.cmd.subcommands.contains_key("help"), "the clap `help` entry is filtered");

    // A pure group: its only usage form is `<COMMAND>` dispatch.
    assert!(spec.cmd.subcommand_required, "the root requires a subcommand");

    // Inline command aliases are captured: `bitbucket ... [aliases: bb]`.
    assert_eq!(spec.cmd.subcommands["bitbucket"].aliases, vec!["bb"]);

    // A value flag with a default and an enumerated choice set.
    let format = spec.cmd.flags.iter().find(|f| f.long == vec!["format"]).unwrap();
    assert_eq!(format.short, vec!['f']);
    assert!(format.arg.is_some(), "--format takes a value");
    assert_eq!(format.default, vec!["table"]);
    let choices = format.arg.as_ref().unwrap().choices.as_ref().unwrap();
    assert_eq!(choices.choices, ["table", "json", "yaml", "csv", "quiet", "markdown"]);

    // A boolean switch takes no value.
    let envelope = spec.cmd.flags.iter().find(|f| f.long == vec!["envelope"]).unwrap();
    assert!(envelope.arg.is_none(), "--envelope is a switch");

    // A short+long value flag.
    let profile = spec.cmd.flags.iter().find(|f| f.long == vec!["profile"]).unwrap();
    assert_eq!(profile.short, vec!['p']);
    assert!(profile.arg.is_some());
}

#[test]
fn atlassian_cli_0_4_2_jira_issue_get_is_a_leaf_with_positional() {
    let spec = clap("atlassian-cli_0.4.2_jira-issue-get.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("Fetch a single issue"));
    assert!(spec.cmd.subcommands.is_empty(), "a leaf has no subcommands");
    assert!(!spec.cmd.subcommand_required);

    // The `<KEY>` positional is required and carries its help.
    let key = spec.cmd.args.iter().find(|a| a.name == "KEY").unwrap();
    assert!(key.required, "<KEY> is required");
    assert_eq!(key.help.as_deref(), Some("Issue key (e.g. DEV-123)"));
}

#[test]
fn atlassian_cli_0_4_2_jira_is_a_nested_group() {
    let spec = clap("atlassian-cli_0.4.2_jira.txt");
    assert!(spec.cmd.subcommand_required);
    for cmd in ["issue", "project", "bulk", "webhooks"] {
        assert!(spec.cmd.subcommands.contains_key(cmd), "jira has `{cmd}`");
    }
}

// --- Famous Clap CLIs, verified against their real --help ---------------------
// These lock in the two layouts clap emits: inline descriptions (atlassian,
// zoxide, starship) and next-line descriptions (fd, hyperfine, bat).

#[test]
fn fd_10_4_2_leaf_with_optional_positionals() {
    let spec = clap("fd_10.4.2_root.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("A program to find entries in your filesystem"));
    assert!(spec.cmd.subcommands.is_empty(), "fd is a single command");
    assert!(!spec.cmd.subcommand_required);

    // Optional positionals from the Arguments section (next-line layout).
    let pattern = spec.cmd.args.iter().find(|a| a.name == "pattern").unwrap();
    assert!(!pattern.required, "[pattern] is optional");
    assert!(pattern.help.is_some(), "[pattern] help is assembled from next-line text");

    let path = spec.cmd.args.iter().find(|a| a.name == "path").unwrap();
    assert!(!path.required && path.var, "[path]... is optional and variadic");

    // A switch with a long-only and short+long form.
    let hidden = spec.cmd.flags.iter().find(|f| f.long == vec!["hidden"]).unwrap();
    assert_eq!(hidden.short, vec!['H']);
    assert!(hidden.arg.is_none(), "--hidden is a switch");

    // A repeatable count switch: `-u, --unrestricted...`.
    let unrestricted = spec.cmd.flags.iter().find(|f| f.long == vec!["unrestricted"]).unwrap();
    assert_eq!(unrestricted.short, vec!['u']);
    assert!(unrestricted.arg.is_none(), "--unrestricted is a count switch, not a value flag");
    assert!(unrestricted.count, "trailing ... marks a count flag");
}

#[test]
fn hyperfine_1_20_0_variadic_required_positional() {
    let spec = clap("hyperfine_1.20.0_root.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("A command-line benchmarking tool."));
    assert!(spec.cmd.subcommands.is_empty());

    // `<command>...` — required and variadic.
    let command = spec.cmd.args.iter().find(|a| a.name == "command").unwrap();
    assert!(command.required && command.var, "<command>... is required and variadic");

    // A value flag whose metavar is `<NUM>`.
    let warmup = spec.cmd.flags.iter().find(|f| f.long == vec!["warmup"]).unwrap();
    assert_eq!(warmup.short, vec!['w']);
    assert!(warmup.arg.is_some(), "--warmup takes a value");
}

#[test]
fn zoxide_0_9_9_group_with_next_line_usage() {
    let spec = clap("zoxide_0.9.9_root.txt");

    // Custom template puts name/author/url before the about — the about is still
    // recovered as the last preamble line.
    assert_eq!(spec.cmd.help.as_deref(), Some("A smarter cd command for your terminal"));

    // Usage is on the line after "Usage:" (`  zoxide <COMMAND>`); still a group.
    assert!(spec.cmd.subcommand_required);
    for cmd in ["add", "edit", "import", "init", "query", "remove"] {
        assert!(spec.cmd.subcommands.contains_key(cmd), "zoxide has `{cmd}`");
    }
}

#[test]
fn zoxide_0_9_9_query_leaf_flags_and_empty_desc_arg() {
    let spec = clap("zoxide_0.9.9_query.txt");

    assert!(spec.cmd.subcommands.is_empty());
    // An argument with no description must still be captured.
    let keywords = spec.cmd.args.iter().find(|a| a.name == "KEYWORDS").unwrap();
    assert!(!keywords.required && keywords.var, "[KEYWORDS]... optional variadic");

    // Inline-layout flags, including a value flag.
    let exclude = spec.cmd.flags.iter().find(|f| f.long == vec!["exclude"]).unwrap();
    assert!(exclude.arg.is_some(), "--exclude takes a path value");
    let all = spec.cmd.flags.iter().find(|f| f.long == vec!["all"]).unwrap();
    assert_eq!(all.short, vec!['a']);
    assert!(all.arg.is_none());
}

#[test]
fn starship_1_25_1_group_and_init_leaf() {
    let root = clap("starship_1.25.1_root.txt");
    assert_eq!(root.cmd.help.as_deref(), Some("The cross-shell prompt for astronauts. ☄🌌️"));
    assert!(root.cmd.subcommand_required);
    for cmd in ["init", "prompt", "config", "module"] {
        assert!(root.cmd.subcommands.contains_key(cmd), "starship has `{cmd}`");
    }
    assert!(!root.cmd.subcommands.contains_key("help"));

    let init = clap("starship_1.25.1_init.txt");
    assert!(init.cmd.subcommands.is_empty());
    assert!(!init.cmd.subcommand_required, "init is runnable (has a `<SHELL>` positional)");
    let shell = init.cmd.args.iter().find(|a| a.name == "SHELL").unwrap();
    assert!(shell.required, "<SHELL> is required");
    let print_full = init.cmd.flags.iter().find(|f| f.long == vec!["print-full-init"]).unwrap();
    assert!(print_full.arg.is_none(), "--print-full-init is a switch");
}

#[test]
fn bat_0_26_1_dual_command_is_runnable() {
    let spec = clap("bat_0.26.1_root.txt");

    assert_eq!(
        spec.cmd.help.as_deref(),
        Some("A cat(1) clone with syntax highlighting and Git integration."),
    );

    // bat has two usage forms: `bat [OPTIONS] [FILE]...` and `bat <COMMAND>`. The
    // run form means it is NOT subcommand-required.
    assert!(!spec.cmd.subcommand_required, "bat has its own run form");

    let file = spec.cmd.args.iter().find(|a| a.name == "FILE").unwrap();
    assert!(!file.required && file.var, "[FILE]... optional variadic");

    let show_all = spec.cmd.flags.iter().find(|f| f.long == vec!["show-all"]).unwrap();
    assert_eq!(show_all.short, vec!['A']);
    assert!(show_all.arg.is_none());
}

// Clap's verbose "Possible values:" block (emitted when values carry per-value
// help) is recovered into choices — both the bullet-list and inline-list shapes.
#[test]
fn bat_0_26_1_recovers_verbose_possible_values() {
    let spec = clap("bat_0.26.1_root.txt");
    let choices_of = |long: &str| {
        spec.cmd
            .flags
            .iter()
            .find(|f| f.long == vec![long.to_string()])
            .unwrap_or_else(|| panic!("flag --{long} not found"))
            .arg
            .as_ref()
            .and_then(|a| a.choices.as_ref())
            .map(|c| c.choices.clone())
            .unwrap_or_default()
    };

    // Bullet list with `* name (desc)`.
    assert_eq!(choices_of("nonprintable-notation"), ["unicode", "caret"]);

    // Inline list `*auto*, never, always.` — `*auto*` marks the default.
    assert_eq!(choices_of("color"), ["auto", "never", "always"]);
    let color = spec.cmd.flags.iter().find(|f| f.long == vec!["color"]).unwrap();
    assert_eq!(color.default, vec!["auto"], "the *…*-wrapped value is the default");

    // The block is stripped from the visible help, not left dangling.
    assert!(
        !color.help.as_deref().unwrap_or_default().contains("Possible values"),
        "the possible-values block is removed from help text",
    );

    // Bullet list with `* name: desc` and a `(default)` marker.
    let style = choices_of("style");
    assert!(style.contains(&"default".to_string()) && style.contains(&"full".to_string()), "{style:?}");
}
