mod common;

use helptext_parser::{InputFormat, Spec, SpecArg, SpecFlag};

fn fixture(filename: &str) -> Spec {
    common::parse_fixture(InputFormat::HashicorpCliHelptext, "hashicorp-cli-helptext", filename)
}

fn flag<'a>(spec: &'a Spec, name: &str) -> &'a SpecFlag {
    spec.cmd.flags.iter().find(|f| f.long == vec![name]).unwrap_or_else(|| panic!("no -{name}"))
}

fn flag_names(spec: &Spec) -> Vec<&str> {
    spec.cmd.flags.iter().map(|f| f.name.as_str()).collect()
}

fn arg<'a>(spec: &'a Spec, name: &str) -> &'a SpecArg {
    spec.cmd.args.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("no arg {name}: {:?}", spec.cmd.args))
}

fn choices(flag: &SpecFlag) -> Vec<String> {
    flag.arg.as_ref().and_then(|a| a.choices.as_ref()).map(|c| c.choices.clone()).unwrap_or_default()
}

// The root page is the tool's own HelpFunc: every listing it prints is a set of commands,
// whatever it calls them. Its prose is not a summary of the tool.
#[test]
fn tofu_1_12_6_root_lists_main_and_other_commands() {
    let spec = fixture("tofu_1.12.6_root.txt");

    assert_eq!(spec.bin, "tofu");
    assert_eq!(spec.cmd.help, None);
    assert_eq!(spec.cmd.subcommands.len(), 24);
    assert_eq!(spec.cmd.subcommands["plan"].help.as_deref(), Some("Show changes required by the current configuration"));
    assert_eq!(spec.cmd.subcommands["workspace"].help.as_deref(), Some("Workspace management"));
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
    assert!(flag(&spec, "chdir").global, "global options go before the subcommand, never after it");
}

