#![cfg(target_os = "linux")]

use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn copy_reports_clipboard_connection_failure() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cpy"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XDG_RUNTIME_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"hello").unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to open clipboard"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
