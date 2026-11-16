//! Stage 1 acceptance checks (spec §8, instruction 0011), run against
//! temporary target projects through the public API only. Local sources
//! only: no network and no administrator rights are needed.

use agent_harness_tui::apply::Action;
use agent_harness_tui::contract::{
    AgentConfig, AgentId, ContractFile, HarnessConfig, HarnessState, Placement, SkillConfig,
    SourceConfig, SourceKind,
};
use agent_harness_tui::init::{self, PendingImport};
use agent_harness_tui::paths::{PathError, ProjectRoot, RelPath};
use agent_harness_tui::sources::{LocalDir, SkillSource};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aht-stage1-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = Self(dir);
        p.write(
            "vendor/review/SKILL.md",
            "---\nname: review\ndescription: Review\n---\n# Review\n",
        );
        p.write("vendor/review/scripts/run.sh", "#!/bin/sh\ntouch PWNED\n");
        p
    }
    fn root(&self) -> ProjectRoot {
        ProjectRoot::new(&self.0).unwrap()
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> HarnessConfig {
    let mut c = HarnessConfig {
        agents: vec![
            AgentConfig {
                id: AgentId::ClaudeCode,
                placement: Placement::Copy,
            },
            AgentConfig {
                id: AgentId::Codex,
                placement: Placement::Reference,
            },
        ],
        sources: vec![SourceConfig {
            id: "local".into(),
            kind: SourceKind::LocalDir,
            path: Some(RelPath::parse("vendor").unwrap()),
            token_env: None,
        }],
        skills: vec![SkillConfig {
            id: "review".into(),
            source: "local".into(),
            reference: "review".into(),
            page_type: None,
        }],
        ..HarnessConfig::default()
    };
    // Spaces and Korean in configurable paths.
    c.paths.references = RelPath::parse("docs/참조 문서").unwrap();
    c.paths.output = RelPath::parse(".산출물").unwrap();
    c
}

fn imports(p: &Project) -> Vec<PendingImport> {
    let package = LocalDir {
        dir: p.0.join("vendor"),
    }
    .fetch("review", None)
    .unwrap();
    vec![PendingImport {
        skill_id: "review".into(),
        source_id: "local".into(),
        reference: "review".into(),
        page_type: None,
        accept_partial: false,
        package,
    }]
}

/// Prepares and applies; panics on conflicts or failure.
fn init_project(p: &Project, imports: &[PendingImport]) -> HarnessState {
    let root = p.root();
    let mut loaded = init::load(&root).unwrap();
    let config = loaded.config.clone().unwrap_or_else(config);
    let prepared = init::prepare(&root, &loaded, &config, imports).unwrap();
    assert_eq!(prepared.plan.conflicts().count(), 0, "{}", prepared.plan);
    let (report, _) = init::apply_prepared(&root, &prepared, &mut loaded.state).unwrap();
    assert_eq!(report.failed, None);
    loaded.state
}

/// No-change verification of an initialized project.
fn verify_unchanged(p: &Project) {
    let root = p.root();
    let loaded = init::load(&root).unwrap();
    let config = loaded.config.clone().expect("initialized");
    let prepared = init::prepare(&root, &loaded, &config, &[]).unwrap();
    assert!(!prepared.plan.has_writes(), "{}", prepared.plan);
    assert_eq!(prepared.plan.conflicts().count(), 0);
}

fn block(text: &str) -> String {
    let start = text.find("agent-harness:begin").unwrap();
    let end = text.find("agent-harness:end").unwrap();
    text[start..end].to_string()
}

fn code_spans(text: &str) -> BTreeSet<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn initializes_with_local_source_and_no_network() {
    let p = Project::new();
    init_project(&p, &imports(&p));
    for f in [
        "harness.toml",
        "harness.lock.json",
        ".harness/state.json",
        ".gitignore",
        "AGENTS.md",
        "AGENTS_ko.md",
        "CLAUDE.md",
        "CLAUDE_ko.md",
        "instructions/completed.log",
        ".skills/review/SKILL.md",
        ".skills/review/scripts/run.sh",
        ".claude/skills/review/SKILL.md",
    ] {
        assert!(p.0.join(f).is_file(), "{f}");
    }
    for d in ["instructions/.completed", "docs/참조 문서", ".산출물"] {
        assert!(p.0.join(d).is_dir(), "{d}");
    }
    // Bundled scripts are copied, never run; reference placement writes nothing.
    assert!(!p.0.join("PWNED").exists() && !p.0.join("vendor/review/PWNED").exists());
    assert!(!p.0.join(".agents").exists());
    // No SQLite file or schema is created for the backlog.
    assert!(!p.0.join("agent.db").exists());
}

#[test]
fn rerun_keeps_config_and_path_resolution() {
    let p = Project::new();
    init_project(&p, &imports(&p));
    let loaded = init::load(&p.root()).unwrap();
    assert_eq!(loaded.config, Some(config()));
    assert_eq!(
        p.root().abs(&loaded.config.unwrap().paths.references),
        p.root().path().join("docs").join("참조 문서")
    );
    let toml = p.read("harness.toml");
    assert!(
        toml.contains("references = \"docs/참조 문서\""),
        "portable relative path"
    );
    assert!(
        !toml.contains(p.0.to_str().unwrap()),
        "no absolute paths in shared config"
    );
    verify_unchanged(&p);
}

#[test]
fn existing_guidance_and_log_survive_and_repeats_add_nothing() {
    let p = Project::new();
    p.write("CLAUDE.md", "# Team rules\n\nKeep tests green.\n");
    p.write("AGENTS.md", "# Agents\r\nUse CRLF.\r\n");
    p.write(
        "instructions/completed.log",
        "u-1 | earlier work | 2026-01-01 09:00:00 +0900\n",
    );
    p.write(".gitignore", "target/\n");
    init_project(&p, &imports(&p));
    for _ in 0..2 {
        verify_unchanged(&p);
    }
    assert!(
        p.read("CLAUDE.md")
            .starts_with("# Team rules\n\nKeep tests green.\n\n")
    );
    assert!(
        p.read("AGENTS.md")
            .starts_with("# Agents\r\nUse CRLF.\r\n\r\n")
    );
    assert_eq!(
        p.read("instructions/completed.log"),
        "u-1 | earlier work | 2026-01-01 09:00:00 +0900\n"
    );
    assert!(p.read(".gitignore").starts_with("target/\n\n"));
    for f in [
        "CLAUDE.md",
        "CLAUDE_ko.md",
        "AGENTS.md",
        "AGENTS_ko.md",
        ".gitignore",
    ] {
        assert_eq!(p.read(f).matches("agent-harness:begin").count(), 1, "{f}");
    }
}

#[test]
fn english_and_korean_guidance_state_the_same_policy() {
    let p = Project::new();
    init_project(&p, &imports(&p));
    for (en, ko) in [("CLAUDE.md", "CLAUDE_ko.md"), ("AGENTS.md", "AGENTS_ko.md")] {
        let (en, ko) = (block(&p.read(en)), block(&p.read(ko)));
        assert_eq!(code_spans(&en), code_spans(&ko));
        for must in [
            "instructions/NNNN_short-name.md",
            "instructions/.completed/<UUID>.md",
            "instructions/completed.log",
            "UUID | summary | datetime with offset",
            "TODO",
            "IN_PROGRESS",
            "DONE",
        ] {
            assert!(code_spans(&en).contains(must), "{must}");
        }
    }
}

#[test]
fn rejects_path_escapes_and_collisions() {
    let p = Project::new();
    let root = p.root();
    assert!(matches!(
        root.resolve("../outside"),
        Err(PathError::OutsideRoot(_))
    ));
    assert!(HarnessConfig::from_text(
        "schema_version = 1\n[paths]\nreferences = \"/etc\"\noutput = \".o\"\n[backlog]\nenabled = false\npath = \"agent.db\"\n"
    )
    .is_err());
    p.write("references", "a file where a directory belongs");
    let err = init::prepare(
        &root,
        &init::load(&root).unwrap(),
        &HarnessConfig::default(),
        &[],
    )
    .unwrap_err();
    assert!(err.contains("collides"), "{err}");
}

#[cfg(unix)]
#[test]
fn rejects_package_paths_that_escape() {
    use agent_harness_tui::sources::SourceError;
    let p = Project::new();
    let outside = Project::new();
    std::os::unix::fs::symlink(
        outside.0.join("vendor/review/SKILL.md"),
        p.0.join("vendor/review/leak.md"),
    )
    .unwrap();
    let err = LocalDir {
        dir: p.0.join("vendor"),
    }
    .fetch("review", None)
    .unwrap_err();
    assert_eq!(err, SourceError::PathEscape("leak.md".into()));
}

#[test]
fn guidance_edits_inside_the_block_are_conflicts() {
    let p = Project::new();
    init_project(&p, &imports(&p));
    let text = p.read("CLAUDE.md").replace("### Git", "### Git (edited)");
    fs::write(p.0.join("CLAUDE.md"), text).unwrap();
    let root = p.root();
    let loaded = init::load(&root).unwrap();
    let mut changed = loaded.config.clone().unwrap();
    changed.backlog.enabled = true;
    let prepared = init::prepare(&root, &loaded, &changed, &[]).unwrap();
    let conflicts: Vec<_> = prepared
        .plan
        .conflicts()
        .map(|i| i.desired.path.to_string())
        .collect();
    assert_eq!(conflicts, ["CLAUDE.md"]);
}

#[test]
fn clone_without_local_state_does_not_overwrite() {
    let p = Project::new();
    init_project(&p, &imports(&p));
    // Simulate a fresh clone: tracked files present, ignored state missing.
    fs::remove_dir_all(p.0.join(".harness")).unwrap();
    fs::remove_dir_all(p.0.join(".claude")).unwrap();
    let text = p
        .read("AGENTS.md")
        .replace("### Git", "### Git (local edit)");
    fs::write(p.0.join("AGENTS.md"), &text).unwrap();
    let root = p.root();
    let loaded = init::load(&root).unwrap();
    let prepared = init::prepare(&root, &loaded, &loaded.config.clone().unwrap(), &[]).unwrap();
    let conflict = prepared
        .plan
        .items
        .iter()
        .find(|i| i.desired.path.as_str() == "AGENTS.md")
        .unwrap();
    assert!(matches!(&conflict.action, Action::Conflict(m) if m.contains("ownership")));
    assert_eq!(p.read("AGENTS.md"), text, "planning wrote nothing");
}

#[cfg(unix)]
#[test]
fn interrupted_apply_restores_files_and_records_no_success() {
    use std::os::unix::fs::PermissionsExt;
    let p = Project::new();
    p.write("CLAUDE.md", "# Original\n");
    fs::create_dir_all(p.0.join(".claude/skills")).unwrap();
    let root = p.root();
    let mut loaded = init::load(&root).unwrap();
    let prepared = init::prepare(&root, &loaded, &config(), &imports(&p)).unwrap();
    // A real filesystem failure part-way through: the copy target turns
    // read-only after planning.
    let locked = p.0.join(".claude/skills");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(locked.join("probe"), "").is_ok() {
        // Running with privileges that ignore permissions: cannot simulate.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let (report, _) = init::apply_prepared(&root, &prepared, &mut loaded.state).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(report.failed.is_some());
    assert!(report.residual.is_empty(), "{:?}", report.residual);
    assert!(!report.restored.is_empty());
    assert_eq!(p.read("CLAUDE.md"), "# Original\n", "restored from backup");
    assert!(!p.0.join("harness.toml").exists(), "new files removed");
    assert!(!p.0.join(".skills").exists(), "created dirs removed");
    assert!(
        HarnessState::load(&p.0).unwrap().is_none(),
        "no success recorded"
    );
    assert!(!p.0.join(".harness/apply.lock").exists());
}

fn git(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn git_tracks_shared_files_and_ignores_local_state() {
    let p = Project::new();
    if !git(&p.0, &["init", "-q"]).0 {
        return; // Git missing: the tool skips Git checks the same way.
    }
    init_project(&p, &imports(&p));
    // Empty directories are invisible to Git; give the output dir a file.
    p.write(".산출물/report.txt", "generated");
    let (_, status) = git(&p.0, &["status", "--porcelain", "--untracked-files=all"]);
    for tracked in [
        "harness.toml",
        "harness.lock.json",
        "CLAUDE.md",
        "AGENTS_ko.md",
        "instructions/completed.log",
        ".skills/review/scripts/run.sh",
        ".gitignore",
    ] {
        assert!(
            status.contains(tracked),
            "{tracked} should be visible to Git:\n{status}"
        );
    }
    for ignored in [".harness/", ".산출물", ".claude/skills/review"] {
        assert!(
            !status.contains(ignored),
            "{ignored} should be ignored:\n{status}"
        );
    }
    // The tool never staged or committed anything.
    assert_eq!(git(&p.0, &["diff", "--cached", "--name-only"]).1, "");
    assert!(!git(&p.0, &["rev-parse", "HEAD"]).0, "no commits");
}

#[test]
fn state_keeps_what_stage2_needs() {
    let p = Project::new();
    let state = init_project(&p, &imports(&p));
    assert!(state.last_success.is_some());
    assert!(
        state
            .managed
            .iter()
            .any(|m| m.block.as_deref() == Some("agent-harness"))
    );
    assert!(state.managed.iter().any(|m| m.block.is_none()));
    assert!(state.deployments.iter().all(|d| d.hash.is_some()));
    assert_eq!(state.adapters.len(), 2);
    let lock = agent_harness_tui::contract::HarnessLock::load(&p.0)
        .unwrap()
        .unwrap();
    assert!(
        lock.skills[0]
            .files
            .iter()
            .all(|f| f.hash.starts_with("sha256:"))
    );
}
