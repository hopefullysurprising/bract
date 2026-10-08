mod common;

use helptext_parser::{InputFormat, Spec, SpecFlag};

const DIR: &str = "oclif-helptext";

fn parse(filename: &str) -> Spec {
    common::parse_fixture(InputFormat::OclifHelptext, DIR, filename)
}

fn flag<'a>(spec: &'a Spec, long: &str) -> &'a SpecFlag {
    spec.cmd.flags.iter().find(|f| f.long.iter().any(|l| l == long))
        .unwrap_or_else(|| panic!("--{long} is listed"))
}

#[test]
fn root_marks_pure_topics_apart_from_commands() {
    let spec = parse("sf_2.152.14_root.txt");

    assert_eq!(spec.bin, "sf");
    assert!(spec.cmd.subcommand_required, "`sf [COMMAND]` only dispatches");
    assert!(spec.cmd.subcommands["org"].subcommand_required, "org is listed only as a topic");
    assert!(!spec.cmd.subcommands["plugins"].subcommand_required, "plugins is a topic and a command");
    assert!(!spec.cmd.subcommands["whatsnew"].subcommand_required, "whatsnew is only a command");
    assert_eq!(spec.cmd.subcommands["org"].help.as_deref(), Some("Commands to create and manage orgs and scratch org users."));
}

// Topic summaries are merged from every plugin contributing to the topic, and come
// out wrong: sf lists `org list` as "Used to list users". The command's own entry
// is the one that describes it.
#[test]
fn a_name_listed_as_topic_and_command_takes_the_command_summary() {
    let spec = parse("sf_2.152.14_org.txt");

    assert_eq!(
        spec.cmd.subcommands["list"].help.as_deref(),
        Some("List all orgs you’ve created or authenticated to."),
    );
    assert_eq!(spec.cmd.subcommands["create"].help.as_deref(), Some("Used to create a user"), "a topic-only entry keeps its topic summary");
}

#[test]
fn children_are_named_by_their_own_segment_whatever_the_separator() {
    let sf = parse("sf_2.152.14_org.txt");
    assert!(sf.cmd.subcommands.contains_key("list"));
    assert!(!sf.cmd.subcommands.contains_key("org list"));

    let heroku = parse("heroku_11.11.0_apps.txt");
    assert!(heroku.cmd.subcommands.contains_key("create"));
    assert!(!heroku.cmd.subcommands.contains_key("apps:create"));

    let nested = parse("heroku_11.11.0_apps-favorites.txt");
    assert_eq!(nested.cmd.subcommands.keys().collect::<Vec<_>>(), ["add", "remove"]);
}

// oclif's `help` command answers `sf help theme --help` with theme's page, so a
// page can describe a command other than the one asked for. Its usage line says
// which, in the CLI's own spelling.
#[test]
fn a_page_names_the_command_it_describes() {
    assert!(parse("sf_2.152.14_root.txt").cmd.full_cmd.is_empty());
    assert_eq!(parse("sf_2.152.14_org-list.txt").cmd.full_cmd, ["org", "list"]);
    assert_eq!(parse("sf_2.152.14_org-create.txt").cmd.full_cmd, ["org", "create"]);
    assert_eq!(parse("heroku_11.11.0_apps-create.txt").cmd.full_cmd, ["apps:create"]);
    assert_eq!(parse("heroku_11.11.0_data.txt").cmd.full_cmd, ["data"]);
}

#[test]
fn a_topic_without_its_own_command_is_not_runnable() {
    for fixture in ["sf_2.152.14_org.txt", "sf_2.152.14_org-create.txt", "heroku_11.11.0_data.txt", "shopify_4.8.5_app.txt"] {
        let spec = parse(fixture);
        assert!(spec.cmd.subcommand_required, "{fixture} only dispatches");
        assert!(!spec.cmd.subcommands.is_empty(), "{fixture} lists its children");
    }
}

#[test]
fn a_command_with_subcommands_stays_runnable() {
    let sf = parse("sf_2.152.14_org-list.txt");
    assert!(!sf.cmd.subcommand_required);
    assert!(sf.cmd.subcommands.contains_key("auth"));
    assert!(sf.cmd.subcommands.contains_key("sobject"));
    assert_eq!(flag(&sf, "all").help.as_deref(), Some("Include expired, deleted, and unknown-status scratch orgs."));

    let heroku = parse("heroku_11.11.0_apps.txt");
    assert!(!heroku.cmd.subcommand_required);
    assert_eq!(heroku.cmd.help.as_deref(), Some("list your apps"));
}

