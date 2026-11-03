mod common;

use helptext_parser::{InputFormat, Spec, SpecArg, SpecFlag};

fn fixture(filename: &str) -> Spec {
    common::parse_fixture(InputFormat::ArgparseHelptext, "argparse-helptext", filename)
}

fn longs(spec: &Spec) -> Vec<&str> {
    spec.cmd.flags.iter().flat_map(|f| f.long.iter().map(String::as_str)).collect()
}

fn flag<'a>(spec: &'a Spec, long: &str) -> &'a SpecFlag {
    spec.cmd.flags.iter().find(|f| f.long.iter().any(|l| l == long)).unwrap_or_else(|| panic!("no --{long} in {:?}", longs(spec)))
}

fn arg<'a>(spec: &'a Spec, name: &str) -> &'a SpecArg {
    spec.cmd.args.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("no arg {name}: {:?}", spec.cmd.args))
}

fn arg_name(flag: &SpecFlag) -> Option<&str> {
    flag.arg.as_ref().map(|a| a.name.as_str())
}

fn choices(flag: &SpecFlag) -> Vec<&str> {
    flag.arg.as_ref().and_then(|a| a.choices.as_ref()).map(|c| c.choices.iter().map(String::as_str).collect()).unwrap_or_default()
}

fn children(spec: &Spec) -> Vec<&str> {
    spec.cmd.subcommands.keys().map(String::as_str).collect()
}

fn help<'a>(spec: &'a Spec, child: &str) -> Option<&'a str> {
    spec.cmd.subcommands[child].help.as_deref()
}

/// What the form offers for a flag, independent of how the page laid it out.
fn form(spec: &Spec) -> Vec<String> {
    spec.cmd.flags.iter().map(|f| format!("{:?} {:?} {:?} {:?} {:?}", f.short, f.long, arg_name(f), choices(f), f.help)).collect()
}

// A subcommand added without `help=` is left out of the listing, though its name
// still sits in the `{…}` choices: pre-commit's internal `hook-impl`.
#[test]
fn pre_commit_4_7_0_root_lists_its_subcommands_but_not_a_hidden_one() {
    let spec = fixture("pre-commit_4.7.0_root.txt");

    assert_eq!(spec.bin, "pre-commit");
    assert_eq!(children(&spec).len(), 15, "{:?}", children(&spec));
    assert!(!children(&spec).contains(&"hook-impl"));
    assert_eq!(help(&spec, "run"), Some("Run hooks."));
    assert_eq!(
        help(&spec, "install-hooks"),
        Some("Install hook environments for all environments in the config file. You may find `pre-commit install --install-hooks` more useful.")
    );
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "the subcommand slot is not an argument: {:?}", spec.cmd.args);
}

