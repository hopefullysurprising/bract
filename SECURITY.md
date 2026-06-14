# Security Policy

## Reporting a vulnerability

Please report security issues **privately** — don't open a public issue.

Use GitHub's [private vulnerability reporting](https://github.com/hopefullysurprising/bract/security/advisories/new)
(the **"Report a vulnerability"** button on the repository's **Security** tab).
That keeps the details between us until a fix is out.

Helpful to include: the bract version, your OS/terminal, and the steps or input
involved. This is a small, volunteer-maintained project, so responses are
best-effort — but genuine reports are taken seriously, and credited if you'd like.

## Scope

bract runs other CLIs, reads environment variables, parses tool binaries, writes
to the clipboard, and stores form-fill history locally. Flaws in how it handles
any of those — e.g. mishandling a value it executes, or exposing data it
shouldn't — are in scope. Vulnerabilities in the tools bract *drives* belong to
those tools.

## Supported versions

Only the latest release receives fixes.
