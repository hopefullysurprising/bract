# bract

**Browse, build, and re-run your whole CLI toolchain from one guided TUI.**

[![release](https://img.shields.io/github/v/release/hopefullysurprising/bract?sort=semver)](https://github.com/hopefullysurprising/bract/releases)
[![license](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)

| Without bract | With bract |
| :--: | :--: |
| ![Reading kubectl --help after --help, then hitting a required-flag error](docs/demo-bare.gif) | ![Searching and filling a guided kubectl form in bract](docs/demo-bract.gif) |

A modern dev setup spans many CLIs — mise, gh, az, kubectl, … — each with its own
flags and docs. bract works out which framework a CLI was built with by inspecting
the binary, parses its `--help` **per framework** (so one parser covers many tools),
and lets you browse commands in file-explorer-style columns and fill in arguments
through a form. No memorising flags — and it remembers what you build, so the next
run is quick.

Point it at a single CLI, or let it pick up a whole toolchain from
[mise](https://mise.jdx.dev).

## Requirements

Cross-platform (macOS, Linux, Windows). [`mise`](https://mise.jdx.dev) is needed
only to discover a toolchain automatically — naming a CLI with `--tool` doesn't
consult it at all.

## Install

```sh
mise use "github:hopefullysurprising/bract@latest"
```

Or grab a binary for your platform from the
[Releases](https://github.com/hopefullysurprising/bract/releases) page — macOS,
Linux and Windows, on both Arm and x86-64.

Either way bract is a single self-contained executable: no toolchain, no runtime,
nothing to install alongside it.

## Use

```sh
bract                          # the tools mise makes active here
bract --tool kubectl           # just this CLI — a name on PATH or a path
bract --tool gh --tool cargo   # several at once
```

Navigate, pick a command, fill the form, run it.

| Key | Action |
|-----|--------|
| `↑`/`↓` `j`/`k` | move |
| `→`/`↵` `l` | open / descend |
| `←` `h` | back |
| `r` | run the focused command (opens its form; `^r` executes) |
| `/` | search the current column |
| `q` | quit |

In a form: `↹` moves between fields, `→` accepts the greyed suggestion (your last
value), `space` toggles a flag, `^r` runs.

Icons: `▸` group · `◆` command that is also a group · `•` runnable command ·
`·` not read yet, so bract doesn't yet know which. A greyed `•` is a dead end —
usually a tool whose `--help` couldn't be read.

## Less typing, every time

- **Remembers your inputs.** Fields you fill often rise to the top, and your last
  value shows as a greyed hint you accept with `→`.
- **Clipboard.** The command you build is copied ready to paste, so re-running it
  later is a single paste.

### Pre-fill any value from the environment

Any flag or argument, on any command, can be pre-filled from an environment
variable — handy for values a CLI won't take from the environment itself (e.g.
Azure CLI's `--org`) or that you'd rather not retype:

```
BRACT_<PATH>__<PARAM>=value
```

`<PATH>` is the tool and its subcommands, `<PARAM>` the flag or argument name —
each upper-cased with `-` → `_`, joined by a **double** underscore. The path
matches as a **prefix**, so one variable can cover a whole command family, and
the most specific match wins. Booleans accept `1`/`true`/`yes`/`on`. These values
are never written to disk.

```sh
BRACT_KUBECTL__NAMESPACE=dev     # --namespace on every kubectl command
BRACT_AZ_DEVOPS__ORG=myorg       # --org for everything under `az devops`
BRACT_GH_REPO_VIEW__JSON=name    # --json for `gh repo view` only
```

## How it works

- **Lazy Miller columns** over your toolchain — a subtree's `--help` is fetched
  only when you open it, on a background thread, then cached against a fingerprint
  of the binary itself, so revisits are immediate and replacing a tool re-reads it.
- **Parsers per framework, not per tool.** One [Cobra](https://github.com/spf13/cobra)
  parser covers any Cobra CLI; [Clap](https://docs.rs/clap) covers Rust CLIs;
  [Knack](https://github.com/microsoft/knack) covers Azure CLI;
  [Usage](https://usage.jdx.dev) covers mise and usage-based CLIs. New frameworks
  are added once and light up every tool built on them.
- **The framework is read from the binary**, not guessed from its name — following
  a multi-call proxy (the `rustup` shims in `~/.cargo/bin`) through to the program
  that actually runs.
- **mise is the backbone** for tool versions, task definitions, and environment —
  when you use it.

## Headless

`bract --spec` walks the whole tree and prints it as a
[Usage](https://usage.jdx.dev) spec instead of opening the TUI, so a script — or
another program — gets one document describing everything a CLI can do:

```sh
bract --tool kubectl --spec
```

Every subcommand's `--help` is fetched, so this is deliberately thorough rather
than fast.

## Works with

Any CLI built on a supported framework — plus mise's own tasks and CLI. Verified
end-to-end against the real specs of:

- **Cobra** — `kubectl`, `helm`, `hugo`, `rclone`, `gh`, `devspace`, `mani`, and
  [hundreds more](https://github.com/spf13/cobra/blob/main/site/content/projects_using_cobra.md).
- **Clap** — `cargo`, `bat`, `fd`, `hyperfine`, `zoxide`, `starship`, `samply`,
  `atlassian-cli`, and most of the Rust CLI ecosystem.
- **Knack** — `az` (Azure CLI).
- **Usage** — `mise` itself and the [`usage`](https://usage.jdx.dev) CLI.

## Feedback

Bug reports, ideas, and questions are very welcome:

- **Bugs & feature requests** → [open an issue](https://github.com/hopefullysurprising/bract/issues/new/choose).
- **Questions & ideas** → [start a discussion](https://github.com/hopefullysurprising/bract/discussions).

bract follows an opinionated roadmap — see [CONTRIBUTING](CONTRIBUTING.md).

## License

[GPL-3.0-only](LICENSE).
