use base64::{Engine, engine::general_purpose::STANDARD};
use nix::{
    errno::Errno,
    sys::termios::{SetArg, SpecialCharacterIndices, Termios, cfmakeraw, tcgetattr, tcsetattr},
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    time::{Duration, Instant},
};

use crate::Error;

const PREFIX: &[u8] = b"\x1b]52;c;";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_ENCODED_RESPONSE: usize = 16 * 1024 * 1024;

pub(crate) fn in_ssh_session() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

pub(crate) fn copy(input: &[u8]) -> Result<(), Error> {
    std::str::from_utf8(input)
        .map_err(|error| Error::operation("OSC 52 copy requires UTF-8 text", error))?;
    let mut tty = open_terminal()?;
    send(&mut tty, &STANDARD.encode(input))
}

pub(crate) fn paste() -> Result<String, Error> {
    let mut tty = open_terminal()?;
    let original = tcgetattr(&tty)
        .map_err(|error| Error::operation("failed to read terminal settings", error))?;
    let mut raw = original.clone();

    // OSC replies arrive without a newline and must not be echoed.
    cfmakeraw(&mut raw);
    raw.control_chars[SpecialCharacterIndices::VMIN as usize] = 0;
    raw.control_chars[SpecialCharacterIndices::VTIME as usize] = 1;
    set_terminal(&tty, &raw)?;

    let response = send(&mut tty, "?").and_then(|()| read_response(&mut tty));
    set_terminal(&tty, &original)?;

    let bytes = STANDARD
        .decode(response?)
        .map_err(|error| Error::operation("invalid OSC 52 base64 response", error))?;
    String::from_utf8(bytes)
        .map_err(|error| Error::operation("OSC 52 clipboard does not contain UTF-8 text", error))
}

fn open_terminal() -> Result<File, Error> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|error| Error::operation("failed to open controlling terminal for OSC 52", error))
}

fn send(terminal: &mut File, payload: &str) -> Result<(), Error> {
    terminal
        .write_all(PREFIX)
        .and_then(|()| terminal.write_all(payload.as_bytes()))
        .and_then(|()| terminal.write_all(b"\x07"))
        .map_err(|error| Error::operation("failed to send OSC 52 to terminal", error))
}

fn set_terminal(tty: &File, settings: &Termios) -> Result<(), Error> {
    loop {
        match tcsetattr(tty, SetArg::TCSANOW, settings) {
            Ok(()) => return Ok(()),
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(Error::operation("failed to set terminal settings", error)),
        }
    }
}

fn read_response(terminal: &mut File) -> Result<Vec<u8>, Error> {
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    let mut matched = 0;
    let mut payload = Vec::new();
    let mut escaped = false;
    let mut buffer = [0; 4096];

    while Instant::now() < deadline {
        let count = match terminal.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(Error::operation("failed to read OSC 52 response", error)),
        };

        for &byte in &buffer[..count] {
            if byte == b'\x03' {
                return Err("OSC 52 paste cancelled by Ctrl-C".into());
            }

            if matched < PREFIX.len() {
                matched = if byte == PREFIX[matched] {
                    matched + 1
                } else {
                    usize::from(byte == PREFIX[0])
                };
                continue;
            }

            match byte {
                b'\\' if escaped => return Ok(payload),
                _ if escaped => return Err("invalid OSC 52 response terminator".into()),
                b'\x07' => return Ok(payload),
                b'\x1b' => escaped = true,
                _ if payload.len() == MAX_ENCODED_RESPONSE => {
                    return Err("OSC 52 response exceeds the size limit".into());
                }
                _ => payload.push(byte),
            }
        }
    }

    Err("timed out waiting for OSC 52 response; check terminal clipboard read support and permissions".into())
}
