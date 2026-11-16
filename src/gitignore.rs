//! `.gitignore` managed block and read-only Git checks (Stage 1 spec §6,
//! instruction 0009).
//!
//! The block lists only harness paths; other rules in `.gitignore` are kept.
//! Git is optional and only queried (`check-ignore`, `ls-files`): this module
//! never changes the index, never commits and never runs `git init`.

use crate::apply::Desired;
use crate::blocks::{self, Syntax};
use crate::contract::{CONFIG_FILE, HarnessConfig, LOCK_FILE};
use crate::guidance::guidance_files;
use crate::paths::{Layout, ProjectRoot, RelPath};
use std::io;
use std::path::Path;
use std::process::Command;

pub const BLOCK_ID: &str = "agent-harness";

/// Lines of the managed block. Patterns are anchored to the project root.
pub fn ignore_body(config: &HarnessConfig, copies: &[RelPath]) -> String {
    let db = &config.backlog.path;
    let mut lines = vec![
        format!("/{}/", config.paths.output),
        "/.harness/".to_string(),
        // Reserved even while the backlog is disabled.
        format!("/{db}"),
        format!("/{db}-wal"),
        format!("/{db}-shm"),
        format!("/{db}-journal"),
    ];
    lines.extend(copies.iter().map(|c| format!("/{c}/")));
    lines.join("\n") + "\n"
}

pub fn gitignore_path() -> RelPath {
    RelPath::parse(".gitignore").expect("valid constant")
}

/// Apply-engine item for the `.gitignore` block.
pub fn desired(
    root: &ProjectRoot,
    config: &HarnessConfig,
    copies: &[RelPath],
) -> io::Result<Desired> {
    blocks::desired(
        root,
        gitignore_path(),
        Syntax::Hash,
        BLOCK_ID,
        &ignore_body(config, copies),
    )
}

/// Paths the harness expects Git to track.
pub fn shared_paths(config: &HarnessConfig) -> Vec<RelPath> {
    let mut v = vec![
        Layout::instructions(),
        Layout::completed(),
        Layout::completed_log(),
        config.paths.references.clone(),
        Layout::skills(),
        RelPath::parse(CONFIG_FILE).expect("valid"),
        RelPath::parse(LOCK_FILE).expect("valid"),
    ];
    for a in &config.agents {
        for (_, p) in guidance_files(a.id) {
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }
    v
}

/// What Git says about the planned layout. Informational only.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitReport {
    /// `Some(reason)` when Git checks were skipped.
    pub unavailable: Option<String>,
    /// Shared paths that existing ignore rules (this repo, a parent
    /// directory or the global excludes file) already exclude: "path: source".
    pub excluded_shared: Vec<String>,
    /// Paths to be ignored that Git already tracks.
    pub tracked_ignored: Vec<String>,
    pub notes: Vec<String>,
}

pub fn git_report(root: &ProjectRoot, config: &HarnessConfig, copies: &[RelPath]) -> GitReport {
    report_with("git", root, config, copies)
}

fn report_with(
    git: &str,
    root: &ProjectRoot,
    config: &HarnessConfig,
    copies: &[RelPath],
) -> GitReport {
    let mut r = GitReport::default();
    let run = |args: &[&str]| {
        Command::new(git)
            .arg("-C")
            .arg(root.path())
            .args(args)
            .output()
    };
    match run(&["rev-parse", "--is-inside-work-tree"]) {
        Err(e) => {
            r.unavailable = Some(format!("Git is not available ({e}); Git checks skipped"));
            return r;
        }
        Ok(out) if !out.status.success() => {
            r.unavailable = Some(
                "not inside a Git work tree; Git checks skipped (Git is never initialized automatically)"
                    .into(),
            );
            return r;
        }
        Ok(_) => {}
    }

    // Existing rules that would hide shared paths. `--no-index` also
    // checks paths that are already tracked.
    let shared: Vec<String> = shared_paths(config).iter().map(|p| p.to_string()).collect();
    let mut args = vec!["check-ignore", "--no-index", "-v", "--"];
    args.extend(shared.iter().map(String::as_str));
    if let Ok(out) = run(&args) {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // `<source>:<line>:<pattern>\t<path>`
            if let Some((source, path)) = line.split_once('\t') {
                r.excluded_shared
                    .push(format!("{path}: excluded by {source}"));
            }
        }
    }

    let mut ignored: Vec<String> = vec![
        format!("{}", config.paths.output),
        ".harness".into(),
        config.backlog.path.to_string(),
    ];
    ignored.extend(copies.iter().map(|c| c.to_string()));
    let mut args = vec!["ls-files", "--"];
    args.extend(ignored.iter().map(String::as_str));
    if let Ok(out) = run(&args) {
        for path in String::from_utf8_lossy(&out.stdout).lines() {
            r.tracked_ignored.push(path.to_string());
        }
    }
    if !r.tracked_ignored.is_empty() {
        r.notes.push(
            "`.gitignore` does not stop tracking files Git already tracks; \
             run `git rm --cached <path>` yourself if they should leave the index"
                .into(),
        );
    }

    for dir in [Layout::completed(), config.paths.references.clone()] {
        if !has_file(&root.abs(&dir)) {
            r.notes.push(format!(
                "`{dir}/` has no files; Git does not share empty directories"
            ));
        }
    }
    r
}

