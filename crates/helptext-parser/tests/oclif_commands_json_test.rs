mod common;

use helptext_parser::{InputFormat, Spec, SpecCommand, SpecFlag};

const DIR: &str = "oclif-commands-json";

fn parse(filename: &str) -> Spec {
    common::parse_fixture(InputFormat::OclifCommandsJson, DIR, filename)
}

fn at<'a>(spec: &'a Spec, path: &[&str]) -> &'a SpecCommand {
    path.iter().fold(&spec.cmd, |cmd, name| {
        cmd.subcommands.get(*name).unwrap_or_else(|| panic!("{} is in the tree", path.join(" ")))
    })
}

fn flag<'a>(cmd: &'a SpecCommand, long: &str) -> &'a SpecFlag {
    cmd.flags.iter().find(|f| f.long.iter().any(|l| l == long))
        .unwrap_or_else(|| panic!("--{long} is listed"))
}

#[test]
fn colon_ids_nest_into_a_tree_with_implied_topics() {
    let spec = parse("sf_2.152.14_org-list-create-plugins-install.json");

    assert!(at(&spec, &["org"]).subcommand_required, "org is only ever a prefix");
    assert!(at(&spec, &["org", "create"]).subcommand_required, "org create is only ever a prefix");
    assert!(!at(&spec, &["org", "create", "scratch"]).subcommand_required);
    assert!(at(&spec, &["org", "list", "sobject", "record-counts"]).subcommands.is_empty());

    let list = at(&spec, &["org", "list"]);
    assert!(!list.subcommand_required, "org list runs and has subcommands");
    assert!(list.subcommands.contains_key("auth"));
    assert_eq!(list.help.as_deref(), Some("List all orgs you’ve created or authenticated to."));
}

#[test]
fn flags_are_typed_and_hidden_ones_left_out() {
    let spec = parse("sf_2.152.14_org-list-create-plugins-install.json");
    let scratch = at(&spec, &["org", "create", "scratch"]);

    let hub = flag(scratch, "target-dev-hub");
    assert!(hub.required);
    assert_eq!(hub.short, vec!['v']);
    assert!(hub.arg.is_some());

    let edition = flag(scratch, "edition");
    let choices = &edition.arg.as_ref().and_then(|a| a.choices.as_ref()).expect("edition has choices").choices;
    assert_eq!(choices.len(), 8);

    assert!(flag(scratch, "track-source").arg.is_none());
    assert!(flag(scratch, "json").global);

    let list = at(&spec, &["org", "list"]);
    assert!(!list.flags.iter().any(|f| f.name == "loglevel"), "hidden flags stay hidden");
}

#[test]
fn arguments_keep_their_requirement() {
    let spec = parse("sf_2.152.14_org-list-create-plugins-install.json");
    let install = at(&spec, &["plugins", "install"]);
    assert_eq!(install.args[0].name, "plugin");
    assert!(install.args[0].required);

    let heroku = parse("heroku_11.11.0_apps-data.json");
    let create = at(&heroku, &["apps", "create"]);
    assert_eq!(create.args[0].name, "app");
    assert!(!create.args[0].required);
}

// oclif's help leaves hidden arguments out (`args.filter(a => !a.hidden)`);
// `heroku apps:destroy --help` shows none.
#[test]
fn hidden_arguments_stay_hidden() {
    let spec = parse("heroku_11.11.0_apps-data.json");
    assert!(at(&spec, &["apps", "destroy"]).args.is_empty());
}

#[test]
fn description_stands_in_for_a_missing_summary() {
    let spec = parse("heroku_11.11.0_apps-data.json");
    let create = at(&spec, &["apps", "create"]);

    assert_eq!(create.help.as_deref(), Some("creates a new app"));
    assert_eq!(flag(create, "remote").default, vec!["heroku"]);
    assert!(!create.flags.iter().any(|f| f.name == "app"), "heroku's hidden --app stays hidden");
}
