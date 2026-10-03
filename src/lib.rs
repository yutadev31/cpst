use std::io::{self, Read, Write};

use arboard::Clipboard;
use clap::Args;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Clipboard,
    Primary,
}

#[derive(Args, Debug)]
pub struct Options {
    /// Use the X11/Wayland primary selection.
    #[arg(short = 'p', long = "primary")]
    pub primary: bool,
}

impl Options {
    pub fn selection(&self) -> Selection {
        if self.primary {
            Selection::Primary
        } else {
            Selection::Clipboard
        }
    }
}

pub fn copy(selection: Selection) -> Result<(), String> {
    let text = read_stdin()?;

    #[cfg(target_os = "linux")]
    return spawn_daemon(selection, text);

    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard =
            Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
        set_text(&mut clipboard, selection, text)
    }
}

/// Runs the long-lived Linux clipboard provider used by `cpy` internally.
///
/// This is intentionally not exposed as a user-facing command. Wayland and X11
/// require the process that owns a selection to keep serving requests, so the
/// public `copy` function starts this function in a detached child process.
pub fn run_daemon(selection: Selection) -> Result<(), String> {
    let text = read_stdin()?;

    #[cfg(target_os = "linux")]
    {
        use arboard::{LinuxClipboardKind, SetExtLinux};

        let mut clipboard =
            Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
        let operation = clipboard.set();
        let operation = match selection {
            Selection::Clipboard => operation.clipboard(LinuxClipboardKind::Clipboard),
            Selection::Primary => operation.clipboard(LinuxClipboardKind::Primary),
        };

        operation
            .wait()
            .text(text)
            .map_err(|error| format!("failed to set clipboard: {error}"))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (selection, text);
        Err("the clipboard daemon is only supported on Linux".to_owned())
    }
}

fn read_stdin() -> Result<String, String> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    String::from_utf8(input)
        .map_err(|_| "stdin is not valid UTF-8; cpy only supports text".to_owned())
}

#[cfg(target_os = "linux")]
fn spawn_daemon(selection: Selection, text: String) -> Result<(), String> {
    use std::process::{Command as ProcessCommand, Stdio};

    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to locate cpy for clipboard daemon: {error}"))?;
    let mut command = ProcessCommand::new(executable);
    command
        .arg("__internal_daemonize")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir("/");
    if selection == Selection::Primary {
        command.arg("--primary");
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start clipboard daemon: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open clipboard daemon stdin".to_owned())?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|error| format!("failed to send data to clipboard daemon: {error}"))
}

pub fn paste(selection: Selection) -> Result<(), String> {
    let mut clipboard =
        Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
    let text = get_text(&mut clipboard, selection)?;
    io::stdout()
        .write_all(text.as_bytes())
        .and_then(|_| io::stdout().flush())
        .map_err(|error| format!("failed to write stdout: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn set_text(clipboard: &mut Clipboard, selection: Selection, text: String) -> Result<(), String> {
    if selection == Selection::Primary {
        return Err("--primary is only supported on Linux".to_owned());
    }
    clipboard
        .set_text(text)
        .map_err(|error| format!("failed to set clipboard: {error}"))
}

#[cfg(target_os = "linux")]
fn get_text(clipboard: &mut Clipboard, selection: Selection) -> Result<String, String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};

    let operation = clipboard.get();
    let operation = match selection {
        Selection::Clipboard => operation.clipboard(LinuxClipboardKind::Clipboard),
        Selection::Primary => operation.clipboard(LinuxClipboardKind::Primary),
    };
    operation
        .text()
        .map_err(|error| format!("failed to read clipboard: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn get_text(clipboard: &mut Clipboard, selection: Selection) -> Result<String, String> {
    if selection == Selection::Primary {
        return Err("--primary is only supported on Linux".to_owned());
    }
    clipboard
        .get_text()
        .map_err(|error| format!("failed to read clipboard: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        options: Options,
    }

    #[test]
    fn defaults_to_clipboard() {
        let cli = TestCli::try_parse_from(["test"]).unwrap();
        assert_eq!(cli.options.selection(), Selection::Clipboard);
    }

    #[test]
    fn accepts_primary_aliases() {
        let short = TestCli::try_parse_from(["test", "-p"]).unwrap();
        assert_eq!(short.options.selection(), Selection::Primary);
        let long = TestCli::try_parse_from(["test", "--primary"]).unwrap();
        assert_eq!(long.options.selection(), Selection::Primary);
    }

    #[test]
    fn rejects_positional_arguments() {
        assert!(TestCli::try_parse_from(["test", "text"]).is_err());
    }
}