fn has_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        if p.is_dir() { has_file(&p) } else { true }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply, plan};
    use crate::contract::{AgentConfig, AgentId, HarnessState, Placement};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-gitignore-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn write(&self, rel: &str, text: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        fn git(&self, args: &[&str]) {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&self.0)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        }
        fn root(&self) -> ProjectRoot {
            ProjectRoot::new(&self.0).unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn config() -> HarnessConfig {
        HarnessConfig {
            agents: vec![AgentConfig {
                id: AgentId::ClaudeCode,
                placement: Placement::Copy,
            }],
            ..HarnessConfig::default()
        }
    }

    fn copies() -> Vec<RelPath> {
        vec![RelPath::parse(".claude/skills/review").unwrap()]
    }

    #[test]
    fn body_lists_only_harness_paths() {
        let mut c = config();
        c.paths.output = RelPath::parse("build/.out").unwrap();
        assert_eq!(
            ignore_body(&c, &copies()),
            "/build/.out/\n/.harness/\n/agent.db\n/agent.db-wal\n/agent.db-shm\n/agent.db-journal\n/.claude/skills/review/\n"
        );
    }

    #[test]
    fn block_keeps_user_rules_and_is_idempotent() {
        let p = TempDir::new();
        p.write(".gitignore", "target/\n*.log\n");
        let root = p.root();
        let mut state = HarnessState::default();
        let pl = plan(
            &root,
            &state,
            &[],
            vec![desired(&root, &config(), &copies()).unwrap()],
        )
        .unwrap();
        apply(&root, &pl, &mut state).unwrap();
        let text = fs::read_to_string(p.0.join(".gitignore")).unwrap();
        assert!(
            text.starts_with("target/\n*.log\n\n# agent-harness:begin agent-harness\n/.output/\n")
        );
        let pl = plan(
            &root,
            &state,
            &[],
            vec![desired(&root, &config(), &copies()).unwrap()],
        )
        .unwrap();
        assert_eq!(apply(&root, &pl, &mut state).unwrap().apply_id, None);
        assert_eq!(
            fs::read_to_string(p.0.join(".gitignore"))
                .unwrap()
                .matches("agent-harness:begin")
                .count(),
            1
        );
    }

    #[test]
    fn generated_rules_ignore_what_they_should_in_real_git() {
        let p = TempDir::new();
        p.git(&["init", "-q"]);
        p.write(
            ".gitignore",
            &format!(
                "# agent-harness:begin agent-harness\n{}# agent-harness:end agent-harness\n",
                ignore_body(&config(), &copies())
            ),
        );
        let r = report_with("git", &p.root(), &config(), &copies());
        assert_eq!(r.unavailable, None);
        assert!(r.excluded_shared.is_empty(), "{:?}", r.excluded_shared);
        // Spot-check the rules with Git itself.
        for (path, ignored) in [
            (".output/x", true),
            (".harness/state.json", true),
            ("agent.db-wal", true),
            (".claude/skills/review/SKILL.md", true),
            (".claude/skills/other/SKILL.md", false),
            ("docs/agent.db", false),
            ("instructions/completed.log", false),
        ] {
            let status = Command::new("git")
                .arg("-C")
                .arg(&p.0)
                .args(["check-ignore", "-q", "--no-index", path])
                .status()
                .unwrap();
            assert_eq!(status.success(), ignored, "{path}");
        }
    }

    #[test]
    fn reports_parent_rules_tracked_files_and_empty_dirs() {
        let repo = TempDir::new();
        repo.git(&["init", "-q"]);
        // A parent directory rule hides the project's references.
        repo.write(".gitignore", "references/\n");
        repo.write("proj/.harness/state.json", "{}");
        repo.git(&["add", "-f", "proj/.harness/state.json"]);
        repo.write("proj/references/a.md", "x");
        let root = ProjectRoot::new(repo.0.join("proj")).unwrap();
        let r = report_with("git", &root, &config(), &copies());
        assert!(
            r.excluded_shared
                .iter()
                .any(|e| e.starts_with("references: excluded by")),
            "{:?}",
            r.excluded_shared
        );
        assert_eq!(r.tracked_ignored, [".harness/state.json"]);
        assert!(r.notes.iter().any(|n| n.contains("git rm --cached")));
        assert!(
            r.notes
                .iter()
                .any(|n| n.contains("instructions/.completed/"))
        );
        assert!(
            !r.notes.iter().any(|n| n.contains("`references/`")),
            "has a file"
        );
        // Read-only: still tracked, nothing committed.
        let staged = Command::new("git")
            .arg("-C")
            .arg(&repo.0)
            .args(["ls-files"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&staged.stdout),
            "proj/.harness/state.json\n"
        );
    }

    #[test]
    fn skips_without_git_or_repository() {
        let p = TempDir::new();
        let r = report_with("git-binary-that-does-not-exist", &p.root(), &config(), &[]);
        assert!(r.unavailable.unwrap().contains("not available"));
        let outside = TempDir::new(); // temp dirs are not inside a repository
        let r = report_with("git", &outside.root(), &config(), &[]);
        if let Some(why) = r.unavailable {
            assert!(why.contains("never initialized"));
        }
        assert!(!outside.0.join(".git").exists(), "no git init");
    }
}
