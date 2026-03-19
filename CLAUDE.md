# Bract — Vision & Principles

## What is this?

A TUI-based developer ecosystem launcher that unifies multiple CLI tools under a single guided, interactive interface. Instead of requiring developers to memorise commands, flags, and cross-tool workflows, the launcher provides step-by-step task discovery, argument provisioning, and execution — all within the terminal.

## Problem statement

Modern development workflows rely on an ensemble of CLI tools (version managers, repo managers, task runners, cloud CLIs, etc.). Each has its own syntax, flags, and documentation style. This creates friction:

- Onboarding developers into a multi-tool setup is slow and error-prone
- CLI-heavy workflows are a barrier to less terminal-savvy team members
- Configuration and parameters are duplicated across tool configs
- There's no single place to discover "what can I do?" across the toolchain

## Objectives

1. **Guided interaction** — Replace memorisation with a TUI that lets developers browse available tasks, select tools, fill in arguments through forms, and execute with confidence
2. **Single source of truth** — Tool-specific data (project tags from Mani, environments, etc.) lives in its canonical location and is never duplicated into task definitions
3. **Framework-level tool integration** — Rather than writing adapters per CLI tool, write parsers per CLI framework (Clap, Cobra, Click, Commander, Oclif, etc.) to cover many tools with few adapters
4. **Opinionated kernel** — Mise (task running + tool versioning) and Mani (multi-repo management) form the core, with other tools integrated as the ecosystem grows
5. **Cross-platform** — Runs on macOS, Linux, and Windows from a single codebase
6. **Easy to test** — Core logic is pure and I/O-free; adapters use fixture-based snapshot tests; TUI renders to in-memory buffers for headless testing

## Core principles

### No duplication of configuration

If Mani defines project tags in `mani.yaml`, the launcher reads them from there — it doesn't ask you to redefine them in a task definition. A template engine (Tera) resolves dynamic values from ecosystem manifests at runtime.

### Parsers for frameworks, not individual tools

Most modern CLIs are built with a framework (Clap, Cobra, Click, Commander, etc.). Tools built on the same framework produce predictably structured `--help` output and completion scripts. One parser per framework covers dozens of tools. For legacy tools without a detectable framework, external documentation (man pages, online docs) can be processed with LLM assistance as a fallback.

### Mise as the execution backbone

Mise handles tool version management, environment variables, and task execution. The launcher adds a presentation and orchestration layer on top — it doesn't replace Mise, it makes Mise accessible. Version locking via Mise also addresses the problem of CLI interfaces changing across versions.

### Progressive disclosure

Not every flag and subcommand needs to be exposed. The launcher surfaces curated, team-relevant workflows first. Advanced options are available but not overwhelming. The TUI guides users through the happy path while keeping the full power of the underlying tools reachable.

### Modular by design

Adapters (parsers), tool discovery, manifest resolution, and the TUI are separate concerns. New tool integrations, new choice sources, or even a web frontend can be added without rewriting the core.

## Foundation technologies

| Role | Tool | Why |
|---|---|---|
| Tool versioning & task runner | **Mise** | Structured task definitions via Usage specs; version locking; env management |
| CLI spec parsing | **usage-lib** (Rust crate) | First-class Rust library for parsing Usage specs into structured types |
| Multi-repo management | **Mani** | Project topology, tags, and repo-level operations |
| TUI rendering | **Ratatui** | Mature Rust TUI framework; supports headless rendering for tests |
| Template engine | **Tera** | Jinja2-compatible; resolves dynamic values from ecosystem manifests |
| Language | **Rust** | Single binary distribution; cross-platform; integrates natively with usage-lib and Ratatui |

## Project structure

Cargo workspace with crates in `crates/`. Current crates:
- `helptext-parser` — parses CLI help output from various framework-specific formats into `usage-lib` Spec types. Library + binary (stdin-pipeable).
- `tui` — Bract TUI application using Ratatui + tui-tree-widget. View-stack architecture (like mobile navigation controllers).

## Coding conventions

- No comments unless absolutely necessary
- No unnecessary dependencies — prefer `Debug` output over adding `serde_json` until serialisation is actually needed
- Re-export consumed third-party types (e.g. `usage-lib` types) so downstream crates don't need direct dependencies
- Prefer existing community tools/standards over custom implementations
- Separate concerns by layer: app-level actions vs component-level actions — don't leak widget navigation into the app
- Keep `main.rs` thin — domain logic (assembly, discovery) belongs in the data layer, not in the entry point
- Separate generic abstractions from specific implementations into different modules (e.g. `Source` trait in `mod.rs`, `MiseTasksSource` in its own file)
- Prefer reusable community widgets over building custom components, even if the fit isn't perfect

## Testing conventions

- Fixture-based integration tests: one file per real tool, named `<tool>_<version>_<description>.<ext>`
- Fixtures live in `tests/fixtures/<format>/` (e.g. `tests/fixtures/usage-kdl/`)
- Shared test helpers in `tests/common/mod.rs`
- Format-specific tests in `tests/<format>_test.rs`, general tests in `tests/parse_test.rs`
- Tests should be practical — validate what matters for the TUI (descriptions, args, choices), not exhaustive field checking
- Don't test third-party library behaviour (e.g. error handling in usage-lib) — only test our own logic
- New parser work follows TDD: create fixture first, write failing test, implement parser

## TUI architecture

- View-stack navigation: `App` manages a `Vec<Box<dyn View>>`, pushing/popping views like a mobile navigation controller
- Each view owns its state, rendering, and key handling — the app only handles app-level actions (Quit) and delegates the rest
- Tree navigation uses `tui-tree-widget` (0.24) — generic over any identifier type, unlimited depth
- Data model (`data/`) must stay free of Ratatui types — UI conversion happens in the view layer
- `crossterm` is re-exported by Ratatui — no direct dependency needed

## Tool discovery

- Mise tools are discovered via `mise ls --json`, filtered to `active: true`
- Tool-to-framework mapping is static (e.g. `mani → CobraHelptext`) — no auto-detection heuristic
- Each recognised tool becomes a separate `Source` instance — Mise is only the discovery mechanism
- Help invocation is wrapped via `mise exec -- <tool> --help` to ensure correct tool version
- Subcommand trees are built eagerly via recursive `--help` parsing during discovery (not lazy-loaded)
- `usage-lib` builder pattern (`SpecFlagBuilder`, `SpecCommandBuilder`, `SpecArgBuilder`) is used for programmatic `Spec` construction in parsers

## Critical rules

- **Never assume tool interfaces.** Always capture real CLI output (help text, JSON responses, etc.) before writing parsers, fixtures, or integration code. If a tool is not available in the current environment, ask the user to provide the output. Fabricating or guessing interface details is unacceptable.

## Known gotchas

- `tui-tree-widget`: `TreeState::select_first()` only works after first render. Use `select(vec![id])` to pre-select before rendering.
- `tui-tree-widget`: Both `Tree::new()` and `TreeItem::new()` return `Result` (duplicate identifier check). Handle errors, don't `unwrap()` on dynamic data.
- Styled text on highlighted rows: ensure foreground colors contrast with `highlight_style` background, or text becomes invisible.

## Useful commands

- `mise tasks --usage` — get Usage KDL spec for mise tasks (not `mise usage` which exports the entire mise CLI)
- `cat file.kdl | cargo run -- usage-kdl` — pipe content into the helptext-parser binary
