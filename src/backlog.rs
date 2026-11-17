//! Optional `agent.db` backlog (instruction 0017, issue #21).
//!
//! When enabled, the instruction root gets a fresh SQLite database plus
//! bash and PowerShell management scripts. All three are create-only: an
//! existing database or edited script is preserved. The schema text is the
//! single source for the database and both scripts.

use crate::apply::Desired;
use crate::contract::HarnessConfig;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const SCHEMA: &str = include_str!("../templates/agent-db.schema.sql");
const SH: &str = include_str!("../templates/agent-db.sh");
const PS1: &str = include_str!("../templates/agent-db.ps1");
pub const SH_FILE: &str = "agent-db.sh";
pub const PS1_FILE: &str = "agent-db.ps1";

/// Script text with the schema inlined.
pub fn script(template: &str) -> String {
    template.replace("@SCHEMA@", SCHEMA.trim_end())
}

/// Files to create when the backlog is enabled; nothing otherwise.
pub fn plan_items(config: &HarnessConfig) -> io::Result<Vec<Desired>> {
    if !config.backlog.enabled {
        return Ok(Vec::new());
    }
    let inst = &config.paths.instructions;
    Ok(vec![
        Desired::create_only(config.backlog.path.clone(), empty_db()?),
        Desired::create_only(inst.join(SH_FILE), script(SH)),
        // ponytail: LF endings; PowerShell 5.1+ reads them fine.
        Desired::create_only(inst.join(PS1_FILE), script(PS1)),
    ])
}

/// Bytes of a new database with the schema applied. Built in a temporary
/// file (WAL needs a real file) so nothing touches the project before apply.
fn empty_db() -> io::Result<Vec<u8>> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let path: PathBuf = std::env::temp_dir().join(format!(
        "agent-harness-db-{}-{}.sqlite",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let build = || -> rusqlite::Result<()> {
        let conn = rusqlite::Connection::open(&path)?;
        conn.execute_batch(SCHEMA)?;
        // Leaves the main file complete; WAL mode stays in its header.
        conn.close().map_err(|(_, e)| e)
    };
    let result = build()
        .map_err(io::Error::other)
        .and_then(|_| std::fs::read(&path));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::Ownership;

    fn enabled() -> HarnessConfig {
        let mut c = HarnessConfig::default();
        c.backlog.enabled = true;
        c
    }

    #[test]
    fn disabled_plans_nothing() {
        assert!(plan_items(&HarnessConfig::default()).unwrap().is_empty());
    }

    #[test]
    fn plans_db_and_scripts_in_instruction_root() {
        let items = plan_items(&enabled()).unwrap();
        let paths: Vec<_> = items.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "instructions/agent.db",
                "instructions/agent-db.sh",
                "instructions/agent-db.ps1"
            ]
        );
        assert!(items.iter().all(|d| d.ownership == Ownership::CreateOnly));
        for d in &items[1..] {
            let text = String::from_utf8(d.content.clone()).unwrap();
            assert!(!text.contains("@SCHEMA@"));
            assert!(text.contains("CREATE TABLE IF NOT EXISTS backlog"));
            assert!(text.contains("AGENT_DB_ACTOR"));
        }
    }

    #[test]
    fn empty_db_has_schema_and_wal() {
        let bytes = empty_db().unwrap();
        let dir = std::env::temp_dir().join(format!("agent-harness-dbt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("agent.db");
        std::fs::write(&file, &bytes).unwrap();
        let conn = rusqlite::Connection::open(&file).unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            names,
            [
                "backlog",
                "idx_backlog_status",
                "idx_instructions_status",
                "instructions"
            ]
        );
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn has(cmd: &str, arg: &str) -> bool {
        std::process::Command::new(cmd)
            .arg(arg)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Runs the generated script end to end in a temp instruction root.
    /// `run(args)` returns (success, stdout).
    fn exercise(tag: &str, run: impl Fn(&std::path::Path, &[&str]) -> (bool, String)) {
        let dir = std::env::temp_dir().join(format!("agent-harness-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for d in plan_items(&enabled()).unwrap() {
            std::fs::write(dir.join(d.path.file_name()), &d.content).unwrap();
        }
        let old = "0000-old | 기존 완료 | 2026-01-01 00:00:00 +0900";
        // No trailing newline: the script must not join lines.
        std::fs::write(dir.join("completed.log"), old).unwrap();
        let ok = |args: &[&str]| {
            let (success, out) = run(&dir, args);
            assert!(success, "{args:?}: {out}");
            out
        };

        ok(&["init"]);
        assert!(ok(&["instruction-show", "0000-old"]).contains("completed"));
        let id = ok(&["backlog-add", "할 일 '하나'", "설명"]);
        assert_eq!(id.trim(), "1");
        assert!(ok(&["backlog-list"]).contains("할 일 '하나'"));
        ok(&["backlog-complete", "1"]);
        assert!(!run(&dir, &["backlog-complete", "9"]).0, "unknown id fails");
        ok(&["instruction-upsert", "u-1", "새 지시서"]);
        ok(&["instruction-complete", "u-1", "요약"]);
        assert!(!run(&dir, &["instruction-complete", "u-1", "again"]).0);
        assert!(!run(&dir, &["instruction-complete", "u-2", "a|b"]).0);

        let log = std::fs::read_to_string(dir.join("completed.log")).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert_eq!(lines[0], old, "existing line untouched");
        assert!(lines[1].starts_with("u-1 | 요약 | "), "{log}");

        let conn = rusqlite::Connection::open(dir.join("agent.db")).unwrap();
        let row: (String, String, String) = conn
            .query_row(
                "SELECT title,status,created_by FROM instructions WHERE uuid='u-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            ("새 지시서".into(), "completed".into(), "tester".into())
        );
        let by: String = conn
            .query_row("SELECT updated_by FROM backlog WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(by, "tester");
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bash_script_works_end_to_end() {
        if cfg!(windows) || !has("bash", "--version") || !has("sqlite3", "--version") {
            eprintln!("skipped: bash or sqlite3 not available");
            return;
        }
        exercise("sh", |dir, args| {
            let out = std::process::Command::new("bash")
                .arg(dir.join(SH_FILE))
                .args(args)
                .env("AGENT_DB_ACTOR", "tester")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            (out.status.success(), text)
        });
    }

    #[test]
    fn powershell_script_works_end_to_end() {
        let shell = ["pwsh", "powershell"]
            .into_iter()
            .find(|s| has(s, "-NoProfile") && has(s, "-Help"));
        let shell = match shell {
            Some(s) if has("sqlite3", "--version") => s,
            _ => {
                eprintln!("skipped: PowerShell or sqlite3 not available");
                return;
            }
        };
        exercise("ps1", |dir, args| {
            let out = std::process::Command::new(shell)
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(dir.join(PS1_FILE))
                .args(args)
                .env("AGENT_DB_ACTOR", "tester")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            (out.status.success(), text)
        });
    }
}
