# Bract

A TUI-based developer ecosystem launcher that unifies multiple CLI tools under a single guided, interactive interface.

## UX decisions

### Filtered flags

Flags that exist purely for CLI infrastructure (`--help`, `--version`) are not presented in the form view. These flags are not actionable in a guided TUI context and would add noise to every command's form.