#[test]
fn flags_carry_requirement_choices_and_value_shape() {
    let spec = parse("sf_2.152.14_org-create-scratch.txt");

    let hub = flag(&spec, "target-dev-hub");
    assert!(hub.required);
    assert_eq!(hub.short, vec!['v']);
    assert!(hub.arg.is_some());
    assert_eq!(hub.help.as_deref(), Some("Username or alias of the Dev Hub org."));

    let edition = flag(&spec, "edition");
    let choices = &edition.arg.as_ref().and_then(|a| a.choices.as_ref()).expect("edition has choices").choices;
    assert_eq!(choices.first().map(String::as_str), Some("developer"));
    assert_eq!(choices.last().map(String::as_str), Some("partner-professional"));

    let track = flag(&spec, "track-source");
    assert!(track.arg.is_none(), "--[no-]track-source is a switch");

    assert!(!flag(&spec, "no-ancestors").global, "a named flag group holds the command's own flags");
    assert!(flag(&spec, "json").global);
    assert!(flag(&spec, "flags-dir").global);

    let aliases = spec.cmd.flags.iter().filter(|f| f.long.iter().any(|l| l == "alias")).count();
    assert_eq!(aliases, 1, "FLAG DESCRIPTIONS repeats flags without declaring new ones");
}

#[test]
fn required_choice_flag_from_a_jit_plugin() {
    let spec = parse("sf_2.152.14_cmdt-generate-field.txt");

    let kind = flag(&spec, "type");
    assert!(kind.required);
    let choices = &kind.arg.as_ref().and_then(|a| a.choices.as_ref()).expect("type has choices").choices;
    assert_eq!(choices.len(), 12);
    assert!(choices.contains(&"LongTextArea".to_string()));
}

#[test]
fn defaults_are_lifted_out_of_the_description() {
    let spec = parse("heroku_11.11.0_apps-create.txt");

    let remote = flag(&spec, "remote");
    assert_eq!(remote.default, vec!["heroku"]);
    assert_eq!(remote.help.as_deref(), Some("the git remote to create, default \"heroku\""));
}

#[test]
fn arguments_keep_their_requirement() {
    let heroku = parse("heroku_11.11.0_apps-create.txt");
    let app = &heroku.cmd.args[0];
    assert_eq!(app.name, "APP");
    assert!(!app.required);
    assert_eq!(app.help.as_deref(), Some("name of app to create"));

    let sf = parse("sf_2.152.14_plugins-install.txt");
    let plugin = &sf.cmd.args[0];
    assert_eq!(plugin.name, "PLUGIN");
    assert!(plugin.required);
}

fn arg<'a>(spec: &'a Spec, name: &str) -> &'a helptext_parser::SpecArg {
    spec.cmd.args.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("{name} is an argument: {:?}", spec.cmd.args))
}

fn choices(arg: Option<&helptext_parser::SpecArg>) -> Vec<String> {
    arg.and_then(|a| a.choices.as_ref()).map(|c| c.choices.clone()).unwrap_or_default()
}

// The cases below come from reading oclif's help renderer (`@oclif/core`
// lib/help, identical in 4.14 and 5.0) and the help classes sf and shopify
// substitute for it, each confirmed on a real page.

// A beta, preview or deprecated command says so on a line of its own above its
// summary.
#[test]
fn the_summary_follows_a_state_line() {
    let spec = parse("sf_2.152.14_agent-mcp-create.txt");
    assert_eq!(spec.cmd.help.as_deref(), Some("Create an MCP server in the API Catalog."));
}

// oclif puts `[default: x, env: Y]` — either half alone, too — ahead of the
// description.
#[test]
fn a_flags_environment_variable_is_not_its_description() {
    let access = parse("heroku_11.11.0_access.txt");
    let app = flag(&access, "app");
    assert!(app.required);
    assert_eq!(app.help.as_deref(), Some("app to run command against"));

    let open = parse("sf_2.152.14_org-open.txt");
    assert_eq!(flag(&open, "path").help.as_deref(), Some("Navigation URL path to open a specific page."));
}

