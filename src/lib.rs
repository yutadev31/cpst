use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use arboard::{Clipboard, ImageData};
#[cfg(target_os = "linux")]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

/// A clipboard operation failure, retaining its underlying cause when available.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A local I/O, image, or clipboard operation failed.
    #[error("{operation}: {source}")]
    Operation {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Input is neither UTF-8 text nor a PNG image.
    #[error("stdin is neither UTF-8 text nor a PNG image")]
    InvalidText(#[source] std::str::Utf8Error),
    /// The provider did not confirm ownership before the deadline.
    #[error("timed out waiting to confirm clipboard copy{detail}", detail = .0.as_ref().map(|error| format!(": {error}")).unwrap_or_default())]
    ConfirmationTimeout(#[source] Option<arboard::Error>),
    /// The provider reported a protocol or lifecycle failure.
    #[error("{0}")]
    Daemon(String),
}

impl Error {
    fn operation(
        operation: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Operation {
            operation,
            source: Box::new(source),
        }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Daemon(message.to_owned())
    }
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone)]
enum Content {
    Text(String),
    Image(ImageData<'static>),
}

/// Copies UTF-8 text or PNG pixels from stdin to the clipboard.
///
/// # Errors
/// Returns an error if input cannot be read or decoded, or clipboard ownership fails.
pub fn copy() -> Result<(), Error> {
    let input = read_stdin()?;

    #[cfg(target_os = "linux")]
    {
        parse_content(&input)?;
        spawn_daemon(&input)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let content = parse_content(&input)?;
        let mut clipboard = Clipboard::new()
            .map_err(|error| Error::operation("failed to open clipboard", error))?;

        match content {
            Content::Text(text) => clipboard.set_text(text),
            Content::Image(image) => clipboard.set_image(image),
        }
        .map_err(|error| Error::operation("failed to set clipboard", error))
    }
}

/// Runs the long-lived Linux clipboard provider used by `cpy` internally.
///
/// This is intentionally not exposed as a user-facing command. Wayland and X11
/// require the process that owns a selection to keep serving requests, so the
/// public `copy` function starts this function in a detached child process.
///
/// # Errors
/// Returns an error if the provider cannot start, serve, or confirm the selection.
pub fn run_daemon() -> Result<(), Error> {
    let input = read_stdin()?;

    #[cfg(target_os = "linux")]
    {
        let content = parse_content(&input)?;
        drop(input);
        let expected = content.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        // The set operation blocks while this process owns the selection.
        std::thread::Builder::new()
            .name("cpst-clipboard-provider".to_owned())
            .spawn(move || {
                let _ = sender.send(provide_selection(content));
            })
            .map_err(|error| Error::operation("failed to start clipboard provider", error))?;

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut probe = Clipboard::new()
            .map_err(|error| Error::operation("failed to open clipboard", error))?;
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

    #[cfg(not(target_os = "linux"))]
    {
        let _ = input;
        Err(Error::Daemon(
            "the clipboard daemon is only supported on Linux".to_owned(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn provide_selection(content: Content) -> Result<(), Error> {
    let mut clipboard =
        Clipboard::new().map_err(|error| Error::operation("failed to open clipboard", error))?;
    let operation = clipboard
        .set()
        .clipboard(LinuxClipboardKind::Clipboard)
        .wait();

    match content {
        Content::Text(text) => operation.text(text),
        Content::Image(image) => operation.image(image),
    }
    .map_err(|error| Error::operation("failed to set clipboard", error))
}

#[cfg(target_os = "linux")]
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

fn read_stdin() -> Result<Vec<u8>, Error> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| Error::operation("failed to read stdin", error))?;
    Ok(input)
}

fn parse_content(input: &[u8]) -> Result<Content, Error> {
    if input.starts_with(PNG_SIGNATURE) {
        let image = image::load_from_memory_with_format(input, image::ImageFormat::Png)
            .map_err(|error| Error::operation("failed to decode PNG from stdin", error))?
            .into_rgba8();
        let (width, height) = image.dimensions();
        return Ok(Content::Image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: image.into_raw().into(),
        }));
    }

    let text = std::str::from_utf8(input).map_err(Error::InvalidText)?;
    Ok(Content::Text(text.to_owned()))
}

#[cfg(target_os = "linux")]
fn spawn_daemon(input: &[u8]) -> Result<(), Error> {
    use std::io::BufRead;
    use std::process::{Command as ProcessCommand, Stdio};

    let executable = std::env::current_exe()
        .map_err(|error| Error::operation("failed to locate cpy for clipboard daemon", error))?;
    let mut command = ProcessCommand::new(executable);
    command
        .arg("__internal_daemonize")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir("/");
    let mut child = command
        .spawn()
        .map_err(|error| Error::operation("failed to start clipboard daemon", error))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Daemon("failed to open clipboard daemon stdin".to_owned()))?;
    stdin
        .write_all(input)
        .map_err(|error| Error::operation("failed to send data to clipboard daemon", error))?;
    drop(stdin);

    let mut response = String::new();
    io::BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| Error::Daemon("failed to read clipboard daemon response".to_owned()))?,
    )
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

/// Writes clipboard text to stdout.
///
/// # Errors
/// Returns an error if the clipboard cannot be read or stdout cannot be written.
pub fn paste() -> Result<(), Error> {
    let mut clipboard =
        Clipboard::new().map_err(|error| Error::operation("failed to open clipboard", error))?;
    let text = get_text(&mut clipboard)?;
    let mut stdout = io::stdout();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| Error::operation("failed to write stdout", error))
}

#[cfg(target_os = "linux")]
fn get_text(clipboard: &mut Clipboard) -> Result<String, Error> {
    clipboard
        .get()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text()
        .map_err(|error| Error::operation("failed to read clipboard", error))
}

#[cfg(not(target_os = "linux"))]
fn get_text(clipboard: &mut Clipboard) -> Result<String, Error> {
    clipboard
        .get_text()
        .map_err(|error| Error::operation("failed to read clipboard", error))
}

#[cfg(test)]
mod tests {
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    use super::*;

    #[test]
    fn parses_text() {
        let Content::Text(text) = parse_content("hello 世界".as_bytes()).unwrap() else {
            panic!("expected text");
        };
        assert_eq!(text, "hello 世界");
    }

    #[test]
    fn parses_png_as_rgba_image() {
        let pixels = [255, 0, 0, 128];
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&pixels, 1, 1, ExtendedColorType::Rgba8)
            .unwrap();

        let Content::Image(image) = parse_content(&png).unwrap() else {
            panic!("expected image");
        };
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.bytes.as_ref(), pixels);
    }

    #[test]
    fn reports_invalid_input() {
        use std::error::Error as _;

        let png_error = parse_content(b"\x89PNG\r\n\x1a\ninvalid").err().unwrap();
        assert!(png_error.to_string().contains("decode PNG"));
        assert!(png_error.source().unwrap().is::<image::ImageError>());

        let text_error = parse_content(b"\xff").err().unwrap();
        assert!(matches!(text_error, Error::InvalidText(_)));
        assert!(text_error.to_string().contains("neither UTF-8"));
        assert!(text_error.source().unwrap().is::<std::str::Utf8Error>());
    }
}
