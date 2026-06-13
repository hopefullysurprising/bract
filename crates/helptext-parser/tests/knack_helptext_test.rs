mod common;

use helptext_parser::InputFormat;

const DIR: &str = "knack-help";

#[test]
fn root_lists_subgroups_as_branches() {
    let spec = common::parse_fixture(InputFormat::KnackHelptext, DIR, "az_2.87.0_root.txt");

    // The root is a group: running `az` alone requires a subcommand.
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.subcommands.contains_key("account"));
    assert_eq!(
        spec.cmd.subcommands["account"].help.as_deref(),
        Some("Manage Azure subscription information."),
    );
    // The root mixes Subgroups (branches) with top-level Commands (leaves like
    // `login`/`logout`): the parser must mark them differently.
    assert!(spec.cmd.subcommands["account"].subcommand_required, "account is a subgroup");
    assert!(!spec.cmd.subcommands["login"].subcommand_required, "login is a command");
    assert!(!spec.cmd.subcommands["logout"].subcommand_required, "logout is a command");
}

#[test]
fn group_distinguishes_subgroups_from_commands() {
    let spec = common::parse_fixture(InputFormat::KnackHelptext, DIR, "az_2.87.0_account.txt");

    // Subgroups are branches; commands are runnable leaves.
    assert!(spec.cmd.subcommands["lock"].subcommand_required);
    assert!(spec.cmd.subcommands["management-group"].subcommand_required);
    assert!(!spec.cmd.subcommands["list"].subcommand_required);
    assert!(!spec.cmd.subcommands["show"].subcommand_required);

    // Wrapped descriptions are stitched back into one line.
    assert_eq!(
        spec.cmd.subcommands["list"].help.as_deref(),
        Some("Get a list of subscriptions for the logged in account. By default, only 'Enabled' subscriptions from the current cloud is shown."),
    );
}

#[test]
fn leaf_command_is_runnable_and_splits_local_from_global_args() {
    let spec = common::parse_fixture(InputFormat::KnackHelptext, DIR, "az_2.87.0_account_list.txt");

    // `az account list` is a runnable leaf.
    assert!(!spec.cmd.subcommand_required);
    assert!(spec.cmd.subcommands.is_empty());

    let all = spec.cmd.flags.iter().find(|f| f.long == vec!["all"]).unwrap();
    assert!(!all.global, "command-specific args are not global");

    // Global Arguments are flagged so the form layer can hide them by default.
    let debug = spec.cmd.flags.iter().find(|f| f.long == vec!["debug"]).unwrap();
    assert!(debug.global);
    let output = spec.cmd.flags.iter().find(|f| f.long == vec!["output"]).unwrap();
    assert!(output.global);
}

#[test]
fn argument_aliases_required_choices_and_defaults() {
    let spec = common::parse_fixture(InputFormat::KnackHelptext, DIR, "az_2.87.0_group_create.txt");

    // `--name --resource-group -g -n [Required]` — all aliases on one line.
    let name = spec.cmd.flags.iter().find(|f| f.long.contains(&"name".to_string())).unwrap();
    assert!(name.required);
    assert_eq!(name.long, vec!["name", "resource-group"]);
    assert_eq!(name.short, vec!['g', 'n']);

    let location = spec.cmd.flags.iter().find(|f| f.long == vec!["location"]).unwrap();
    assert!(location.required);

    let tags = spec.cmd.flags.iter().find(|f| f.long == vec!["tags"]).unwrap();
    assert!(!tags.required);

    // Global Policy Arguments + Global Arguments are global.
    let token = spec.cmd.flags.iter().find(|f| f.long == vec!["acquire-policy-token"]).unwrap();
    assert!(token.global);

    // Inline `Allowed values:` and `Default:` are recovered into the arg.
    let output = spec.cmd.flags.iter().find(|f| f.long == vec!["output"]).unwrap();
    let choices = output.arg.as_ref().and_then(|a| a.choices.as_ref()).unwrap();
    assert!(choices.choices.contains(&"json".to_string()));
    assert!(choices.choices.contains(&"yaml".to_string()));
    assert_eq!(output.default, vec!["json"]);
}