// Python 3.12 wraps the `...` after the choices onto a line of its own.
#[test]
fn pre_commit_4_7_0_root_under_python_3_12_still_reads_the_subcommand_slot() {
    let spec = fixture("pre-commit_4.7.0_root_python3.12.txt");

    assert_eq!(children(&spec).len(), 15);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

// `add_subparsers(metavar='TYPE')` names the slot instead of listing its choices.
#[test]
fn ansible_galaxy_2_21_5_root_names_its_subcommand_slot_by_metavar() {
    let spec = fixture("ansible-galaxy_2.21.5_root.txt");

    assert_eq!(children(&spec), ["collection", "role"]);
    assert_eq!(help(&spec, "role"), Some("Manage an Ansible Galaxy role."));
    assert_eq!(spec.cmd.help.as_deref(), Some("Perform various Role and Collection related operations."));
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

#[test]
fn ansible_galaxy_2_21_5_collection_is_a_group_one_level_down() {
    let spec = fixture("ansible-galaxy_2.21.5_collection.txt");

    assert_eq!(children(&spec), ["download", "init", "build", "publish", "install", "list", "verify"]);
    assert_eq!(help(&spec, "install"), Some("Install collection(s) from file(s), URL(s) or Ansible Galaxy"));
    assert!(spec.cmd.subcommand_required);
}

// The listing above a command carries its summary; the command's own page carries a
// longer description, which must not replace that summary in the tree.
#[test]
fn ansible_galaxy_2_21_5_collection_install_leaves_the_summary_to_its_parent() {
    let spec = fixture("ansible-galaxy_2.21.5_collection_install.txt");

    assert_eq!(spec.cmd.help, None);
    assert!(spec.cmd.help_long.as_deref().is_some_and(|d| d.starts_with("Install collection(s) from file(s), URL(s) or Ansible Galaxy to the first")));
    assert!(spec.cmd.subcommands.is_empty());
    assert!(!spec.cmd.subcommand_required);

    let collection = arg(&spec, "collection_name");
    assert!(!collection.required && collection.var, "[collection_name ...] is optional and repeatable");

    let server = flag(&spec, "server");
    assert_eq!(server.short, ['s']);
    assert_eq!(arg_name(server), Some("API_SERVER"));
    assert_eq!(flag(&spec, "token").long, ["token", "api-key"]);
    assert_eq!(arg_name(flag(&spec, "ignore-certs")), None);
    let codes = choices(flag(&spec, "ignore-signature-status-code"));
    assert_eq!((codes.len(), codes[0]), (16, "EXPSIG"));
}

// The positional shares its name with a command word of the program:
// `ansible-galaxy collection download [collection ...]`.
#[test]
fn ansible_galaxy_2_21_5_collection_download_reads_its_positional_not_its_programs_name() {
    let spec = fixture("ansible-galaxy_2.21.5_collection_download.txt");

    let collection = arg(&spec, "collection");
    assert!(!collection.required && collection.var);
}

// Ansible's metavar for the role is `role_name[,version]`, brackets and all.
#[test]
fn ansible_galaxy_2_21_5_role_info_takes_roles_named_with_brackets() {
    let spec = fixture("ansible-galaxy_2.21.5_role_info.txt");

    assert_eq!(spec.cmd.args.len(), 1, "{:?}", spec.cmd.args);
    let role = &spec.cmd.args[0];
    assert_eq!(role.name, "role_name,version");
    assert!(role.required && role.var, "role_name[,version] [role_name[,version] ...] takes one or more");
}

// Before Python 3.13 the metavar follows every option string: `-s API_SERVER,
// --server API_SERVER`, not `-s, --server API_SERVER`.
#[test]
fn a_python_3_12_page_repeating_the_metavar_offers_the_same_form() {
    let current = fixture("ansible-galaxy_2.21.5_collection_install.txt");
    let older = fixture("ansible-galaxy_2.21.5_collection_install_python3.12.txt");

    assert_eq!(form(&older), form(&current));
}

// Before Python 3.10 the options section is headed `optional arguments:`.
#[test]
fn ansible_galaxy_2_15_13_collection_install_under_python_3_9() {
    let spec = fixture("ansible-galaxy_2.15.13_collection_install_python3.9.txt");

    let server = flag(&spec, "server");
    assert_eq!((server.short.as_slice(), arg_name(server)), (['s'].as_slice(), Some("API_SERVER")));
    assert!(!arg(&spec, "collection_name").required);
}

#[test]
fn pre_commit_4_7_0_run_reads_choices_values_and_bare_switches() {
    let spec = fixture("pre-commit_4.7.0_run.txt");

    assert_eq!(spec.cmd.flags.len(), 23, "{:?}", longs(&spec));
    assert!(!arg(&spec, "hook").required);
    assert_eq!(choices(flag(&spec, "color")), ["auto", "always", "never"]);
    assert_eq!(choices(flag(&spec, "hook-stage")).len(), 11);
    assert_eq!(arg_name(flag(&spec, "files")), Some("FILES"));

    let verbose = flag(&spec, "verbose");
    assert_eq!((verbose.short.as_slice(), arg_name(verbose), verbose.help.as_deref()), (['v'].as_slice(), None, None));
    assert_eq!(flag(&spec, "all-files").short, ['a']);
    assert_eq!(flag(&spec, "show-diff-on-failure").help.as_deref(), Some("When hooks fail, run `git diff` directly afterward."));

    let from_ref = flag(&spec, "from-ref");
    assert_eq!((from_ref.long.as_slice(), from_ref.short.as_slice()), (["from-ref", "source"].map(String::from).as_slice(), ['s'].as_slice()));
    assert_eq!(arg_name(from_ref), Some("FROM_REF"));
}

// Python 3.14 colours help whenever FORCE_COLOR is set, pipe or no pipe.
#[test]
fn a_page_forced_into_colour_offers_the_same_form_as_the_plain_one() {
    let plain = fixture("pre-commit_4.7.0_run.txt");
    let coloured = fixture("pre-commit_4.7.0_run_force-color.txt");

    assert_eq!(form(&coloured), form(&plain));
    assert_eq!(format!("{:?}", coloured.cmd.args), format!("{:?}", plain.cmd.args));
}

#[test]
fn pre_commit_4_3_0_run_under_python_3_9() {
    let spec = fixture("pre-commit_4.3.0_run_python3.9.txt");

    let config = flag(&spec, "config");
    assert_eq!((config.short.as_slice(), arg_name(config)), (['c'].as_slice(), Some("CONFIG")));
    assert!(!arg(&spec, "hook").required);
}

#[test]
fn pre_commit_4_7_0_hazmat_is_a_group_with_options_of_its_own() {
    let spec = fixture("pre-commit_4.7.0_hazmat.txt");

    assert_eq!(children(&spec), ["cd", "ignore-exit-code", "n1"]);
    assert_eq!(help(&spec, "n1"), Some("run the command once per filename"));
    assert!(spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.help, None);
    assert_eq!(choices(flag(&spec, "color")), ["auto", "always", "never"]);
}

// A group's description sits under its heading at the same indent as an entry.
#[test]
fn ansible_playbook_2_21_5_reads_titled_option_groups_but_not_their_descriptions() {
    let spec = fixture("ansible-playbook_2.21.5_root.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("Runs Ansible playbooks, executing the defined tasks on the targeted hosts."));
    assert_eq!(spec.cmd.args.len(), 1, "{:?}", spec.cmd.args);
    let playbook = arg(&spec, "playbook");
    assert!(playbook.required && playbook.var, "playbook [playbook ...] takes one or more");

    let key = flag(&spec, "private-key");
    assert_eq!((key.long.as_slice(), arg_name(key)), (["private-key", "key-file"].map(String::from).as_slice(), Some("PRIVATE_KEY_FILE")));
    assert!(longs(&spec).contains(&"become"), "Privilege Escalation Options: {:?}", longs(&spec));
}

#[test]
fn ansible_inventory_2_21_5_reads_a_described_group_of_actions() {
    let spec = fixture("ansible-inventory_2.21.5_root.txt");

    assert_eq!(spec.cmd.args.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["group"]);
    assert!(!arg(&spec, "group").required);
    assert_eq!(arg_name(flag(&spec, "host")), Some("HOST"));
    assert!(longs(&spec).contains(&"graph") && longs(&spec).contains(&"list"));
}

// textwrap breaks a long word after its hyphen: `--metadata-` / `dump.`
#[test]
fn ansible_doc_2_21_5_reads_neither_its_epilog_nor_a_hyphen_break_as_a_space() {
    let spec = fixture("ansible-doc_2.21.5_root.txt");

    assert_eq!(spec.cmd.help.as_deref(), Some("plugin documentation tool"));
    assert!(!spec.cmd.help_long.as_deref().unwrap_or_default().contains("man pages"));
    let plugin = arg(&spec, "plugin");
    assert!(!plugin.required && plugin.var);

    let kind = flag(&spec, "type");
    assert_eq!(kind.short, ['t']);
    assert!(choices(kind).contains(&"module"), "{:?}", choices(kind));
    assert!(flag(&spec, "no-fail-on-errors").help.as_deref().is_some_and(|h| h.contains("Only used for --metadata-dump. Do not fail")));
}

#[test]
fn ansible_vault_2_21_5_root_lists_subcommands_under_their_choices() {
    let spec = fixture("ansible-vault_2.21.5_root.txt");

    assert_eq!(children(&spec).len(), 7);
    assert_eq!(help(&spec, "encrypt_string"), Some("Encrypt a string"));
    assert_eq!(spec.cmd.help.as_deref(), Some("encryption/decryption utility for Ansible data files"));
}

#[test]
fn ansible_vault_2_21_5_create_takes_any_number_of_files() {
    let spec = fixture("ansible-vault_2.21.5_create.txt");

    let file = arg(&spec, "file_name");
    assert!(!file.required && file.var);
    assert_eq!(file.help.as_deref(), Some("Filename"));
}

#[test]
fn ansible_config_2_21_5_root_joins_a_wrapped_subcommand_summary() {
    let spec = fixture("ansible-config_2.21.5_root.txt");

    assert_eq!(children(&spec), ["list", "dump", "view", "init", "validate"]);
    assert_eq!(
        help(&spec, "validate"),
        Some("Validate the configuration file and environment variables. By default it only checks the base settings without accounting for plugins (see -t).")
    );
}

// yq replaces its usage line and then prints `jq --help`, whose `Command options:`
// block is laid out like an argparse section. yq hides those options from its own
// help, so they are not its form.
#[test]
fn yq_4_4_5_root_reads_its_own_options_and_not_the_jq_help_it_appends() {
    let spec = fixture("yq_4.4.5_root.txt");

    assert_eq!(spec.bin, "yq");
    assert_eq!(spec.cmd.help.as_deref(), Some("yq: Command-line YAML processor - jq wrapper for YAML documents"));
    assert_eq!(spec.cmd.flags.len(), 11, "{:?}", longs(&spec));
    assert!(!longs(&spec).contains(&"null-input"));

    let output = flag(&spec, "yaml-output");
    assert_eq!((output.long.as_slice(), output.short.as_slice()), (["yaml-output", "yml-output"].map(String::from).as_slice(), ['y'].as_slice()));
    assert_eq!(choices(flag(&spec, "yaml-output-grammar-version")), ["1.1", "1.2"]);
    assert_eq!(arg_name(flag(&spec, "width")), Some("WIDTH"));

    assert!(!arg(&spec, "jq_filter").required);
    let files = arg(&spec, "files");
    assert!(!files.required && files.var);
    assert!(spec.cmd.subcommands.is_empty() && !spec.cmd.subcommand_required);
}

#[test]
fn yq_4_4_5_root_under_python_3_9_offers_the_same_form() {
    assert_eq!(form(&fixture("yq_4.4.5_root_python3.9.txt")), form(&fixture("yq_4.4.5_root.txt")));
}