#[test]
fn vault_2_2_1_root_keeps_commands_without_a_synopsis() {
    let spec = fixture("vault_2.2.1_root.txt");

    assert_eq!(spec.cmd.subcommands.len(), 33);
    assert_eq!(spec.cmd.subcommands["kv"].help.as_deref(), Some("Interact with Vault's Key-Value storage"));
    assert_eq!(spec.cmd.subcommands["events"].help, None);
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

// The framework's own BasicHelpFunc, which consul and packer keep.
#[test]
fn consul_2_1_0_root_is_the_frameworks_basic_listing() {
    let spec = fixture("consul_2.1.0_root.txt");

    assert_eq!(spec.bin, "consul");
    assert_eq!(spec.cmd.subcommands.len(), 34);
    assert_eq!(spec.cmd.subcommands["force-leave"].help.as_deref(), Some("Forces a member of the cluster to enter the \"left\" state"));
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

// A command page carries the long help text only; the one-line synopsis is its parent's.
#[test]
fn tofu_1_12_6_state_is_a_pure_group() {
    let spec = fixture("tofu_1.12.6_state.txt");

    assert_eq!(spec.cmd.help, None);
    assert!(spec.cmd.help_long.as_deref().unwrap().starts_with("This command has subcommands for advanced state management."));
    assert_eq!(spec.cmd.subcommands.len(), 7);
    assert_eq!(spec.cmd.subcommands["replace-provider"].help.as_deref(), Some("Replace provider in the state"));
    assert!(spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

#[test]
fn tofu_1_12_6_providers_runs_and_has_subcommands() {
    let spec = fixture("tofu_1.12.6_providers.txt");

    assert!(!spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.subcommands.len(), 3);
    assert_eq!(spec.cmd.subcommands["mirror"].help.as_deref(), Some("Save local copies of all required provider plugins"));
    assert!(!arg(&spec, "DIR").required);
    assert_eq!(flag_names(&spec), ["test-directory", "var", "var-file"]);
    assert!(flag(&spec, "var").arg.is_some());
}

#[test]
fn tofu_1_12_6_plan_flags() {
    let spec = fixture("tofu_1.12.6_plan.txt");

    assert!(spec.cmd.subcommands.is_empty());
    assert!(!spec.cmd.subcommand_required);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
    assert_eq!(spec.cmd.flags.len(), 27);

    let destroy = flag(&spec, "destroy");
    assert!(destroy.arg.is_none(), "a bare flag is a switch");
    assert!(destroy.help.as_deref().unwrap().starts_with("Select the \"destroy\" planning mode,"));
    assert!(destroy.help.as_deref().unwrap().ends_with("instead of the usual behavior."), "wrapped lines are joined");

    // `-refresh=false` names the value that changes something: a boolean, never a default.
    let refresh = flag(&spec, "refresh");
    assert_eq!(choices(refresh), ["true", "false"]);
    assert!(refresh.default.is_empty());

    assert!(flag(&spec, "lock-timeout").default.is_empty(), "nothing on the page tells a default from an example");
    assert!(flag(&spec, "var").arg.is_some());
}

// Prose may open with a flag's name; only a flag line has the name stand alone.
#[test]
fn tofu_1_12_6_untaint_reads_a_bare_positional() {
    let spec = fixture("tofu_1.12.6_untaint.txt");

    assert!(arg(&spec, "name").required);
    assert_eq!(spec.cmd.args.len(), 1);
    assert_eq!(
        flag_names(&spec),
        ["allow-missing", "lock", "lock-timeout", "ignore-remote-version", "var", "var-file", "json", "json-into"]
    );
}

// Vault's flags print their value type, with the description beneath, and examples
// sit deeper still: `$ vault kv get -mount=secret foo` is not a flag.
#[test]
fn vault_2_2_1_kv_get_flags_and_key() {
    let spec = fixture("vault_2.2.1_kv_get.txt");

    assert!(arg(&spec, "KEY").required);
    assert_eq!(spec.cmd.flags.len(), 24);
    assert_eq!(
        flag(&spec, "address").help.as_deref(),
        Some("Address of the Vault server. The default is https://127.0.0.1:8200. This can also be specified via the VAULT_ADDR environment variable.")
    );
    assert_eq!(flag(&spec, "address").arg.as_ref().unwrap().name, "string");
    assert!(flag(&spec, "disable-redirects").arg.is_none());
    assert_eq!(flag(&spec, "version").arg.as_ref().unwrap().name, "int");
    assert!(spec.cmd.help_long.as_deref().unwrap().contains("$ vault kv get -mount=secret foo"));
}

// The framework inserts a parent nobody registered, with fixed text and no synopsis.
#[test]
fn vault_2_2_1_operator_raft_autopilot_is_a_generated_parent() {
    let spec = fixture("vault_2.2.1_operator_raft_autopilot.txt");

    assert!(spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.help_long, None, "the framework's placeholder says nothing about the command");
    assert_eq!(spec.cmd.subcommands.len(), 3);
    assert_eq!(spec.cmd.subcommands["get-config"].help.as_deref(), Some("Returns the configuration of the autopilot subsystem under integrated storage"));
}

#[test]
fn vault_2_2_1_operator_raft_lists_a_subcommand_without_a_synopsis() {
    let spec = fixture("vault_2.2.1_operator_raft.txt");

    assert_eq!(spec.cmd.subcommands.len(), 5);
    assert_eq!(spec.cmd.subcommands["autopilot"].help, None);
    assert_eq!(spec.cmd.subcommands["list-peers"].help.as_deref(), Some("Returns the Raft peer set"));
}

#[test]
fn vault_2_2_1_agent_runs_and_has_subcommands() {
    let spec = fixture("vault_2.2.1_agent.txt");

    assert!(!spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.subcommands.len(), 1);
    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
    assert!(flag(&spec, "config").arg.is_some());
}

// Consul's section headings have no colon.
#[test]
fn consul_2_1_0_kv_put_flags_and_args() {
    let spec = fixture("consul_2.1.0_kv_put.txt");

    assert!(arg(&spec, "KEY").required);
    assert!(!arg(&spec, "DATA").required);
    assert_eq!(spec.cmd.flags.len(), 19);
    assert!(flag(&spec, "ca-file").help.as_deref().unwrap().starts_with("Path to a CA file"));
    assert!(flag(&spec, "cas").arg.is_none());
}

// A description wraps onto a line that starts with another flag's name.
#[test]
fn nomad_2_0_7_acl_binding_rule_info_reads_only_flag_lines() {
    let spec = fixture("nomad_2.0.7_acl_binding-rule_info.txt");

    assert!(arg(&spec, "acl_binding_rule_id").required);
    assert_eq!(spec.cmd.flags.len(), 13);
    assert!(flag(&spec, "ca-path").help.as_deref().unwrap().contains("If both -ca-cert and -ca-path are specified, -ca-cert is used."));
    assert!(flag(&spec, "t").arg.is_none());
}

// `[<key>=<value>]...` shows what to type, not what it is called.
#[test]
fn nomad_2_0_7_var_put_reads_the_first_usage_line() {
    let spec = fixture("nomad_2.0.7_var_put.txt");

    assert!(spec.cmd.help_long.as_deref().unwrap().starts_with("The 'var put' command is used to create or update an existing variable."));
    assert!(arg(&spec, "variable spec file reference").required);
    assert_eq!(spec.cmd.args.len(), 1, "{:?}", spec.cmd.args);
}

#[test]
fn vault_2_2_1_write_names_its_data_without_the_sample() {
    let spec = fixture("vault_2.2.1_write.txt");

    assert!(arg(&spec, "PATH").required);
    assert!(!arg(&spec, "DATA").required);
    assert_eq!(spec.cmd.args.len(), 2, "{:?}", spec.cmd.args);
}

// Only a command with subcommands has `<command>` stand for one.
#[test]
fn nomad_2_0_7_alloc_exec_keeps_its_command_argument() {
    let spec = fixture("nomad_2.0.7_alloc_exec.txt");

    assert!(arg(&spec, "allocation").required);
    assert!(arg(&spec, "command").required);
    assert!(!spec.cmd.subcommand_required);
}

#[test]
fn nomad_2_0_7_node_meta_names_an_optional_subcommand() {
    let spec = fixture("nomad_2.0.7_node_meta.txt");

    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
    assert!(!spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.subcommands.len(), 2);
}

#[test]
fn tofu_1_12_6_show_names_its_option_groups_not_arguments() {
    let spec = fixture("tofu_1.12.6_show.txt");

    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

// `([-kind <config kind> -name <config name>] | [-f FILENAME])` chooses between flags.
#[test]
fn consul_2_1_0_config_delete_offers_no_flag_group_as_an_argument() {
    let spec = fixture("consul_2.1.0_config_delete.txt");

    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
}

#[test]
fn nomad_2_0_7_plugin_lists_its_subcommand() {
    let spec = fixture("nomad_2.0.7_plugin.txt");

    assert_eq!(spec.cmd.subcommands.len(), 1);
    assert_eq!(spec.cmd.subcommands["status"].help.as_deref(), Some("Display status information about a plugin"));
}

#[test]
fn packer_1_16_2_build_flags_and_template() {
    let spec = fixture("packer_1.16.2_build.txt");

    assert!(arg(&spec, "TEMPLATE").required);
    assert_eq!(spec.cmd.flags.len(), 15);

    let on_error = flag(&spec, "on-error");
    assert_eq!(choices(on_error), ["cleanup", "abort", "ask", "run-cleanup-provisioner"]);
    assert!(on_error.help.as_deref().unwrap().starts_with("If the build fails do:"));

    assert_eq!(choices(flag(&spec, "color")), ["true", "false"]);
    assert!(flag(&spec, "machine-readable").arg.is_none());
    assert_eq!(flag(&spec, "debug").help.as_deref(), Some("Debug mode enabled for builds."));
}

// Flags listed straight under the description, with no heading of their own.
#[test]
fn nomad_2_0_7_operator_utilization_reads_flags_without_a_heading() {
    let spec = fixture("nomad_2.0.7_operator_utilization.txt");

    assert_eq!(spec.cmd.flags.len(), 13);
    assert!(flag(&spec, "message").help.as_deref().unwrap().starts_with("Provide context about the conditions"));
    assert!(spec.cmd.help_long.as_deref().unwrap().ends_with("capability."));
}

// Vault indents this page's `HTTP Options:` heading with a tab.
#[test]
fn vault_2_2_1_tpm_inspect_reads_flags_under_an_indented_heading() {
    let spec = fixture("vault_2.2.1_tpm_inspect.txt");

    assert_eq!(spec.cmd.flags.len(), 21);
    assert!(flag(&spec, "address").arg.is_some());
}

// A paragraph that wraps onto a flag's name is still prose: `-namespace CLI flag.`
#[test]
fn vault_2_2_1_namespace_create_reads_a_wrapped_flag_name_as_prose() {
    let spec = fixture("vault_2.2.1_namespace_create.txt");

    assert!(flag(&spec, "namespace").help.as_deref().unwrap().starts_with("The namespace to use for the command."));
}

#[test]
fn nomad_2_0_7_node_drain_reads_a_wrapped_flag_name_as_prose() {
    let spec = fixture("nomad_2.0.7_node_drain.txt");

    assert_eq!(flag(&spec, "enable").help.as_deref(), Some("Enable draining for the specified node."));
}

// An example indented with tabs sits deeper than any flag line.
#[test]
fn nomad_2_0_7_job_tag_apply_reads_no_flag_from_its_example() {
    let spec = fixture("nomad_2.0.7_job_tag_apply.txt");

    assert_eq!(flag(&spec, "description").help.as_deref(), Some("Specifies a description for the version. This is an optional field."));
}

#[test]
fn nomad_2_0_7_flags_spacing_off_their_values() {
    assert_eq!(choices(flag(&fixture("nomad_2.0.7_var_put.txt"), "in")), ["hcl", "json"]);
    assert!(flag(&fixture("nomad_2.0.7_node_meta_apply.txt"), "unset").arg.is_some());
}

#[test]
fn consul_2_1_0_connect_envoy_reads_prose_ending_in_a_flag_as_prose() {
    let spec = fixture("consul_2.1.0_connect_envoy.txt");

    assert!(spec.cmd.flags.iter().all(|f| !f.name.ends_with('.')), "{:?}", flag_names(&spec));
    assert!(flag(&spec, "envoy-binary").arg.is_some());
    assert!(spec.cmd.help_long.as_deref().unwrap().contains("-envoy-binary."));
}

#[test]
fn terraform_1_17_0_init_reads_a_tag_as_description() {
    let spec = fixture("terraform_1.17.0_init.txt");

    let experiment = flag(&spec, "enable-pluggable-state-storage-experiment");
    assert!(experiment.arg.is_none());
    assert!(experiment.help.as_deref().unwrap().starts_with("[EXPERIMENTAL] A flag to enable an alternative init command"));
}

// Terraform hands `stacks` to a plugin, which prints its own listing.
#[test]
fn terraform_1_17_0_stacks_lists_the_plugins_commands() {
    let spec = fixture("terraform_1.17.0_stacks.txt");

    assert!(spec.cmd.subcommand_required);
    assert_eq!(spec.cmd.subcommands.len(), 12);
    assert_eq!(spec.cmd.subcommands["deployment-group"].help.as_deref(), Some("Manage deployment groups"));
    assert!(spec.cmd.flags.iter().filter(|f| f.name != "usage").all(|f| f.global));
}

#[test]
fn terraform_1_17_0_stacks_deployment_step_show_flags() {
    let spec = fixture("terraform_1.17.0_stacks_deployment-step_show.txt");

    assert!(spec.cmd.args.is_empty(), "{:?}", spec.cmd.args);
    assert!(!flag(&spec, "json").global);
    assert!(flag(&spec, "chdir").global);
    assert_eq!(spec.cmd.flags.len(), 6);
}
