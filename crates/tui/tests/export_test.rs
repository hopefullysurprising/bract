mod common;

use bract::data::export::usage_specs;
use common::{gomplate_source, kubectl_source, mani_source, mise_self_source, samply_source};
use helptext_parser::InputFormat;

#[test]
fn a_tools_whole_tree_is_emitted_not_just_its_root() {
    let specs = usage_specs(vec![mani_source()]);
    let spec = specs.first().expect("mani produces a spec");

    assert_eq!(spec.name, "mani");
    for command in ["sync", "run", "exec", "describe", "edit"] {
        assert!(spec.cmd.subcommands.contains_key(command), "tree has `{command}`");
    }
}

#[test]
fn a_commands_own_flags_and_description_survive() {
    let specs = usage_specs(vec![mani_source()]);
    let run = specs[0].cmd.subcommands.get("run").expect("mani run");

    assert_eq!(run.help.as_deref(), Some("Run tasks."));
    assert!(!run.flags.is_empty(), "run carries its flags");
}

// A flags-only tool is a complete document in itself: no subcommands, but its
// options are the whole point of emitting it.
#[test]
fn a_tool_without_subcommands_still_emits_its_flags() {
    let specs = usage_specs(vec![gomplate_source()]);
    let spec = specs.first().expect("gomplate produces a spec");

    assert!(spec.cmd.subcommands.is_empty(), "nothing to nest");
    assert!(
        spec.cmd.flags.iter().any(|f| f.long.iter().any(|l| l == "datasource")),
        "its flags are the document"
    );
}

// The output is only useful if it is a usage spec in fact and not just in shape.
// Rendering it and reading it back with our own usage-kdl parser is the check.
#[test]
fn what_is_emitted_parses_back_as_a_usage_spec() {
    let specs = usage_specs(vec![mani_source()]);
    let rendered = format!("{}", specs[0]);

    let reparsed = helptext_parser::parse(InputFormat::UsageKdl, &rendered)
        .unwrap_or_else(|e| panic!("emitted spec must parse: {e}\n---\n{rendered}"));

    assert_eq!(reparsed.name, "mani");
    for command in ["sync", "run", "exec"] {
        assert!(
            reparsed.cmd.subcommands.contains_key(command),
            "`{command}` survives the round trip"
        );
    }
}

// Anything a parser puts in a flag's *name* ends up in the rendered spec, where
// usage's own grammar has to accept it. Clap writes an optional value into the
// name — `--include-args[=<INCLUDE_ARGS>]` — and carrying that through produced a
// spec no parser would read. A Cobra tool cannot exercise this, so the round trip
// needs a clap tool that has one.
#[test]
fn an_optional_value_flag_survives_the_round_trip() {
    let specs = usage_specs(vec![samply_source()]);
    let rendered = format!("{}", specs[0]);

    let reparsed = helptext_parser::parse(InputFormat::UsageKdl, &rendered)
        .unwrap_or_else(|e| panic!("emitted spec must parse: {e}\n---\n{rendered}"));

    let record = reparsed.cmd.subcommands.get("record").expect("samply record");
    assert!(
        record.flags.iter().any(|f| f.long.iter().any(|l| l == "include-args")),
        "the flag keeps a name usage can read"
    );
}

// Every tool's tree grows in the same bag of tasks, and a command is counted out
// as soon as its own help is parsed — never waiting for its subtree. Were the
// walk judged finished a moment too early, before a completed fetch had put its
// children in, a subtree would go missing and which one would vary by timing.
#[test]
fn every_tool_is_walked_to_its_leaves_in_one_pass() {
    let specs = usage_specs(vec![kubectl_source(), mani_source(), gomplate_source()]);
    assert_eq!(specs.len(), 3, "every tool emits a spec");

    let kubectl = specs.iter().find(|s| s.name == "kubectl").expect("kubectl emitted");
    let create = kubectl.cmd.subcommands.get("create").expect("kubectl create");
    let deployment =
        create.subcommands.get("deployment").expect("kubectl create deployment");
    assert!(!deployment.flags.is_empty(), "a third-level command carries its own flags");

    let mani = specs.iter().find(|s| s.name == "mani").expect("mani emitted");
    assert!(mani.cmd.subcommands.contains_key("run"), "a second tool's tree survives too");
}

// Mise Tasks, mise's own CLI and usage-lib tools hand over their whole tree in
// one dump, every child already loaded. The walk asked the source for each child
// regardless, got the empty "you already have that" answer, and believed it —
// emitting bare names with no nesting, no help, no flags, and every command
// marked as needing a subcommand it does not have.
#[test]
fn a_tree_delivered_in_one_dump_survives_intact() {
    let specs = usage_specs(vec![mise_self_source()]);
    let spec = specs.first().expect("mise produces a spec");

    let generate = spec.cmd.subcommands.get("generate").expect("mise generate");
    assert_eq!(
        generate.help.as_deref(),
        Some("Generate completions, documentation, and other artifacts from usage specs"),
        "a group keeps the description its dump carried"
    );
    assert!(
        generate.subcommands.contains_key("completion"),
        "a nested subcommand is not dropped"
    );

    let lint = spec.cmd.subcommands.get("lint").expect("mise lint");
    assert!(!lint.subcommand_required, "a runnable leaf is not marked as needing a subcommand");
    assert!(
        lint.flags.iter().any(|f| f.long.iter().any(|l| l == "format")),
        "a leaf keeps its own flags"
    );
}
