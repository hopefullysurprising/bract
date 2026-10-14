mod common;

use helptext_parser::{InputFormat, Spec, SpecArg, SpecFlag};

fn fixture(filename: &str) -> Spec {
    common::parse_fixture(InputFormat::FangHelptext, "fang-helptext", filename)
}

fn flag<'a>(spec: &'a Spec, long: &str) -> &'a SpecFlag {
    spec.cmd.flags.iter().find(|f| f.long == vec![long]).unwrap_or_else(|| panic!("no --{long}"))
}

fn arg<'a>(spec: &'a Spec, name: &str) -> &'a SpecArg {
    spec.cmd.args.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("no arg {name}: {:?}", spec.cmd.args))
}

#[test]
fn glab_1_120_0_root() {
    let spec = fixture("glab_1.120.0_root.txt");

    assert_eq!(spec.bin, "glab");
    assert_eq!(spec.cmd.help.as_deref(), Some("GLab is an open source GitLab CLI tool that brings GitLab to your command line."));
    assert_eq!(spec.cmd.subcommands.len(), 48);
    assert_eq!(spec.cmd.subcommands["api"].help.as_deref(), Some("Make an authenticated request to the GitLab API."));
    assert_eq!(spec.cmd.subcommands["work-items"].help.as_deref(), Some("Manage work items. (EXPERIMENTAL)"));
    assert!(spec.cmd.subcommand_required);
    // `glab <command> <subcommand>` names the tree below, not arguments of glab.
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);

    let version = flag(&spec, "version");
    assert_eq!(version.short, vec!['v']);
    assert!(version.arg.is_none(), "cobra's own --version is a switch");
    assert!(flag(&spec, "help").arg.is_none(), "cobra's own --help is a switch");
}

// A fang page prints a command's long description when it has one, so only the
// listing it came from carries the one-line summary — the page must not replace it.
#[test]
fn glab_1_120_0_mr_keeps_its_listings_summary() {
    let spec = fixture("glab_1.120.0_mr.txt");

    assert_eq!(spec.cmd.help, None);
    let long = spec.cmd.help_long.as_deref().expect("long description");
    assert!(long.starts_with("Open a merge request from your current branch"), "{long}");
    assert!(long.contains("\n\nUse `--repo` to target a project other than the current one."), "{long}");

    assert_eq!(spec.cmd.subcommands.len(), 20);
    assert_eq!(spec.cmd.subcommands["list"].help.as_deref(), Some("List merge requests."));
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

// fang prints no value placeholder, so a switch and a flag taking a value look the
// same: every flag offers a value, and pflag accepts `--draft=true` for a switch.
#[test]
fn glab_1_120_0_mr_list_flags_all_take_a_value() {
    let spec = fixture("glab_1.120.0_mr_list.txt");

    assert!(spec.cmd.subcommands.is_empty());
    assert!(!spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.flags.len(), 28);

    for long in ["author", "draft", "per-page"] {
        assert!(flag(&spec, long).arg.is_some(), "--{long} offers a value");
    }
    assert!(flag(&spec, "help").arg.is_none());

    let assignee = flag(&spec, "assignee");
    assert_eq!(assignee.short, vec!['a']);
    assert!(assignee.help.as_deref().unwrap().starts_with("Get only merge requests assigned to users."));

    // A default prints as a bare `(30)`, indistinguishable from prose such as
    // `(EXPERIMENTAL)`, so it stays part of the description and never prefills.
    let per_page = flag(&spec, "per-page");
    assert_eq!(per_page.help.as_deref(), Some("Number of items to list per page. (30)"));
    assert!(per_page.default.is_empty());

    assert_eq!(flag(&spec, "repo").short, vec!['R'], "inherited flags are listed with the command's own");
}

#[test]
fn glab_1_120_0_api_takes_its_endpoint() {
    let spec = fixture("glab_1.120.0_api.txt");

    assert_eq!(spec.cmd.args.len(), 1);
    assert!(arg(&spec, "endpoint").required);
    assert_eq!(flag(&spec, "method").short, vec!['X']);
}

#[test]
fn glab_1_120_0_mr_approve_takes_one_of_two_placeholders() {
    let spec = fixture("glab_1.120.0_mr_approve.txt");

    assert_eq!(spec.cmd.args.len(), 1, "{:?}", spec.cmd.args);
    let target = arg(&spec, "id|branch");
    assert!(!target.required);
    assert!(target.choices.is_none(), "placeholders are not literal choices");
}

// A usage alternative written as a flag (`-g <group>`, `--name <name>`) is that
// flag, listed with the others; only placeholders are positional. A `--` group passes
// what follows to another program, which a positional cannot express.
#[test]
fn glab_1_120_0_repo_clone_keeps_only_positional_alternatives() {
    let spec = fixture("glab_1.120.0_repo_clone.txt");

    let names: Vec<&str> = spec.cmd.args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["repo", "dir"]);
}

#[test]
fn glab_1_120_0_securefile_download_names_its_positional_alternative() {
    let spec = fixture("glab_1.120.0_securefile_download.txt");

    let names: Vec<&str> = spec.cmd.args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["id"]);
}

