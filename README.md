# bract

**Browse and run your whole CLI toolchain from one guided TUI.**

[![release](https://img.shields.io/github/v/release/hopefullysurprising/bract?sort=semver)](https://github.com/hopefullysurprising/bract/releases)
[![license](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)

| Without bract | With bract |
| :--: | :--: |
| ![Reading kubectl --help after --help, then hitting a required-flag error](docs/demo-bare.gif) | ![Searching and filling a guided kubectl form in bract](docs/demo-bract.gif) |

A modern dev setup spans many CLIs — mise, gh, az, kubectl, … — each with its own
flags and docs. bract discovers the tools [mise](https://mise.jdx.dev) manages,
parses their `--help` **per framework** (so one parser covers many tools), and
lets you browse commands in file-explorer-style columns and fill in arguments
through a form. No memorising flags.

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

Icons: `▸` group · `◆` command that is also a group · `•` runnable command.

## How it works

- **Lazy Miller columns** over your toolchain — a subtree's `--help` is fetched
  only when you open it, on a background thread, so startup is instant even for
  a CLI as large as `az`.
- **Parsers per framework, not per tool.** One [Cobra](https://github.com/spf13/cobra)
  parser covers any Cobra CLI; a [Knack](https://github.com/microsoft/knack) parser
  covers Azure CLI. New frameworks are added once and light up every tool built on them.
- **mise is the backbone** for tool versions, task definitions, and environment.

## Works with

Any CLI built on a supported framework — plus mise's own tasks. Verified end-to-end
against the real `--help` of:

- **Cobra** — `kubectl`, `helm`, `hugo`, `rclone`, `gh`, and [hundreds more](https://github.com/spf13/cobra/blob/main/site/content/projects_using_cobra.md).
- **Knack** — `az` (Azure CLI).

## License

[GPL-3.0-only](LICENSE).
