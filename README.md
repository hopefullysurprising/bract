# bract

**Browse, build, and re-run your whole CLI toolchain from one guided TUI.**

[![release](https://img.shields.io/github/v/release/hopefullysurprising/bract?sort=semver)](https://github.com/hopefullysurprising/bract/releases)
[![license](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)

| Without bract | With bract |
| :--: | :--: |
| ![Reading kubectl --help after --help, then hitting a required-flag error](docs/demo-bare.gif) | ![Searching and filling a guided kubectl form in bract](docs/demo-bract.gif) |

A modern dev setup spans many CLIs — mise, gh, az, kubectl, … — each with its own
flags and docs. bract discovers the tools [mise](https://mise.jdx.dev) manages,
parses their `--help` **per framework** (so one parser covers many tools), and lets
you browse commands in file-explorer-style columns and fill in arguments through a
form. No memorising flags — and it remembers what you build, so the next run is quick.

## Requirements

bract drives mise — it needs [`mise`](https://mise.jdx.dev) installed with some
tools active in the current directory. Cross-platform (macOS, Linux, Windows).

## Install

```sh
# via mise (recommended)
mise use "github:hopefullysurprising/bract@latest"

# or with cargo
cargo install --git https://github.com/hopefullysurprising/bract --locked bract
```

Prebuilt binaries are also on the [Releases](https://github.com/hopefullysurprising/bract/releases) page.

## Use

Run `bract` in a project. Navigate, pick a command, fill the form, run it.

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

Icons: `▸` group · `◆` command that is also a group · `•` runnable command.

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
  only when you open it, on a background thread, then cached per tool version, so
  startup is instant and revisits are immediate.
- **Parsers per framework, not per tool.** One [Cobra](https://github.com/spf13/cobra)
  parser covers any Cobra CLI; [Knack](https://github.com/microsoft/knack) covers
  Azure CLI; [Usage](https://usage.jdx.dev) covers mise and usage-based CLIs. New
  frameworks are added once and light up every tool built on them.
- **mise is the backbone** for tool versions, task definitions, and environment.

## Works with

Any CLI built on a supported framework — plus mise's own tasks and CLI. Verified
end-to-end against the real specs of:

- **Cobra** — `kubectl`, `helm`, `hugo`, `rclone`, `gh`, and [hundreds more](https://github.com/spf13/cobra/blob/main/site/content/projects_using_cobra.md).
- **Knack** — `az` (Azure CLI).
- **Usage** — `mise` itself and the [`usage`](https://usage.jdx.dev) CLI.

## License

[GPL-3.0-only](LICENSE).
