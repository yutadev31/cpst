# cpst

`cpst` is a small, cross-platform command-line toolkit for interacting with
the system clipboard.

It provides two commands:

- `cpy` copies UTF-8 text or a PNG image from standard input to the system clipboard.
- `pst` writes the system clipboard contents to standard output.

## Installation

Install from a local checkout with Cargo:

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

Both commands use the regular system clipboard. `cpy` reads all input
from standard input and accepts UTF-8 text or PNG images; `pst` writes
clipboard text without adding a newline.

## Platform support

The system clipboard is supported on platforms supported by
[`arboard`](https://docs.rs/arboard).

On Linux, clipboard access requires a working graphical session and the
appropriate X11 or Wayland runtime. When building from source, the Nix
development shell and CI configuration provide the Wayland development
dependency.

## Development

Clone the repository and enter the Nix development shell if Nix is available:

```sh
nix develop
```

Then format, check, lint, test, and build the project:

```sh
cargo fmt --check
cargo check
cargo clippy
cargo test
cargo build
```

## License

This project is licensed under the MIT License. See
[`LICENSE.txt`](LICENSE.txt) for the full license text.
