use std::io::{self, Read, Write};

use arboard::Clipboard;

pub fn copy() -> Result<(), String> {
    let text = read_stdin()?;

    #[cfg(target_os = "linux")]
    return spawn_daemon(text);

    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard =
            Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
        clipboard
            .set_text(text)
            .map_err(|error| format!("failed to set clipboard: {error}"))
    }
}

/// Runs the long-lived Linux clipboard provider used by `cpy` internally.
///
/// This is intentionally not exposed as a user-facing command. Wayland and X11
/// require the process that owns a selection to keep serving requests, so the
/// public `copy` function starts this function in a detached child process.
pub fn run_daemon() -> Result<(), String> {
    let text = read_stdin()?;

    #[cfg(target_os = "linux")]
    {
        use arboard::{LinuxClipboardKind, SetExtLinux};

        let mut clipboard =
            Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
        clipboard
            .set()
            .clipboard(LinuxClipboardKind::Clipboard)
            .wait()
            .text(text)
            .map_err(|error| format!("failed to set clipboard: {error}"))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = text;
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
fn spawn_daemon(text: String) -> Result<(), String> {
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

pub fn paste() -> Result<(), String> {
    let mut clipboard =
        Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
    let text = get_text(&mut clipboard)?;
    io::stdout()
        .write_all(text.as_bytes())
        .and_then(|_| io::stdout().flush())
        .map_err(|error| format!("failed to write stdout: {error}"))
}

#[cfg(target_os = "linux")]
fn get_text(clipboard: &mut Clipboard) -> Result<String, String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};

    clipboard
        .get()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text()
        .map_err(|error| format!("failed to read clipboard: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn get_text(clipboard: &mut Clipboard) -> Result<String, String> {
    clipboard
        .get_text()
        .map_err(|error| format!("failed to read clipboard: {error}"))
}
