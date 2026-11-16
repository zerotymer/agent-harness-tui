// Smoke test: the binary builds and runs.
use std::process::Command;

#[test]
fn binary_runs() {
    let out = Command::new(env!("CARGO_BIN_EXE_agent-harness-tui"))
        .output()
        .expect("run binary");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("agent-harness-tui "));
}
