//! Initialization orchestration (instruction 0010): loads existing state and
//! combines every module into one plan, so the TUI can preview and apply it
//! in a single explicit step. No terminal code here.

use crate::apply::{self, ApplyError, ApplyReport, Desired, Plan};
use crate::contract::{
    CONFIG_FILE, ContractFile, HarnessConfig, HarnessLock, HarnessState, LOCK_FILE,
};
use crate::gitignore::{self, GitReport};
use crate::guidance;
use crate::paths::{HiddenAttr, ProjectRoot, RelPath, apply_hidden_attribute};
use crate::placement;
use crate::sources::{ImportRequest, Package, import_items, lock_with};

/// Existing files in the project. `config` is `None` before the first init.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: Option<HarnessConfig>,
    pub lock: HarnessLock,
    pub state: HarnessState,
}

/// Fails on unreadable or unsupported files instead of ignoring them.
pub fn load(root: &ProjectRoot) -> Result<Loaded, String> {
    let p = root.path();
    Ok(Loaded {
        config: HarnessConfig::load(p).map_err(|e| e.to_string())?,
        lock: HarnessLock::load(p)
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        state: HarnessState::load(p)
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
    })
}

/// A skill fetched in this session, to import on apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImport {
    pub skill_id: String,
    pub source_id: String,
    pub reference: String,
    pub page_type: Option<String>,
    pub accept_partial: bool,
    pub package: Package,
}

#[derive(Debug)]
pub struct Prepared {
    pub config: HarnessConfig,
    pub lock: HarnessLock,
    pub plan: Plan,
    pub git: GitReport,
    pub warnings: Vec<String>,
}

