# Changelog

Notable changes to bract. This file is the source of GitHub Release notes —
[dist](https://opensource.axo.dev/cargo-dist/) parses the section whose heading
matches the released version.

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
