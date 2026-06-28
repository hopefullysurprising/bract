mod common;

use common::{
    atlassian_source, az_source, cli_source, kubectl_source, mani_source, mise_self_source,
    task_source, Session,
};

// --- Root ordering: Mise Tasks pinned on top, the rest alphabetical -----------

#[test]
fn root_tools_are_sorted_with_mise_tasks_pinned() {
    // Deliberately unsorted input order.
    let mut session = Session::new(vec![az_source(), task_source(), mani_source()], 100, 30);
    assert_eq!(session.root_names(), vec!["Mise Tasks", "az", "mani"]);
}

#[test]
fn mise_self_is_pinned_directly_under_mise_tasks() {
    // The Mise kernel leads — Tasks, then Mise's own CLI — before the alphabetical
    // toolchain, regardless of input order.
    let mut session =
        Session::new(vec![az_source(), mani_source(), mise_self_source(), task_source()], 100, 30);
    assert_eq!(session.root_names(), vec!["Mise Tasks", "Mise", "az", "mani"]);
}

// --- Mise's own CLI navigates and runs (usage spec, eager full tree) -----------

#[test]
fn mise_self_navigates_nested_commands_and_runs() {
    let mut session = Session::new(vec![mise_self_source()], 100, 30);
    // `usage generate completion` is a 3-level nested path from one spec dump.
    session.navigate(&["Mise", "generate", "completion"]);
    assert!(session.focused_runnable(), "a usage leaf is runnable");
    let spec = session.run_via_form();
    assert_eq!(spec.bin, vec!["mise"]);
    assert_eq!(spec.args.first().map(String::as_str), Some("generate"));
    assert!(spec.args.contains(&"completion".to_string()), "nested path reaches the command: {:?}", spec.args);
}

// --- Root tools gain their own --help description -----------------------------

#[test]
fn tool_gains_its_help_description() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani"]);
    let desc = session.focused_description().unwrap_or_default();
    assert!(desc.contains("repositories manager"), "tool description loaded: {desc:?}");
}

// --- Search matches descriptions, not just names ------------------------------

#[test]
fn filter_matches_on_description_text() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    session.navigate(&["az", "account"]);
    // "aks" is named cryptically; only its description mentions Kubernetes.
    session.filter("kubernetes");
    assert_eq!(session.focused_command(), Some("aks".to_string()));
}

// --- Loading is debounced: scrolling does not enqueue a fetch per item --------

#[test]
fn scrolling_does_not_load_until_settled() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    session.navigate(&["az", "account"]); // settled, nothing pending
    assert_eq!(session.pending_loads(), 0);

    session.press_down();
    session.press_down(); // moved onto unloaded siblings, but no idle tick yet
    assert_eq!(session.pending_loads(), 0, "moving the cursor must not trigger loads");
    assert!(!session.focused_loaded(), "the passed-over item isn't fetched while scrolling");

    session.pump(); // idle tick after input settles
    assert!(session.focused_loaded(), "the settled item loads on idle");
}

// --- gh's custom Cobra template now navigates and runs end-to-end -------------

#[test]
fn gh_custom_template_navigates_and_runs() {
    let mut session = Session::new(vec![cli_source()], 120, 30);
    session.navigate(&["gh", "repo", "view"]); // proves root + repo expanded lazily
    assert!(!session.focused_expandable(), "repo view is a leaf");

    session.open_run_form();
    let spec = session.run();
    assert_eq!(spec.bin, vec!["gh"]);
    assert_eq!(&spec.args[..2], &["repo", "view"]);
}

// --- The Miller-columns navigation experience renders as designed -------------

// --- Knack / az: lazy discovery + group-vs-command semantics + run assembly ---

#[test]
fn knack_group_is_an_unrunnable_branch() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    session.navigate(&["az", "account"]);
    assert!(session.focused_expandable(), "az account is a group");
    assert!(!session.focused_runnable(), "a group cannot be run directly");
}

#[test]
fn knack_leaf_runs_with_full_command_path() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    // Reaching `list` proves `az` -> `account` were lazily loaded and expanded.
    session.navigate(&["az", "account", "list"]);
    assert!(!session.focused_expandable(), "az account list is a leaf");
    assert!(session.focused_runnable());

    session.open_run_form();
    let spec = session.run();
    assert_eq!(spec.bin, vec!["az"]);
    assert_eq!(spec.args, vec!["account", "list"]);
}

// --- Cobra / mani: lazy discovery + run assembly ------------------------------

#[test]
fn cobra_leaf_runs_after_lazy_load() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "sync"]);
    session.open_run_form();
    let spec = session.run();
    // The lazily-loaded subcommand path is what we assert; flag defaults
    // (e.g. `--forks 4`) show as placeholders and are omitted unless edited.
    assert_eq!(spec.bin, vec!["mani"]);
    assert_eq!(spec.args.first().map(String::as_str), Some("sync"));
}

// --- Mise tasks: runnable branch + separator-joined run path ------------------

