// Binary-level checks that need no terminal.
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agent-harness-tui"))
}

#[test]
fn version_flag() {
    let out = bin().arg("--version").output().expect("run binary");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("agent-harness-tui "));
}

#[test]
fn fails_fast_without_a_terminal() {
    let dir = std::env::temp_dir().join(format!("aht-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = bin()
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("interactive terminal"));
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "nothing written"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unknown_argument_is_an_error() {
    let out = bin().arg("--bogus").output().expect("run binary");
    assert_eq!(out.status.code(), Some(2));
}
