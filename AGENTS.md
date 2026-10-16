# AGENTS.md

## Project Overview

`cpst` is a Rust package that provides cross-platform CLI tools for copying
standard input to the system clipboard and pasting from the system clipboard
to standard output.

It provides two binaries:

- `cpy`: copies UTF-8 text or PNG images from stdin to the system clipboard.
- `pst`: writes the system clipboard to stdout.

Both tools use the regular system clipboard. Clipboard access is implemented
with `arboard`, and command-line parsing is handled with `clap`.

## Repository Layout

- `src/lib.rs`: shared clipboard operations, CLI options, platform handling,
  and unit tests.
- `src/bin/cpy.rs`: `cpy` binary entry point.
- `src/bin/pst.rs`: `pst` binary entry point.
- `Cargo.toml`: package metadata and Rust dependencies.
- `Cargo.lock`: locked dependency versions.
- `flake.nix`: Nix package definition and development shell.

## Development Workflow

Use the Nix development shell when available:

```sh
nix develop
```

Format and validate changes with:

```sh
cargo fmt --check
cargo check
cargo clippy
cargo test
```

Build the binaries with:

```sh
cargo build
```

Run the tools locally with stdin/stdout, for example:

```sh
printf 'hello' | cargo run --bin cpy
cargo run --bin pst
grim - | cargo run --bin cpy
```

## Development Principles

- Keep changes small and focused; do not combine them with unrelated refactoring.
- Fix root causes rather than adding temporary workarounds.
- Preserve cross-platform behavior and keep platform-specific code isolated with
  conditional compilation where appropriate.
- Do not silently ignore errors. Keep error messages actionable and include the
  operation that failed when possible.
- Prefer library or IPC-based integration over external command execution.
- Preserve the stdin/stdout streaming behavior of the CLI tools.
- When compatibility improvements would reduce readability, prioritize
  readability.
- If the task is ambiguous or lacks necessary information, ask for
  clarification before proceeding.
- After completing work, output an English commit message following the Conventional Commits specification.