/// Builds the full plan: imports, instruction dirs, guidance blocks, skill
/// copies, `.gitignore` block, `harness.toml` and `harness.lock.json`.
/// Reads only; nothing is written.
pub fn prepare(
    root: &ProjectRoot,
    loaded: &Loaded,
    config: &HarnessConfig,
    imports: &[PendingImport],
) -> Result<Prepared, String> {
    config.validate().map_err(|e| e.to_string())?;
    config.layout().validate(root).map_err(|errs| {
        errs.iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;

    let io = |e: std::io::Error| e.to_string();
    let mut lock = loaded.lock.clone();
    let mut imported = Vec::new();
    for i in imports {
        let req = ImportRequest {
            skill_id: &i.skill_id,
            source_id: &i.source_id,
            reference: &i.reference,
            page_type: i.page_type.as_deref(),
            accept_partial: i.accept_partial,
        };
        let (desired, locked) = import_items(root, &lock, &req, &i.package)
            .map_err(|e| format!("{}: {e}", i.skill_id))?;
        lock = lock_with(&lock, locked);
        imported.extend(desired);
    }

    let (mut dirs, mut desired) = guidance::plan_items(root, config).map_err(io)?;
    dirs.push(config.paths.references.clone());
    dirs.push(config.paths.output.clone());
    desired.extend(imported.iter().cloned());

    let placed =
        placement::plan_placement(root, config, &lock, &loaded.state, &imported).map_err(io)?;
    desired.extend(placed.desired);
    let copies = placement::ignore_paths(config, &lock);
    desired.push(gitignore::desired(root, config, &copies).map_err(io)?);
    desired.push(Desired::file(
        RelPath::parse(CONFIG_FILE).expect("valid"),
        config.to_text().map_err(|e| e.to_string())?,
    ));
    desired.push(Desired::file(
        RelPath::parse(LOCK_FILE).expect("valid"),
        lock.to_text().map_err(|e| e.to_string())?,
    ));

    let mut plan = apply::plan(root, &loaded.state, &dirs, desired).map_err(io)?;
    plan.deployments = placed.deployments;
    plan.adapters = placed.adapters;
    let git = gitignore::git_report(root, config, &copies);
    Ok(Prepared {
        config: config.clone(),
        lock,
        plan,
        git,
        warnings: placed.warnings,
    })
}

/// Applies a prepared plan, then tries to hide the output directory.
pub fn apply_prepared(
    root: &ProjectRoot,
    prepared: &Prepared,
    state: &mut HarnessState,
) -> Result<(ApplyReport, HiddenAttr), ApplyError> {
    let report = apply::apply(root, &prepared.plan, state)?;
    let hidden = if report.failed.is_none() {
        apply_hidden_attribute(&root.abs(&prepared.config.paths.output))
    } else {
        HiddenAttr::NotApplicable
    };
    Ok((report, hidden))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::Action;
    use crate::contract::{AgentConfig, AgentId, Placement, SkillConfig, SourceConfig, SourceKind};
    use crate::sources::{LocalDir, SkillSource};
    use std::fs;
    use std::path::PathBuf;

    struct TempProject(PathBuf);

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn project(tag: &str) -> TempProject {
        let dir = std::env::temp_dir().join(format!("aht-init-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("vendor/review/scripts")).unwrap();
        fs::write(
            dir.join("vendor/review/SKILL.md"),
            "---\nname: review\ndescription: r\n---\n",
        )
        .unwrap();
        fs::write(dir.join("vendor/review/scripts/x.sh"), "echo\n").unwrap();
        TempProject(dir)
    }

    fn config() -> HarnessConfig {
        HarnessConfig {
            agents: vec![AgentConfig {
                id: AgentId::ClaudeCode,
                placement: Placement::Copy,
            }],
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
        }
    }

    #[test]
    fn full_init_then_verify_is_unchanged() {
        let p = project("full");
        let root = ProjectRoot::new(&p.0).unwrap();
        let package = LocalDir {
            dir: p.0.join("vendor"),
        }
        .fetch("review", None)
        .unwrap();
        let imports = vec![PendingImport {
            skill_id: "review".into(),
            source_id: "local".into(),
            reference: "review".into(),
            page_type: None,
            accept_partial: false,
            package,
        }];
        let mut loaded = load(&root).unwrap();
        assert!(loaded.config.is_none());
        let prepared = prepare(&root, &loaded, &config(), &imports).unwrap();
        assert_eq!(prepared.plan.conflicts().count(), 0);
        let (report, _) = apply_prepared(&root, &prepared, &mut loaded.state).unwrap();
        assert_eq!(report.failed, None);
        for f in [
            "harness.toml",
            "harness.lock.json",
            ".harness/state.json",
            ".gitignore",
            "CLAUDE.md",
            "CLAUDE_ko.md",
            "instructions/completed.log",
            ".skills/review/scripts/x.sh",
            ".claude/skills/review/SKILL.md",
        ] {
            assert!(p.0.join(f).is_file(), "{f}");
        }
        for d in ["instructions/.completed", "references", ".output"] {
            assert!(p.0.join(d).is_dir(), "{d}");
        }

        // Re-run from disk: same config, no imports → nothing to do.
        let again = load(&root).unwrap();
        let config = again.config.clone().unwrap();
        let verify = prepare(&root, &again, &config, &[]).unwrap();
        assert!(!verify.plan.has_writes(), "{}", verify.plan);
        assert!(
            verify
                .plan
                .items
                .iter()
                .all(|i| matches!(i.action, Action::Unchanged | Action::Preserve))
        );
    }

    #[test]
    fn invalid_layout_is_reported_before_planning() {
        let p = project("invalid");
        let root = ProjectRoot::new(&p.0).unwrap();
        let mut c = HarnessConfig::default();
        c.paths.output = RelPath::parse("out").unwrap();
        let err = prepare(&root, &load(&root).unwrap(), &c, &[]).unwrap_err();
        assert!(err.contains("dot-prefixed"), "{err}");
    }

    #[test]
    fn unsupported_state_version_blocks_loading() {
        let p = project("version");
        fs::create_dir_all(p.0.join(".harness")).unwrap();
        fs::write(p.0.join(".harness/state.json"), r#"{"schema_version": 7}"#).unwrap();
        let err = load(&ProjectRoot::new(&p.0).unwrap()).unwrap_err();
        assert!(err.contains("schema_version 7"), "{err}");
    }
}