// shopify renders every flag section stacked — the label alone, its description
// indented beneath — and adds the environment variable as a line of its own.
// A description line may start with `-` without being a flag.
#[test]
fn a_stacked_flag_list_reads_like_a_two_column_one() {
    let spec = parse("shopify_4.8.5_app-init.txt");
    let names: Vec<&str> = spec.cmd.flags.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["package-manager", "name", "path", "auth-alias", "client-id", "flavor", "no-color", "organization-id", "template", "verbose"],
    );

    assert_eq!(choices(flag(&spec, "package-manager").arg.as_ref()), ["npm", "yarn", "pnpm", "bun"]);
    assert_eq!(flag(&spec, "path").default, vec!["/home/runner/work/cli/cli/packages/cli"]);
    assert_eq!(flag(&spec, "auth-alias").help.as_deref(), Some("Alias of the Shopify account to use for authentication."));
    assert!(flag(&spec, "template").help.as_deref().unwrap_or_default().contains("- <reactRouter|none>"));

    let release = parse("shopify_4.8.5_app-release.txt");
    assert!(flag(&release, "version").required, "(required) opens the stacked description");
}

#[test]
fn an_arguments_choices_and_default_are_lifted_out() {
    let autocomplete = parse("sf_2.152.14_autocomplete.txt");
    let shell = arg(&autocomplete, "SHELL");
    assert!(!shell.required);
    assert_eq!(choices(Some(shell)), ["zsh", "bash", "powershell"]);
    assert_eq!(shell.help.as_deref(), Some("Shell type"));

    let inspect = parse("sf_2.152.14_plugins-inspect.txt");
    let plugin = arg(&inspect, "PLUGIN");
    assert_eq!(plugin.default, vec!["."]);
    assert_eq!(plugin.help.as_deref(), Some("Plugin to inspect."));

    let setting = parse("heroku_11.11.0_pg-settings-log-statement.txt");
    let value = arg(&setting, "VALUE");
    assert_eq!(choices(Some(value)), ["none", "ddl", "mod", "all"]);
    assert_eq!(value.help.as_deref(), Some("type of SQL statements to log"));
}

// oclif drops the ARGUMENTS section when no argument has a description; the
// usage line still names them. In a custom usage line, a flag's value
// (`-a APP`) and a placeholder (`[flags]`) are not arguments.
#[test]
fn arguments_without_descriptions_come_from_the_usage_line() {
    let update = parse("sf_2.152.14_update.txt");
    assert!(!arg(&update, "CHANNEL").required);

    let search = parse("shopify_4.8.5_search.txt");
    assert!(!arg(&search, "query").required);

    let pull = parse("heroku_11.11.0_container-pull.txt");
    let names: Vec<&str> = pull.cmd.args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["PROCESS_TYPE"]);
    assert!(pull.cmd.args[0].required);

    let ps = parse("heroku_11.11.0_ps.txt");
    let names: Vec<&str> = ps.cmd.args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["TYPE"], "[TYPE [TYPE ...]] is one repeatable argument");
}

// `showFlagOptionsInTitle`, and sf's legacy plugins, put the choices in the label.
#[test]
fn choices_in_the_label_are_choices() {
    let spec = parse("sf_2.152.14_force-lightning-lwc-test-run.txt");
    let level = flag(&spec, "loglevel");
    let level_choices = choices(level.arg.as_ref());
    assert_eq!(level_choices.first().map(String::as_str), Some("trace"));
    assert_eq!(level_choices.last().map(String::as_str), Some("FATAL"));
    assert_eq!(level.default, vec!["warn"]);
}

// A custom `helpValue` replaces `<value>` with anything, spaces included.
#[test]
fn a_custom_value_placeholder_still_takes_a_value() {
    let spec = parse("sf_2.152.14_api-request-rest.txt");
    assert!(flag(&spec, "header").arg.is_some());
    assert!(flag(&spec, "stream-to-file").arg.is_some());
    assert_eq!(flag(&spec, "stream-to-file").help.as_deref(), Some("Stream responses to a file."));
    assert_eq!(choices(flag(&spec, "method").arg.as_ref()).len(), 8);
}

#[test]
fn a_replaced_help_class_still_reads() {
    let root = parse("shopify_4.8.5_root.txt");
    assert!(root.cmd.subcommands["app"].subcommand_required);
    assert!(!root.cmd.subcommands["upgrade"].subcommand_required);

    let theme = parse("shopify_4.8.5_theme.txt");
    assert!(theme.cmd.subcommands.contains_key("check"));
    assert!(theme.cmd.subcommands["metafields"].subcommand_required);
}
