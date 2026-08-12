# Changelog

Notable changes to bract. This file is the source of GitHub Release notes —
[dist](https://opensource.axo.dev/cargo-dist/) parses the section whose heading
matches the released version.

## [0.6.0] - 2026-08-13

### Added

- **Clap-based CLIs.** A fourth parser alongside Cobra, Knack and Usage, with
  detection straight from the binary — so Rust CLIs join the tree without any
  per-tool configuration.
- **Browse any CLI, with or without Mise.** `bract --tool <name|path>` builds a
  tool's command tree by introspecting the binary and never consults Mise.
  Repeat it to browse several at once. Tools outside the Mise ecosystem —
  anything on your PATH — are now reachable.
- **Headless mode.** `bract --spec` walks the whole tree and prints it as a
  [Usage](https://usage.jdx.dev) spec instead of opening the TUI, giving a
  reader — often another program — one document describing everything a CLI can
  do.
- **A command line of its own.** `--help` and `--version`, unknown arguments
  rejected, and `BRACT_NO_CLIPBOARD` surfaced as the documented `--no-clipboard`
  (the environment variable still works).

### Changed

- **The help cache follows the program, not its neighbours.** Entries are keyed
  on a fingerprint of the binary itself rather than the version Mise reports for
  the tool that owned the directory. A shared directory such as `~/.cargo/bin`
  holds binaries Mise never installed, so a genuine upgrade could go unnoticed;
  now replacing a tool re-reads its help, in both modes.

### Fixed

- **Multi-call dispatchers.** `~/.cargo/bin` is thirteen symlinks to a single
  `rustup`, which then runs a different program for each name. Bract read the
  router's bytes and gave every proxy rustup's framework, listing tools that
  could not work (`rust-gdb`, `rls`) and misparsing ones that could. It now asks
  the dispatcher which binary a name resolves to and introspects that.
- **Python CLIs behind a shell wrapper.** Homebrew ships `az` as a bash script
  naming its interpreter on the second line, so a shebang-only search never
  found it.
- **A flags-only tool is a leaf.** A CLI with no subcommands (gomplate) showed an
  expand arrow that expanded nothing and offered no way to reach its run form.
- **A tool's marker no longer changes under you.** Expandability is shown as
  unknown until the tool's help has actually been read, rather than guessed.
- **Help that arrives with a non-zero exit is kept.** `devspace run --help`
  prints its full help and *then* fails; that help was being discarded and the
  failure shown instead. Error messages are also stripped of colour codes, which
  previously reached the screen as raw escapes.
- **Decorated help.** A tool that banners each subcommand with rows of `#`
  (devspace) no longer has those rows become the command's description.
- **Clap parsing.** Command aliases (`build, b`) are recorded rather than
  becoming part of the name, cargo's `...` list-elision marker is no longer
  offered as a command, and a flag with an optional value
  (`--include-args[=<VALUE>]`) keeps a usable name.

## [0.5.0] - 2026-06-14

### Added

- **Mise and usage-based CLIs.** Browse and build `mise`'s own commands — pinned
  right under Mise Tasks — and any CLI built on the [Usage](https://usage.jdx.dev)
  spec, via a third parser alongside Cobra and Knack.
- **Form memory.** Fields you fill often rise to the top, and your last value for
  a field is offered as a greyed hint you accept with `→`.
- **Environment-variable parameters.** Pre-fill any flag or argument from
  `BRACT_<PATH>__<PARAM>` — set once, applied across a whole command family, and
  never written to disk. Useful for values a CLI won't take from the environment
  itself (e.g. Azure CLI's `--org`).
- **Clipboard.** The command you build is copied ready to paste (a native tool
  with an OSC 52 fallback that also works over SSH), so re-running it later is a
  single paste.
- **Opt-outs** `BRACT_NO_MEMORY` and `BRACT_NO_CLIPBOARD`, for demos, CI, or privacy.

### Changed

- **Instant navigation.** A subtree's `--help` is cached per tool version, and
  cache hits resolve synchronously — so revisiting a tool is immediate, with no
  loading spinner.

## [0.4.1] - 2026-06-13

### Added

- Verified Cobra parsing end-to-end against `kubectl`, `helm`, `hugo`, `rclone`,
  and `gh`, including universal (fat) macOS binaries.
- Clear hint and clean exit when bract is launched without a terminal attached.
- README with side-by-side demos contrasting the bare-CLI and guided-form flows.

### Fixed

- Run forms no longer echo back unedited flag defaults — defaults show as
  placeholders and are omitted unless you change them.
- Flags inherited at several command levels now appear once in the form.
- Value flags are passed as `--flag=value`, so optional-value flags such as
  kubectl's `--dry-run` bind their value instead of being read as a positional.

## [0.4.0]

First tracked release. See the
[releases page](https://github.com/hopefullysurprising/bract/releases) for
earlier versions.
