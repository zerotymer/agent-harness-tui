//! Public skill install through the skills CLI (Stage 2 spec §3).
//!
//! The TUI only decides *what* to install: `npx skills add <source>
//! [-s <skill>]`. Agents, scope and skill selection are asked by the CLI
//! itself, so no `-a`, `-g` or `-y` is added. The command runs without a
//! shell, only after the user confirms, with the terminal handed over.

use crate::catalog::{SKILLS_CLI, SkillsEntry};
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The package runner; `npx.cmd` on Windows.
pub const NPX: &str = if cfg!(windows) { "npx.cmd" } else { "npx" };

/// A confirmed install, waiting for the terminal to be handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl RunRequest {
    /// The command line as shown to the user.
    pub fn display(&self) -> String {
        let name = self
            .program
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::iter::once(name)
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Arguments after the program, or `None` for installers the TUI never runs.
pub fn args(entry: &SkillsEntry) -> Option<Vec<String>> {
    if entry.installer != SKILLS_CLI {
        return None;
    }
    let mut args = vec!["skills".into(), "add".into(), entry.source.clone()];
    if let Some(skill) = entry.skill.as_deref().filter(|s| *s != "*") {
        args.extend(["-s".into(), skill.into()]);
    }
    Some(args)
}

/// Result of the name conflict check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    None,
    /// The named skill is already deployed or selected by the harness.
    With(String),
    /// Installing every skill: names are not known in advance.
    Unknown,
}

/// Compares the entry's `skill` with the harness skill names, ignoring case.
pub fn conflict<'a>(entry: &SkillsEntry, harness: impl IntoIterator<Item = &'a str>) -> Conflict {
    match entry.skill.as_deref().filter(|s| *s != "*") {
        None => Conflict::Unknown,
        Some(name) => harness
            .into_iter()
            .find(|h| h.eq_ignore_ascii_case(name))
            .map_or(Conflict::None, |h| Conflict::With(h.to_string())),
    }
}

/// Looks up `name` in the directories of a `PATH`-style value.
pub fn find_program(name: &str, path: Option<OsString>) -> Option<PathBuf> {
    std::env::split_paths(&path?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Runs the command with inherited stdio in `cwd`, then waits for Enter so
/// the user can read the CLI's output before the TUI redraws. Returns the
/// exit code (`None` when the process was ended by a signal).
pub fn run_interactive(req: &RunRequest, cwd: &Path) -> Result<Option<i32>, String> {
    println!("\n$ {}\n", req.display());
    let status = run(req, cwd);
    match &status {
        Ok(Some(code)) => println!("\nskills CLI 종료 코드: {code}"),
        Ok(None) => println!("\nskills CLI가 신호로 종료되었습니다."),
        Err(e) => println!("\n실행하지 못했습니다: {e}"),
    }
    print!("Enter를 누르면 TUI로 돌아갑니다. ");
    let _ = io::stdout().flush();
    let _ = io::stdin().lock().read_line(&mut String::new());
    status
}

/// Runs the command without a shell, inheriting stdio.
pub fn run(req: &RunRequest, cwd: &Path) -> Result<Option<i32>, String> {
    Command::new(&req.program)
        .args(&req.args)
        .current_dir(cwd)
        .status()
        .map(|s| s.code())
        .map_err(|e| format!("{}: {e}", req.program.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Text, parse};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-install-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entry(skill: Option<&str>) -> SkillsEntry {
        SkillsEntry {
            id: "x".into(),
            title: Text {
                en: Some("x".into()),
                ko: None,
            },
            summary: None,
            version: None,
            updated: None,
            installer: SKILLS_CLI.into(),
            source: "owner/repo@v1.2.3".into(),
            skill: skill.map(str::to_string),
            install: None,
        }
    }

    #[test]
    fn argument_vectors_never_add_agent_scope_or_yes() {
        assert_eq!(
            args(&entry(Some("pdf"))).unwrap(),
            ["skills", "add", "owner/repo@v1.2.3", "-s", "pdf"]
        );
        for all in [None, Some("*")] {
            assert_eq!(
                args(&entry(all)).unwrap(),
                ["skills", "add", "owner/repo@v1.2.3"]
            );
        }
        for skill in [None, Some("pdf"), Some("*")] {
            let a = args(&entry(skill)).unwrap();
            assert!(
                !a.iter()
                    .any(|x| ["-a", "-g", "-y", "--yes", "--all"].contains(&x.as_str()))
            );
        }
        let mut other = entry(None);
        other.installer = "claude-code-cli".into();
        assert_eq!(args(&other), None, "other installers are never run");
        let req = RunRequest {
            program: PathBuf::from("/usr/bin").join(NPX),
            args: args(&entry(Some("pdf"))).unwrap(),
        };
        assert_eq!(
            req.display(),
            format!("{NPX} skills add owner/repo@v1.2.3 -s pdf")
        );
    }

    #[test]
    fn option_like_values_are_rejected_by_the_catalog() {
        for (source, skill) in [("-x", "pdf"), ("o/r", "--global")] {
            let text = format!(
                r#"{{"schema_version":1,"kind":"skills","entries":[{{"id":"x","installer":"skills-cli","source":"{source}","skill":"{skill}","title":{{"en":"x"}}}}]}}"#
            );
            assert!(parse::<SkillsEntry>(&text).is_err(), "{source} {skill}");
        }
    }

    #[test]
    fn conflicts_by_skill_name() {
        let harness = ["Review", "pdf-tools"];
        assert_eq!(
            conflict(&entry(Some("review")), harness),
            Conflict::With("Review".into())
        );
        assert_eq!(conflict(&entry(Some("pdf")), harness), Conflict::None);
        assert_eq!(conflict(&entry(Some("*")), harness), Conflict::Unknown);
        assert_eq!(conflict(&entry(None), harness), Conflict::Unknown);
    }

    #[test]
    fn finds_programs_on_the_path_only() {
        let d = TempDir::new();
        fs::write(d.0.join(NPX), "").unwrap();
        let path = std::env::join_paths([PathBuf::from("/nonexistent-dir"), d.0.clone()]).unwrap();
        assert_eq!(find_program(NPX, Some(path.clone())), Some(d.0.join(NPX)));
        assert_eq!(find_program("no-such-runner", Some(path)), None);
        assert_eq!(find_program(NPX, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn stub_runner_reports_exit_codes_and_gets_plain_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let d = TempDir::new();
        let stub = d.0.join("npx");
        // Records its arguments one per line, then exits with $STUB_CODE.
        fs::write(
            &stub,
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > args.txt\nexit $(cat code.txt)\n",
        )
        .unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        let mut e = entry(Some("pdf"));
        e.source = "owner/repo;touch_PWNED".into(); // no shell: stays one argument
        let req = RunRequest {
            program: stub,
            args: args(&e).unwrap(),
        };
        for code in [0, 3] {
            fs::write(d.0.join("code.txt"), code.to_string()).unwrap();
            assert_eq!(run(&req, &d.0), Ok(Some(code)));
        }
        assert_eq!(
            fs::read_to_string(d.0.join("args.txt")).unwrap(),
            "skills\nadd\nowner/repo;touch_PWNED\n-s\npdf\n"
        );
        assert!(!d.0.join("touch_PWNED").exists());
        let missing = RunRequest {
            program: d.0.join("missing"),
            args: vec![],
        };
        assert!(run(&missing, &d.0).is_err());
    }
}