#[test]
fn sesh_2_33_0_clone_leaves_out_what_follows_a_double_dash() {
    let spec = fixture("sesh_2.33.0_clone.txt");

    let names: Vec<(&str, bool)> = spec.cmd.args.iter().map(|a| (a.name.as_str(), a.required)).collect();
    assert_eq!(names, [("repo", true)]);
}

// `glab duo cli` appends its own section at column 0 after fang's flags. Its
// `[command]` is a real argument: the command has no subcommands to dispatch to.
#[test]
fn glab_1_120_0_duo_cli_ignores_the_tools_own_footer() {
    let spec = fixture("glab_1.120.0_duo_cli.txt");

    let longs: Vec<&str> = spec.cmd.flags.iter().map(|f| f.long[0].as_str()).collect();
    assert_eq!(longs, ["help", "install", "update", "yes"]);
    assert!(spec.cmd.subcommands.is_empty());
    assert!(!arg(&spec, "command").required);
}

// fang 0.3.0 pads with no-break spaces where later versions use plain ones.
#[test]
fn container_use_0_4_2_root_from_fang_0_3() {
    let spec = fixture("container-use_0.4.2_root.txt");

    assert_eq!(spec.bin, "container-use");
    assert_eq!(
        spec.cmd.help.as_deref(),
        Some("Container Use creates isolated development environments for AI agents. Each environment runs in its own container with dedicated git branches.")
    );
    assert_eq!(spec.cmd.subcommands.len(), 14);
    assert_eq!(spec.cmd.subcommands["diff"].help.as_deref(), Some("Show what files an agent changed"));
    assert_eq!(flag(&spec, "version").short, vec!['v']);
}

#[test]
fn container_use_0_4_2_completion_offers_its_shells() {
    let spec = fixture("container-use_0.4.2_completion.txt");

    assert_eq!(spec.cmd.subcommands.len(), 4);
    let shell = &spec.cmd.args[0];
    assert!(!shell.required);
    let choices = shell.choices.as_ref().expect("literal alternatives are choices");
    assert_eq!(choices.choices, ["bash", "zsh", "fish", "powershell"]);
    assert!(flag(&spec, "command-name").arg.is_some());
}

#[test]
fn container_use_0_4_2_delete_takes_any_number_of_environments() {
    let spec = fixture("container-use_0.4.2_delete.txt");

    assert!(!arg(&spec, "env").required);
    assert_eq!(spec.cmd.flags.len(), 2);
}

#[test]
fn skate_1_1_0_set_from_fang_0_4() {
    let spec = fixture("skate_1.1.0_set.txt");

    let names: Vec<(&str, bool)> = spec.cmd.args.iter().map(|a| (a.name.as_str(), a.required)).collect();
    assert_eq!(names, [("KEY", true), ("@DB", false), ("VALUE", false)]);
    assert!(flag(&spec, "store").arg.is_some());
}

#[test]
fn goreleaser_2_18_3_release_from_fang_1_0() {
    let spec = fixture("goreleaser_2.18.3_release.txt");

    assert_eq!(spec.cmd.flags.len(), 17);
    assert_eq!(flag(&spec, "config").short, vec!['f']);
    assert_eq!(flag(&spec, "timeout").help.as_deref(), Some("Timeout to the entire release process (1h0m0s)"));
    assert!(spec.cmd.args.is_empty());
}

#[test]
fn crush_0_98_1_root_examples_are_not_commands() {
    let spec = fixture("crush_0.98.1_root.txt");

    assert_eq!(spec.cmd.subcommands.len(), 13);
    assert_eq!(spec.cmd.subcommands["run"].help.as_deref(), Some("Run a single non-interactive prompt"));
    assert_eq!(flag(&spec, "cwd").short, vec!['c']);
}

#[test]
fn crush_0_98_1_run_takes_a_prompt() {
    let spec = fixture("crush_0.98.1_run.txt");

    assert!(!arg(&spec, "prompt").required);
    assert!(spec.cmd.subcommands.is_empty());
}

fn page(format_dir: &str, filename: &str) -> impl FnOnce() -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format_dir).join(filename);
    move || std::fs::read_to_string(path).ok()
}

// fang is detected from the binary; its root page confirms it. A page Cobra rendered
// itself — standard, or gh's own uppercase template — is read as Cobra after all.
#[test]
fn a_root_page_confirms_fang_or_hands_the_tool_back_to_cobra() {
    let fang = InputFormat::FangHelptext;
    assert_eq!(fang.confirm(page("fang-helptext", "glab_1.120.0_root.txt")), InputFormat::FangHelptext);
    assert_eq!(fang.confirm(page("fang-helptext", "container-use_0.4.2_root.txt")), InputFormat::FangHelptext);
    assert_eq!(fang.confirm(page("cobra-helptext", "mani_0.32.0_root.txt")), InputFormat::CobraHelptext);
    assert_eq!(fang.confirm(page("cobra-helptext", "gh_2.92.0_root.txt")), InputFormat::CobraHelptext);
    assert_eq!(fang.confirm(|| None), InputFormat::FangHelptext, "without a root page the detection stands");
}

#[test]
fn only_fang_asks_for_the_root_page() {
    assert_eq!(InputFormat::CobraHelptext.confirm(|| panic!("Cobra needs no confirming")), InputFormat::CobraHelptext);
}
