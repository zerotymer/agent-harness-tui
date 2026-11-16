//! Agent-specific skill placement (Stage 1 spec §5, instruction 0008).
//!
//! `.skills/` stays the shared source. Per agent, skills are either copied
//! into the directory the agent discovers, or only referenced from guidance.
//!
//! Official discovery rules, checked 2026-11-16 (adapter version 1):
//!
//! | Agent | Project skill directory | Source |
//! | --- | --- | --- |
//! | Claude Code | `.claude/skills/<name>/SKILL.md` | Claude Code CHANGELOG and skill docs (`name`/`description` frontmatter required) |
//! | Codex | `.agents/skills/<name>/SKILL.md` | learn.chatgpt.com "Build skills" / customization overview (`name`/`description` required) |
//!
//! Listing `.skills/` in guidance does not make either agent discover it, so
//! "reference" placement is guidance only. Symlink placement is not offered:
//! it needs extra privileges on Windows and copies work everywhere.

use crate::apply::{Desired, Ownership, sha256};
use crate::contract::{
    AdapterRecord, AgentId, Deployment, HarnessConfig, HarnessLock, HarnessState, Placement,
};
use crate::paths::{Layout, ProjectRoot, RelPath};
use std::io;

/// Bumped when an agent's discovery rules are re-checked and change.
pub const ADAPTER_VERSION: &str = "1";
pub const RULES_CHECKED: &str = "2026-11-16";

pub fn agent_name(agent: AgentId) -> &'static str {
    match agent {
        AgentId::ClaudeCode => "Claude Code",
        AgentId::Codex => "Codex",
    }
}

/// Project directory the agent scans for skills.
pub fn skills_dir(agent: AgentId) -> RelPath {
    RelPath::parse(match agent {
        AgentId::ClaudeCode => ".claude/skills",
        AgentId::Codex => ".agents/skills",
    })
    .expect("valid constant")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Copy,
    Reference,
    Symlink,
}

/// Placement methods with availability; unavailable ones carry a reason.
pub fn methods(_agent: AgentId) -> Vec<(Method, Result<(), &'static str>)> {
    vec![
        (Method::Copy, Ok(())),
        (Method::Reference, Ok(())),
        (
            Method::Symlink,
            Err("not offered: symlinks need extra privileges on Windows; use copy"),
        ),
    ]
}

#[derive(Debug, Default)]
pub struct PlacementPlan {
    pub desired: Vec<Desired>,
    pub deployments: Vec<Deployment>,
    pub adapters: Vec<AdapterRecord>,
    pub warnings: Vec<String>,
}

/// Plans copies for every agent with `copy` placement and every locked
/// skill. `pending` holds files planned in the same apply (e.g. a fresh
/// import) whose bytes are not on disk yet.
pub fn plan_placement(
    root: &ProjectRoot,
    config: &HarnessConfig,
    lock: &HarnessLock,
    state: &HarnessState,
    pending: &[Desired],
) -> io::Result<PlacementPlan> {
    let mut out = PlacementPlan::default();
    let mut seen = Vec::new();
    for a in &config.agents {
        if seen.contains(&a.id) {
            continue;
        }
        seen.push(a.id);
        out.adapters.push(AdapterRecord {
            agent: a.id,
            version: ADAPTER_VERSION.into(),
        });
        if a.placement != Placement::Copy {
            continue;
        }
        for skill in &lock.skills {
            let target_dir = skills_dir(a.id).join(&skill.id);
            let deployed = state
                .deployments
                .iter()
                .any(|d| d.agent == a.id && d.skill == skill.id);
            if !deployed && root.abs(&target_dir).exists() {
                out.desired.push(Desired {
                    path: target_dir.join("SKILL.md"),
                    content: Vec::new(),
                    ownership: Ownership::Unresolvable(format!(
                        "`{target_dir}/` already exists and was not deployed by this tool"
                    )),
                });
                continue;
            }
            for f in &skill.files {
                let source = Layout::skills().join(&skill.id).join(f.path.as_str());
                let target = target_dir.join(f.path.as_str());
                let bytes = match pending.iter().find(|d| d.path == source) {
                    Some(d) => d.content.clone(),
                    None => match std::fs::read(root.abs(&source)) {
                        Ok(b) => b,
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {
                            out.desired.push(Desired {
                                path: target,
                                content: Vec::new(),
                                ownership: Ownership::Unresolvable(format!(
                                    "source `{source}` is missing"
                                )),
                            });
                            continue;
                        }
                        Err(e) => return Err(e),
                    },
                };
                if f.path.as_str() == "SKILL.md" && !bytes.starts_with(b"---") {
                    out.warnings.push(format!(
                        "`{source}` has no frontmatter; {} needs `name` and `description` to discover it",
                        agent_name(a.id)
                    ));
                }
                out.deployments.push(Deployment {
                    skill: skill.id.clone(),
                    agent: a.id,
                    placement: Placement::Copy,
                    target: target.clone(),
                    hash: Some(sha256(&bytes)),
                });
                out.desired.push(Desired::file(target, bytes));
            }
        }
    }
    Ok(out)
}

