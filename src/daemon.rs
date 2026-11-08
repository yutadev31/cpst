use super::*;
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};
use std::{
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub(super) fn run(input: Vec<u8>) -> Result<(), Error> {
    let content = parse_content(&input)?;
    drop(input);
    let expected = std::sync::Arc::new(content);
    let content = expected.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    // The set operation blocks while this process owns the selection.
    std::thread::Builder::new()
        .name("cpst-clipboard-provider".to_owned())
        .spawn(move || {
            let _ = sender.send(provide_selection(content));
        })
        .map_err(|error| Error::operation("failed to start clipboard provider", error))?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut probe =
        Clipboard::new().map_err(|error| Error::operation("failed to open clipboard", error))?;
    let mut last_probe_error = None;

    // Read back the selection before telling the parent that copying succeeded.
    loop {
        match receiver.try_recv() {
            Ok(Err(error)) => return Err(error),
            Ok(Ok(())) => return Err("clipboard ownership ended before confirmation".into()),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("clipboard provider stopped before confirmation".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        let confirmed = match selection_matches(&mut probe, &expected) {
            Ok(confirmed) => confirmed,
            Err(error) => {
                last_probe_error = Some(error);
                false
            }
        };

        if confirmed {
            io::stdout()
                .write_all(b"OK\n")
                .map_err(|error| Error::operation("failed to confirm clipboard copy", error))?;
            return receiver
                .recv()
                .map_err(|error| Error::operation("clipboard provider stopped", error))?;
        }

        if Instant::now() >= deadline {
            return Err(Error::ConfirmationTimeout(last_probe_error));
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}
fn provide_selection(content: std::sync::Arc<Content>) -> Result<(), Error> {
    let mut clipboard =
        Clipboard::new().map_err(|error| Error::operation("failed to open clipboard", error))?;
    let operation = clipboard
        .set()
        .clipboard(LinuxClipboardKind::Clipboard)
        .wait();

    match content.as_ref() {
        Content::Text(text) => operation.text(text.as_str()),
        Content::Image(image) => operation.image(ImageData {
            width: image.width,
            height: image.height,
            bytes: image.bytes.as_ref().into(),
        }),
    }
    .map_err(|error| Error::operation("failed to set clipboard", error))
}

fn selection_matches(
    clipboard: &mut Clipboard,
    expected: &Content,
) -> Result<bool, arboard::Error> {
    match expected {
        Content::Text(text) => clipboard
            .get()
            .clipboard(LinuxClipboardKind::Clipboard)
            .text()
            .map(|actual| actual == *text),
        Content::Image(image) => clipboard
            .get()
            .clipboard(LinuxClipboardKind::Clipboard)
            .image()
            .map(|actual| {
                actual.width == image.width
                    && actual.height == image.height
                    && actual.bytes == image.bytes
            }),
    }
}

pub(super) fn spawn_daemon(executable: &std::path::Path, input: &[u8]) -> Result<(), Error> {
    use std::io::BufRead;

    let mut command = Command::new(executable);
    command
        .arg(DAEMON_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir("/");
    let (mut stdin, stdout) = spawn_reaped(&mut command)?;
    stdin
        .write_all(input)
        .map_err(|error| Error::operation("failed to send data to clipboard daemon", error))?;
    drop(stdin);

    let mut response = String::new();
    io::BufReader::new(stdout)
        .read_line(&mut response)
        .map_err(|error| Error::operation("failed to read clipboard daemon response", error))?;

    match response.trim_end() {
        "OK" => Ok(()),
        error if let Some(error) = error.strip_prefix("ERR ") => {
            Err(Error::Daemon(error.to_owned()))
        }
        _ => Err(Error::Daemon(
            "clipboard daemon stopped without confirming the copy".to_owned(),
        )),
    }
}

fn spawn_reaped(command: &mut Command) -> Result<(ChildStdin, ChildStdout), Error> {
    let (sender, receiver) = mpsc::sync_channel::<Child>(1);
    // Start the reaper first so a thread creation failure cannot orphan a child.
    std::thread::Builder::new()
        .name("cpst-daemon-reaper".to_owned())
        .spawn(move || {
            if let Ok(mut child) = receiver.recv()
                && let Err(error) = child.wait()
            {
                eprintln!("cpst: failed to reap clipboard daemon: {error}");
            }
        })
        .map_err(|error| Error::operation("failed to start clipboard daemon reaper", error))?;

    let mut child = command
        .spawn()
        .map_err(|error| Error::operation("failed to start clipboard daemon", error))?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    if let Err(error) = sender.send(child) {
        // A disconnected receiver means the reaper unexpectedly stopped.
        let mut child = error.0;
        child
            .kill()
            .map_err(|error| Error::operation("failed to stop unreaped clipboard daemon", error))?;
        child
            .wait()
            .map_err(|error| Error::operation("failed to reap clipboard daemon", error))?;
        return Err("clipboard daemon reaper stopped".into());
    }

    Ok((
        stdin.ok_or_else(|| Error::Daemon("failed to open clipboard daemon stdin".to_owned()))?,
        stdout
            .ok_or_else(|| Error::Daemon("failed to read clipboard daemon response".to_owned()))?,
    ))
}

pub(super) fn get_text(clipboard: &mut Clipboard) -> Result<String, Error> {
    clipboard
        .get()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text()
        .map_err(|error| Error::operation("failed to read clipboard", error))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::io::BufRead;

    #[test]
    fn reaps_child_without_waiting_for_its_lifetime() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "echo $$; read line"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let (stdin, stdout) = spawn_reaped(&mut command).unwrap();
        let mut pid = String::new();
        io::BufReader::new(stdout).read_line(&mut pid).unwrap();
        let process = std::path::PathBuf::from(format!("/proc/{}", pid.trim()));
        assert!(process.exists(), "child must remain alive after spawning");

        drop(stdin);
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.exists() {
            assert!(Instant::now() < deadline, "child was not reaped");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