#[test]
fn mise_runnable_branch_runs_with_joined_path() {
    let mut session = Session::new(vec![task_source()], 100, 30);
    session.navigate(&["Mise Tasks", "app", "check"]);
    assert!(session.focused_expandable(), "app:check has subtasks");
    assert!(session.focused_runnable(), "app:check is itself a task");

    session.open_run_form();
    let spec = session.run();
    assert_eq!(spec.bin, vec!["mise", "run"]);
    assert_eq!(spec.args, vec!["app:check"]);
}

#[test]
fn mise_deep_leaf_runs_with_joined_path() {
    let mut session = Session::new(vec![task_source()], 100, 30);
    session.navigate(&["Mise Tasks", "app", "check", "api", "test", "cov"]);
    let spec = session.run_via_form();
    assert_eq!(spec.args, vec!["app:check:api:test:cov"]);
}

// --- Cobra groups are not runnable; only leaves and dual commands are ----------
// Regression guard: a pure Cobra group (subcommands only) must not be presented
// as a runnable command, while a command with its own run form stays runnable.

#[test]
fn cobra_pure_group_is_not_runnable() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "describe"]);
    assert!(session.focused_expandable(), "describe is a group");
    assert!(!session.focused_runnable(), "a pure group must not be runnable");
}

#[test]
fn cobra_dual_command_stays_runnable() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "edit"]);
    assert!(session.focused_expandable(), "edit has subcommands");
    assert!(session.focused_runnable(), "edit has its own run form, so it stays runnable");
}

#[test]
fn cobra_leaf_is_runnable() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "sync"]);
    assert!(!session.focused_expandable());
    assert!(session.focused_runnable());
}

// --- Review hardening: never run a node whose help hasn't loaded --------------
// Pressing `r` on a node still being resolved must not open a form with empty/
// wrong parameters; it should kick off the load instead.
#[test]
fn run_key_on_unloaded_node_does_not_open_form() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "describe"]); // describe loaded & focused
    session.press_down(); // move to a sibling that move_selection did NOT load
    assert!(!session.focused_loaded(), "the passed-over sibling isn't loaded yet");

    session.press_run_key();
    assert!(!session.on_form(), "running an unloaded node must not open a form");
}

// --- Review hardening: a filter that matches nothing acts on no hidden node ----
#[test]
fn enter_on_empty_filter_does_not_act_on_hidden_node() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    session.navigate(&["az", "account"]); // active column = az subgroups
    let depth = session.active_depth();

    session.filter("zzz-no-such-command");
    session.press_enter(); // selection is hidden — must be a no-op

    assert!(!session.on_form(), "Enter on an empty filter must not run a hidden node");
    assert_eq!(session.active_depth(), depth, "Enter on an empty filter must not descend");
}

// --- Positional arguments on leaf commands are fillable and reach the command --

#[test]
fn leaf_positional_argument_is_fillable_and_runs() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "exec"]);
    session.open_run_form();

    // `mani exec <command>` — the positional must be present and settable.
    assert!(session.set_field("<command>", "ls"), "the <command> positional is a form field");

    let spec = session.run();
    assert_eq!(spec.bin, vec!["mani"]);
    assert_eq!(spec.args.first().map(String::as_str), Some("exec"));
    assert!(spec.args.contains(&"ls".to_string()), "positional value reaches the command: {:?}", spec.args);
}

// --- Cobra expandability is discovered without selecting (peek one deeper) -----

#[test]
fn cobra_expandability_resolved_by_background_peek() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani"]); // tool focused; its subcommands load as Unknown
    assert!(session.unresolved_visible() > 0, "cobra children start with unknown expandability");

    session.pump(); // background peeks resolve branch-vs-leaf for every visible item
    assert_eq!(session.unresolved_visible(), 0, "no lingering grey dots after peeking");
}

// --- Parent-level flags accumulate into grouped form sections (regression) -----

#[test]
fn form_groups_parent_flags_into_their_own_sections() {
    let mut session = Session::new(vec![mani_source()], 100, 30);
    session.navigate(&["mani", "sync"]);
    session.open_run_form();

    let labels = session.form_section_labels();
    assert!(labels.contains(&String::new()), "the leaf's own parameters form an unlabelled section");
    assert!(labels.contains(&"mani".to_string()), "root-level flags are grouped under 'mani'");
}

// --- Inherited flags shown inline at every level appear once in the form -------
// kubectl repeats inherited flags under each command's `Options:` (no separate
// `Global Flags:` header), so `create` and `create deployment` both declare
// `--dry-run`/`--field-manager`/`--validate`. The form must dedup them.
#[test]
fn cobra_inherited_flags_appear_once_in_form() {
    let mut session = Session::new(vec![kubectl_source()], 120, 40);
    session.navigate(&["kubectl", "create", "deployment"]);
    session.open_run_form();

    let names = session.form_field_names();
    for flag in ["--dry-run", "--field-manager", "--validate"] {
        let count = names.iter().filter(|n| n.as_str() == flag).count();
        assert_eq!(count, 1, "{flag} must appear exactly once, got {count} in {names:?}");
    }
}

