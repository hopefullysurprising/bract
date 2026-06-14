# Changelog

Notable changes to bract. This file is the source of GitHub Release notes —
[dist](https://opensource.axo.dev/cargo-dist/) parses the section whose heading
matches the released version.

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
