# cpst

A lightweight, cross-platform clipboard utility written in Rust.

`cpst` provides two simple commands for interacting with the system clipboard:

- **`cpy`** — Copy standard input to the clipboard.
- **`pst`** — Paste clipboard contents to standard output.

Supports UTF-8 text and PNG images.

## Installation

### Cargo

```sh
cargo install --git https://github.com/yutadev31/cpst
```

### Nix

```sh
nix profile install github:yutadev31/cpst
```

## Usage

### Copy text

```sh
echo "Hello, world!" | cpy
```

```sh
cpy < file.txt
```

### Paste text

```sh
pst
```

```sh
pst > file.txt
```

### Copy an image

```sh
cpy < image.png
```

Copy a screenshot using [shot](https://github.com/yutadev31/shot):

```sh
shot | cpy
```

### Paste an image

```sh
pst > image.png
```

Images are written as PNG only when standard output is redirected to a file or pipe. Binary image data is never written directly to a terminal.

When the clipboard provides both text and an image, `pst` prefers text.

## Platform Support

`cpst` uses [arboard](https://github.com/1Password/arboard) for clipboard access.

It supports Windows, macOS, and Unix-like systems with a supported graphical clipboard backend, including X11 and Wayland.

**Note**: macOS support is currently untested due to a lack of access to macOS hardware.

## Development

A Nix development environment is available:

```sh
nix develop
```

Run checks with Cargo:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## License

Licensed under the [MIT License](LICENSE.txt).