// --- A command runs with only the parameters the user actually filled ----------
// Flag defaults are placeholders, not values: an untouched `--field-manager`
// (default `kubectl-create`) must not be echoed back, which would bloat — and
// for kubectl, break — the command.
#[test]
fn value_flag_defaults_are_omitted_unless_edited() {
    let mut session = Session::new(vec![kubectl_source()], 120, 40);
    session.navigate(&["kubectl", "create", "deployment"]);
    session.open_run_form();

    assert!(session.set_field("<NAME>", "web"), "the NAME positional is fillable");
    assert!(session.set_field("--image", "nginx"), "--image is fillable");

    let spec = session.run();
    assert_eq!(spec.bin, vec!["kubectl"]);
    assert_eq!(
        spec.args,
        vec!["create", "deployment", "--image=nginx", "web"],
        "only the command path plus what the user filled — no default flags"
    );
}

// --- Search jumps to a match in a long column ---------------------------------

#[test]
fn filter_jumps_to_matching_subcommand() {
    let mut session = Session::new(vec![az_source()], 100, 30);
    session.navigate(&["az", "account"]); // active column = az's (long) subgroup list
    session.filter("acr");
    assert_eq!(session.focused_command(), Some("acr".to_string()));
}

// --- The Miller-columns navigation experience renders as designed -------------

#[test]
fn miller_columns_render() {
    let mut session = Session::new(vec![az_source()], 100, 24);
    session.navigate(&["az", "account"]);
    insta::assert_snapshot!(session.screen());
}

// The detail card for a focused leaf shows the same grouped parameters as the run
// form: the leaf's own params plus parent-level flags under their command name.
#[test]
fn leaf_card_shows_whole_tree_params() {
    let mut session = Session::new(vec![mani_source()], 100, 24);
    session.navigate(&["mani", "sync"]);
    insta::assert_snapshot!(session.screen());
}

// A runnable branch (`app:check` — a task that is also a parent) is marked with
// a distinct glyph in the column and leads its child column with a "▶ run" header,
// so its dual nature is obvious rather than buried in the footer.
#[test]
fn runnable_branch_surfaces_its_run_action() {
    let mut session = Session::new(vec![task_source()], 100, 24);
    session.navigate(&["Mise Tasks", "app", "check"]);
    assert!(session.focused_runnable() && session.focused_expandable());
    insta::assert_snapshot!(session.screen());
}

// A dual command's child column leads with the full run preview — its description
// and parameters grouped by command level (here, the parent `mani`'s flags) — the
// same content the run form shows, set apart from the subtree by a separator.
#[test]
fn dual_command_preview_shows_grouped_params_above_subtree() {
    let mut session = Session::new(vec![mani_source()], 100, 24);
    session.navigate(&["mani", "edit"]);
    assert!(session.focused_runnable() && session.focused_expandable());
    insta::assert_snapshot!(session.screen());
}

// --- Clap support: a real Clap CLI (atlassian-cli) driven through the TUI ------
// Mirrors the Cobra coverage above, but for a clap-derived tool: detection picks
// ClapHelptext, the help parser builds the tree, and navigation/forms/run work
// the same way they do for Cobra and Knack tools.

#[test]
fn atlassian_root_and_jira_are_groups() {
    let mut session = Session::new(vec![atlassian_source()], 120, 40);
    session.navigate(&["atlassian-cli"]);
    // A pure clap group (`Usage: atlassian-cli [OPTIONS] <COMMAND>`) is expandable
    // but not runnable on its own.
    assert!(session.focused_expandable(), "the root is a group");
    assert!(!session.focused_runnable(), "a pure group is not runnable");

    // Its services load lazily as children, and a nested group recurses.
    session.navigate(&["atlassian-cli", "jira"]);
    assert!(session.focused_expandable(), "jira is a nested group");
    assert!(!session.focused_runnable());
}

#[test]
fn atlassian_leaf_runs_with_its_positional() {
    let mut session = Session::new(vec![atlassian_source()], 120, 40);
    session.navigate(&["atlassian-cli", "jira", "issue", "get"]);
    assert!(session.focused_runnable(), "jira issue get is a runnable leaf");
    assert!(!session.focused_expandable(), "a leaf has no children");

    session.open_run_form();
    let fields = session.form_field_names();
    // The leaf's own positional plus the root's global flags (profile/config/debug)
    // propagate into the form via the ancestor-command hierarchy.
    assert!(fields.iter().any(|f| f == "<KEY>"), "the <KEY> positional is a form field: {fields:?}");
    assert!(fields.iter().any(|f| f == "--profile"), "root global flags propagate down: {fields:?}");

    assert!(session.set_field("<KEY>", "DEV-123"), "the KEY positional accepts a value");
    let spec = session.run();
    assert_eq!(spec.bin, vec!["atlassian-cli"]);
    // The full subcommand path and the positional value reach the command line.
    assert_eq!(&spec.args[..4], &["jira", "issue", "get", "DEV-123"], "args: {:?}", spec.args);
}