/// Regenerable copies, ignored by Git by default (used by instruction 0009).
/// Per skill, so unrelated skills in the same agent directory stay tracked.
pub fn ignore_paths(config: &HarnessConfig, lock: &HarnessLock) -> Vec<RelPath> {
    let mut out = Vec::new();
    for a in config
        .agents
        .iter()
        .filter(|a| a.placement == Placement::Copy)
    {
        for s in &lock.skills {
            let p = skills_dir(a.id).join(&s.id);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{Action, apply, plan};
    use crate::contract::{AgentConfig, LockedFile, LockedSkill};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-placement-{}-{}",
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
        fn read(&self, rel: &str) -> String {
            fs::read_to_string(self.0.join(rel)).unwrap()
        }
        fn root(&self) -> ProjectRoot {
            ProjectRoot::new(&self.0).unwrap()
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const SKILL: &str = "---\nname: review\ndescription: Review code\n---\n# Review\n";

    fn rel(s: &str) -> RelPath {
        RelPath::parse(s).unwrap()
    }

    fn lock() -> HarnessLock {
        HarnessLock {
            schema_version: 1,
            skills: vec![LockedSkill {
                id: "review".into(),
                source: "local".into(),
                reference: "review".into(),
                page_type: None,
                files: vec![
                    LockedFile {
                        path: rel("SKILL.md"),
                        hash: sha256(SKILL.as_bytes()),
                        locale: None,
                        revision: None,
                    },
                    LockedFile {
                        path: rel("scripts/check.sh"),
                        hash: sha256(b"echo hi\n"),
                        locale: None,
                        revision: None,
                    },
                ],
            }],
        }
    }

    fn config(agents: &[(AgentId, Placement)]) -> HarnessConfig {
        HarnessConfig {
            agents: agents
                .iter()
                .map(|(id, placement)| AgentConfig {
                    id: *id,
                    placement: *placement,
                })
                .collect(),
            ..HarnessConfig::default()
        }
    }

    fn project() -> TempProject {
        let p = TempProject::new();
        p.write(".skills/review/SKILL.md", SKILL);
        p.write(".skills/review/scripts/check.sh", "echo hi\n");
        p
    }

    fn run(
        p: &TempProject,
        c: &HarnessConfig,
        state: &mut HarnessState,
    ) -> crate::apply::ApplyReport {
        let root = p.root();
        let pp = plan_placement(&root, c, &lock(), state, &[]).unwrap();
        let mut pl = plan(&root, state, &[], pp.desired).unwrap();
        pl.deployments = pp.deployments;
        pl.adapters = pp.adapters;
        apply(&root, &pl, state).unwrap()
    }

    #[test]
    fn copies_into_each_agents_discovery_dir_and_records_mapping() {
        let p = project();
        let c = config(&[
            (AgentId::ClaudeCode, Placement::Copy),
            (AgentId::Codex, Placement::Copy),
        ]);
        let mut state = HarnessState::default();
        let report = run(&p, &c, &mut state);
        assert_eq!(report.failed, None);
        assert_eq!(p.read(".claude/skills/review/SKILL.md"), SKILL);
        assert_eq!(
            p.read(".agents/skills/review/scripts/check.sh"),
            "echo hi\n"
        );
        assert_eq!(state.deployments.len(), 4);
        let d = state
            .deployments
            .iter()
            .find(|d| d.target.as_str() == ".claude/skills/review/SKILL.md")
            .unwrap();
        assert_eq!(d.hash.as_deref(), Some(sha256(SKILL.as_bytes()).as_str()));
        assert_eq!(state.adapters.len(), 2);
        // Persisted, and a second run changes nothing.
        let saved = <HarnessState as crate::contract::ContractFile>::load(&p.0).unwrap();
        assert_eq!(saved, Some(state.clone()));
        assert_eq!(run(&p, &c, &mut state).apply_id, None);
    }

    #[test]
    fn reference_placement_writes_no_files() {
        let p = project();
        let c = config(&[(AgentId::ClaudeCode, Placement::Reference)]);
        let pp = plan_placement(&p.root(), &c, &lock(), &HarnessState::default(), &[]).unwrap();
        assert!(pp.desired.is_empty() && pp.deployments.is_empty());
        assert_eq!(pp.adapters.len(), 1);
        assert!(ignore_paths(&c, &lock()).is_empty());
    }

    #[test]
    fn unrelated_existing_skills_are_not_overwritten() {
        let p = project();
        p.write(
            ".claude/skills/review/SKILL.md",
            "user's own review skill\n",
        );
        p.write(".claude/skills/other/SKILL.md", "unrelated\n");
        let c = config(&[(AgentId::ClaudeCode, Placement::Copy)]);
        let root = p.root();
        let state = HarnessState::default();
        let pp = plan_placement(&root, &c, &lock(), &state, &[]).unwrap();
        let pl = plan(&root, &state, &[], pp.desired).unwrap();
        assert!(matches!(&pl.items[0].action, Action::Conflict(m) if m.contains("not deployed")));
        assert_eq!(
            p.read(".claude/skills/review/SKILL.md"),
            "user's own review skill\n"
        );
        assert_eq!(p.read(".claude/skills/other/SKILL.md"), "unrelated\n");
    }

    #[test]
    fn edited_copy_is_a_conflict() {
        let p = project();
        let c = config(&[(AgentId::Codex, Placement::Copy)]);
        let mut state = HarnessState::default();
        run(&p, &c, &mut state);
        p.write(".agents/skills/review/SKILL.md", "edited copy\n");
        p.write(".skills/review/SKILL.md", "---\nname: review\n---\nv2\n");
        let root = p.root();
        let pp = plan_placement(&root, &c, &lock(), &state, &[]).unwrap();
        let pl = plan(&root, &state, &[], pp.desired).unwrap();
        assert!(matches!(pl.items[0].action, Action::Conflict(_)));
    }

    #[test]
    fn uses_pending_import_bytes_and_warns_without_frontmatter() {
        let p = TempProject::new(); // nothing in .skills yet
        let c = config(&[(AgentId::ClaudeCode, Placement::Copy)]);
        let pending = vec![
            Desired::file(rel(".skills/review/SKILL.md"), "# no frontmatter\n"),
            Desired::file(rel(".skills/review/scripts/check.sh"), "echo hi\n"),
        ];
        let pp =
            plan_placement(&p.root(), &c, &lock(), &HarnessState::default(), &pending).unwrap();
        assert_eq!(pp.desired[0].content, b"# no frontmatter\n");
        assert_eq!(pp.warnings.len(), 1);
        assert!(pp.warnings[0].contains("frontmatter"));
    }

    #[test]
    fn missing_source_is_a_conflict_not_an_error() {
        let p = TempProject::new();
        let c = config(&[(AgentId::ClaudeCode, Placement::Copy)]);
        let pp = plan_placement(&p.root(), &c, &lock(), &HarnessState::default(), &[]).unwrap();
        assert!(
            pp.desired.iter().all(
                |d| matches!(&d.ownership, Ownership::Unresolvable(m) if m.contains("missing"))
            )
        );
    }

    #[test]
    fn symlink_is_unavailable_with_reason_and_ignores_are_per_skill() {
        let m = methods(AgentId::ClaudeCode);
        assert!(m.iter().any(|(k, r)| *k == Method::Symlink && r.is_err()));
        assert!(m.iter().filter(|(_, r)| r.is_ok()).count() == 2);
        let c = config(&[
            (AgentId::ClaudeCode, Placement::Copy),
            (AgentId::Codex, Placement::Reference),
        ]);
        let ignores: Vec<_> = ignore_paths(&c, &lock())
            .iter()
            .map(|p| p.to_string())
            .collect();
        assert_eq!(ignores, [".claude/skills/review"]);
    }
}
