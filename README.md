# cpst

`cpst` is a small, cross-platform command-line toolkit for interacting with the system clipboard.

It provides two commands:

- `cpy` copies UTF-8 text or a PNG image from standard input to the system clipboard.
- `pst` writes the system clipboard contents to standard output.

## Installation

Install from the workspace root with Cargo:

```sh
cargo install --path .
```

If you have Nix installed, build the package with:

```sh
nix build
./result/bin/cpy --help
./result/bin/pst --help
```

You can also enter the development shell with `nix develop`.

## Usage

Copy text to the clipboard:

```sh
printf 'hello, clipboard!\n' | cpy
```

Copy a screenshot on Wayland:

```sh
grim - | cpy
```

Paste the clipboard contents:

```sh
pst
```

Both commands use the regular system clipboard. `cpy` reads all input from standard input and accepts UTF-8 text or PNG images; `pst` writes clipboard text without adding a newline.

## Platform support

The system clipboard is supported on platforms supported by [`arboard`](https://docs.rs/arboard).

On Linux and BSD, clipboard access requires a working graphical session and the appropriate X11 or Wayland runtime. When building from source, the Nix development shell and CI configuration provide the Wayland development dependency.

## Development

Enter the Nix development shell from the workspace root if Nix is available:

```sh
nix develop
```

Then format, check, lint, test, and build the package:

```sh
cargo fmt --all --check
cargo check -p cpst --locked
cargo clippy -p cpst --all-targets --all-features --locked -- -D warnings
cargo test -p cpst --locked
cargo build -p cpst --locked
```

## License

This project is licensed under the MIT License. See [`LICENSE.txt`](LICENSE.txt) for the full license text.

## Library errors

`copy`, `paste`, and `run_daemon` return `Result<(), cpst::Error>` instead of `Result<(), String>`. Match error variants when handling failures and use `std::error::Error::source` to inspect local operation failures. Use `to_string()` when a string is needed. Errors reported by the separate clipboard daemon retain its protocol message; an in-process source is unavailable across that process boundary. The CLI and daemon protocol are unchanged.

The `copy` library function requires the path to an executable that handles
`DAEMON_ARGUMENT` by calling `run_daemon`. The argument is unused on Windows and macOS. On other platforms, input validation occurs in the daemon.
