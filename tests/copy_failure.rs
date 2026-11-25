#![cfg(not(any(target_os = "windows", target_os = "macos")))]

use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn copy_reports_clipboard_connection_failure() {
    let output = copy(&[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to open clipboard"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn copy_passes_explicit_format_to_daemon() {
    let output = copy(&["--format", "jpg"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to decode JPEG from stdin"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy(args: &[&str]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cpy"))
        .args(args)
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_CLIENT")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XDG_RUNTIME_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"hello").unwrap();

    child.wait_with_output().unwrap()
}
