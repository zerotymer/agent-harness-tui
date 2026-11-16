//! Apply engine: plan, preview, backup, write, recover (Stage 1 spec §3,
//! §7 steps 5-6; instruction 0004).
//!
//! Callers (instructions 0005-0009) describe the desired files; this module
//! decides what is safe to write using ownership recorded in
//! `.harness/state.json`, then applies everything or restores what it can.
//! There is no directory-wide transaction: after a failure the report lists
//! what was restored and what could not be.

use crate::contract::{
    AppliedRecord, ContractFile, HarnessState, ManagedEntry, STATE_FILE, backup_path,
};
use crate::paths::{ProjectRoot, RelPath};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const APPLY_LOCK: &str = ".harness/apply.lock";

/// `sha256:<hex>` of `bytes`.
pub fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::from("sha256:");
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// One file the caller wants to exist with `content`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desired {
    pub path: RelPath,
    /// Full file content after applying.
    pub content: Vec<u8>,
    /// What the tool owns: the whole file, a managed block, or nothing.
    pub ownership: Ownership,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    /// The tool owns the whole file.
    File,
    /// The tool owns one block. Hashes come from the block parser (0005).
    Block {
        id: String,
        /// Hash of the block after applying.
        new_hash: String,
        /// Hash of the block currently on disk; `None` if absent.
        current_hash: Option<String>,
    },
    /// Created only when missing, never changed, never owned
    /// (e.g. an empty `completed.log`).
    CreateOnly,
    /// The caller could not build safe content (e.g. malformed block
    /// markers); always planned as a conflict, never written.
    Unresolvable(String),
}

impl Desired {
    pub fn file(path: RelPath, content: impl Into<Vec<u8>>) -> Self {
        Self {
            path,
            content: content.into(),
            ownership: Ownership::File,
        }
    }

    pub fn create_only(path: RelPath, content: impl Into<Vec<u8>>) -> Self {
        Self {
            path,
            content: content.into(),
            ownership: Ownership::CreateOnly,
        }
    }

    fn block_id(&self) -> Option<&str> {
        match &self.ownership {
            Ownership::Block { id, .. } => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    /// Already as desired; nothing to write.
    Unchanged,
    /// Exists and is left alone by design (create-only).
    Preserve,
    Conflict(String),
}

impl Action {
    fn writes(&self) -> bool {
        matches!(self, Self::Create | Self::Update)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub desired: Desired,
    pub action: Action,
    existed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Directories to create (existing ones are reported as kept).
    pub dirs: Vec<(RelPath, bool)>,
    pub items: Vec<PlanItem>,
}

impl Plan {
    pub fn conflicts(&self) -> impl Iterator<Item = &PlanItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.action, Action::Conflict(_)))
    }

    pub fn has_writes(&self) -> bool {
        self.items.iter().any(|i| i.action.writes()) || self.dirs.iter().any(|(_, e)| !e)
    }
}

/// Text preview, independent of the TUI.
impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (dir, exists) in &self.dirs {
            let verb = if *exists { "keep" } else { "mkdir" };
            writeln!(f, "{verb:<9} {dir}/")?;
        }
        for i in &self.items {
            let target = match i.desired.block_id() {
                Some(id) => format!("{} [block {id}]", i.desired.path),
                None => i.desired.path.to_string(),
            };
            match &i.action {
                Action::Create => writeln!(f, "{:<9} {target}", "create")?,
                Action::Update => writeln!(f, "{:<9} {target}", "update")?,
                Action::Unchanged => writeln!(f, "{:<9} {target}", "unchanged")?,
                Action::Preserve => writeln!(f, "{:<9} {target}", "preserve")?,
                Action::Conflict(why) => writeln!(f, "{:<9} {target}: {why}", "CONFLICT")?,
            }
        }
        Ok(())
    }
}

/// Builds a plan by comparing desired content, the recorded baseline and
/// the filesystem. Reads only.
pub fn plan(
    root: &ProjectRoot,
    state: &HarnessState,
    dirs: &[RelPath],
    desired: Vec<Desired>,
) -> io::Result<Plan> {
    let dirs = dirs
        .iter()
        .map(|d| (d.clone(), root.abs(d).is_dir()))
        .collect();
    let mut items = Vec::new();
    for d in desired {
        let abs = root.abs(&d.path);
        let current = match fs::read(&abs) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let action = match root.resolve(d.path.as_str()) {
            Err(e) => Action::Conflict(e.to_string()),
            Ok(_) if abs.is_dir() => Action::Conflict("a directory exists at this path".into()),
            Ok(_) => classify(state, &d, current.as_deref()),
        };
        items.push(PlanItem {
            existed: current.is_some(),
            desired: d,
            action,
        });
    }
    Ok(Plan { dirs, items })
}

fn classify(state: &HarnessState, d: &Desired, current: Option<&[u8]>) -> Action {
    if let Ownership::Unresolvable(why) = &d.ownership {
        return Action::Conflict(why.clone());
    }
    let Some(current) = current else {
        return Action::Create;
    };
    let baseline = state
        .managed
        .iter()
        .find(|m| m.path == d.path && m.block.as_deref() == d.block_id())
        .map(|m| m.hash.as_str());
    let (current_owned, new_owned) = match &d.ownership {
        Ownership::CreateOnly => return Action::Preserve,
        Ownership::Unresolvable(_) => unreachable!("handled above"),
        Ownership::File => (Some(sha256(current)), sha256(&d.content)),
        Ownership::Block {
            new_hash,
            current_hash,
            ..
        } => (current_hash.clone(), new_hash.clone()),
    };
    if current == d.content.as_slice() || current_owned.as_deref() == Some(new_owned.as_str()) {
        return Action::Unchanged;
    }
    match (&d.ownership, current_owned.as_deref(), baseline) {
        (_, Some(c), Some(b)) if c == b => Action::Update,
        // Inserting a new block into a user file is the intended use.
        (Ownership::Block { .. }, None, None) => Action::Update,
        (Ownership::Block { .. }, None, Some(_)) => {
            Action::Conflict("the managed block was removed by the user".into())
        }
        (_, _, None) => Action::Conflict(
            "exists but ownership is not recorded (no local state); resolve manually".into(),
        ),
        _ => Action::Conflict("modified since the last apply".into()),
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    /// Backup directory name and `last_success.id`; `None` if nothing ran.
    pub apply_id: Option<String>,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub preserved: Vec<String>,
    /// Set when applying failed; nothing was recorded as success.
    pub failed: Option<String>,
    pub restored: Vec<String>,
    /// Changes that could not be undone after a failure.
    pub residual: Vec<String>,
}

#[derive(Debug)]
pub enum ApplyError {
    /// The plan has conflicts; nothing was written.
    Conflicts(Vec<String>),
    /// Another apply is running, or a previous one crashed.
    Locked(PathBuf),
    Io(io::Error),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflicts(c) => write!(f, "resolve conflicts first: {}", c.join(", ")),
            Self::Locked(p) => write!(
                f,
                "{} exists: another apply is running, or a previous run stopped \
                 unexpectedly. Remove it once no other run is active.",
                p.display()
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<io::Error> for ApplyError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Applies `plan`. On success `state` is updated and written last; on
/// failure files are restored where possible and `state` is untouched.
pub fn apply(
    root: &ProjectRoot,
    plan: &Plan,
    state: &mut HarnessState,
) -> Result<ApplyReport, ApplyError> {
    apply_with(root, plan, state, None)
}

/// `fail_at`: test hook that fails the n-th write (0-based).
fn apply_with(
    root: &ProjectRoot,
    plan: &Plan,
    state: &mut HarnessState,
    fail_at: Option<usize>,
) -> Result<ApplyReport, ApplyError> {
    let conflicts: Vec<String> = plan
        .conflicts()
        .map(|i| i.desired.path.to_string())
        .collect();
    if !conflicts.is_empty() {
        return Err(ApplyError::Conflicts(conflicts));
    }

    let mut new_state = state.clone();
    for i in &plan.items {
        record_ownership(&mut new_state, i);
    }
    let mut report = ApplyReport::default();
    if !plan.has_writes() && new_state.managed == state.managed {
        fill_untouched(&mut report, plan);
        return Ok(report); // idempotent: nothing to do, nothing recorded
    }

    let _lock = ApplyLock::acquire(root)?;
    let (apply_id, at) = utc_now();
    let apply_id = unique_backup_id(root, &apply_id)?;
    new_state.last_success = Some(AppliedRecord {
        id: apply_id.clone(),
        at,
        tool_version: env!("CARGO_PKG_VERSION").into(),
    });
    report.apply_id = Some(apply_id.clone());

    let mut tx = Tx::new(root, &apply_id, fail_at);
    let result = (|| -> io::Result<()> {
        for (dir, exists) in &plan.dirs {
            if !exists {
                tx.mkdir(dir)?;
            }
        }
        for i in plan.items.iter().filter(|i| i.action.writes()) {
            tx.write(&i.desired.path, &i.desired.content, i.existed)?;
        }
        // Success is recorded last, inside the same recoverable sequence.
        let state_rel = RelPath::parse(STATE_FILE).expect("valid constant");
        let state_text = new_state
            .to_text()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let state_existed = root.abs(&state_rel).exists();
        tx.write(&state_rel, state_text.as_bytes(), state_existed)
    })();

    match result {
        Ok(()) => {
            for i in &plan.items {
                let p = i.desired.path.to_string();
                match i.action {
                    Action::Create => report.created.push(p),
                    Action::Update => report.updated.push(p),
                    Action::Unchanged => report.unchanged.push(p),
                    Action::Preserve => report.preserved.push(p),
                    Action::Conflict(_) => unreachable!("conflicts rejected above"),
                }
            }
            *state = new_state;
        }
        Err(e) => {
            report.failed = Some(e.to_string());
            let (restored, residual) = tx.rollback();
            report.restored = restored;
            report.residual = residual;
        }
    }
    Ok(report)
}

fn fill_untouched(report: &mut ApplyReport, plan: &Plan) {
    for i in &plan.items {
        let p = i.desired.path.to_string();
        if i.action == Action::Preserve {
            report.preserved.push(p);
        } else {
            report.unchanged.push(p);
        }
    }
}

/// Records the owned hash for File/Block items (adopting identical files).
fn record_ownership(state: &mut HarnessState, item: &PlanItem) {
    let d = &item.desired;
    let hash = match &d.ownership {
        Ownership::CreateOnly | Ownership::Unresolvable(_) => return,
        Ownership::File => sha256(&d.content),
        Ownership::Block { new_hash, .. } => new_hash.clone(),
    };
    let block = d.block_id().map(str::to_string);
    match state
        .managed
        .iter_mut()
        .find(|m| m.path == d.path && m.block == block)
    {
        Some(m) => m.hash = hash,
        None => state.managed.push(ManagedEntry {
            path: d.path.clone(),
            block,
            hash,
        }),
    }
}

/// Recoverable sequence of filesystem changes.
struct Tx<'a> {
    root: &'a ProjectRoot,
    apply_id: &'a str,
    fail_at: Option<usize>,
    writes: usize,
    /// (path, had a previous version backed up)
    written: Vec<(RelPath, bool)>,
    made_dirs: Vec<PathBuf>,
}

impl<'a> Tx<'a> {
    fn new(root: &'a ProjectRoot, apply_id: &'a str, fail_at: Option<usize>) -> Self {
        Self {
            root,
            apply_id,
            fail_at,
            writes: 0,
            written: Vec::new(),
            made_dirs: Vec::new(),
        }
    }

    /// Creates missing ancestors and records them for rollback.
    fn mkdir(&mut self, rel: &RelPath) -> io::Result<()> {
        let mut missing = Vec::new();
        let mut p = self.root.abs(rel);
        while !p.exists() {
            missing.push(p.clone());
            if !p.pop() {
                break;
            }
        }
        for dir in missing.into_iter().rev() {
            fs::create_dir(&dir)?;
            self.made_dirs.push(dir);
        }
        Ok(())
    }

    fn write(&mut self, rel: &RelPath, content: &[u8], existed: bool) -> io::Result<()> {
        let n = self.writes;
        self.writes += 1;
        if self.fail_at == Some(n) {
            return Err(io::Error::other(format!("injected failure at write {n}")));
        }
        let abs = self.root.abs(rel);
        if let Some(parent) = rel.as_str().rsplit_once('/').map(|(p, _)| p) {
            self.mkdir(&RelPath::parse(parent).expect("parent of a RelPath"))?;
        }
        if existed {
            let backup = backup_path(self.apply_id, rel).expect("slug id");
            let backup_abs = self.root.abs(&backup);
            fs::create_dir_all(backup_abs.parent().expect("has parent"))?;
            fs::copy(&abs, &backup_abs)?;
        }
        self.written.push((rel.clone(), existed));
        atomic_write(&abs, content)
    }

    /// Undoes writes in reverse order. Returns (restored, residual).
    fn rollback(self) -> (Vec<String>, Vec<String>) {
        let mut restored = Vec::new();
        let mut residual = Vec::new();
        for (rel, existed) in self.written.iter().rev() {
            let abs = self.root.abs(rel);
            let result = if *existed {
                let backup = self
                    .root
                    .abs(&backup_path(self.apply_id, rel).expect("slug id"));
                fs::read(&backup).and_then(|b| atomic_write(&abs, &b))
            } else {
                match fs::remove_file(&abs) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
            };
            match result {
                Ok(()) => restored.push(rel.to_string()),
                Err(e) => residual.push(format!("{rel}: {e}")),
            }
        }
        for dir in self.made_dirs.iter().rev() {
            // Only empty directories we created; anything else stays.
            if let Err(e) = fs::remove_dir(dir) {
                residual.push(format!("{}: {e}", dir.display()));
            }
        }
        (restored, residual)
    }
}

/// Writes via a sibling temp file and rename, keeping existing permissions.
fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.aht-tmp"));
    let result = (|| {
        fs::write(&tmp, content)?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Exclusive lock file; removed on drop.
struct ApplyLock(PathBuf);

impl ApplyLock {
    fn acquire(root: &ProjectRoot) -> Result<Self, ApplyError> {
        let path = root.abs(&RelPath::parse(APPLY_LOCK).expect("valid constant"));
        fs::create_dir_all(path.parent().expect("has parent"))?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Self(path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(ApplyError::Locked(path)),
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for ApplyLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn unique_backup_id(root: &ProjectRoot, base: &str) -> io::Result<String> {
    for n in 1.. {
        let id = if n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let dir = root
            .abs(&RelPath::parse(&format!("{}/{id}", crate::contract::BACKUPS_DIR)).expect("slug"));
        if !dir.exists() {
            return Ok(id);
        }
    }
    unreachable!()
}

/// (`YYYYMMDD-HHMMSS`, RFC 3339) in UTC.
// ponytail: UTC only, no tz crate; switch to `jiff` if local offsets are needed.
fn utc_now() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_utc(secs)
}

fn format_utc(secs: u64) -> (String, String) {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    (
        format!("{y:04}{mo:02}{d:02}-{h:02}{m:02}{s:02}"),
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-apply-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn root(&self) -> ProjectRoot {
            ProjectRoot::new(&self.0).unwrap()
        }
        fn read(&self, rel: &str) -> String {
            fs::read_to_string(self.0.join(rel)).unwrap()
        }
        fn write(&self, rel: &str, text: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rel(s: &str) -> RelPath {
        RelPath::parse(s).unwrap()
    }

    fn desired() -> Vec<Desired> {
        vec![
            Desired::file(rel("harness.toml"), "schema_version = 1\n"),
            Desired::file(rel(".skills/a/SKILL.md"), "# A\n"),
            Desired::create_only(rel("instructions/completed.log"), ""),
        ]
    }

    fn dirs() -> Vec<RelPath> {
        vec![rel("instructions/.completed")]
    }

    fn run(p: &TempProject, state: &mut HarnessState, d: Vec<Desired>) -> ApplyReport {
        let root = p.root();
        let plan = plan(&root, state, &dirs(), d).unwrap();
        apply(&root, &plan, state).unwrap()
    }

    #[test]
    fn fresh_apply_creates_and_records_success_last() {
        let p = TempProject::new();
        let mut state = HarnessState::default();
        let report = run(&p, &mut state, desired());
        assert_eq!(report.failed, None);
        assert_eq!(report.created.len(), 3);
        assert_eq!(p.read(".skills/a/SKILL.md"), "# A\n");
        assert!(p.0.join("instructions/.completed").is_dir());
        // create-only files are never owned
        assert_eq!(state.managed.len(), 2);
        let saved = HarnessState::load(&p.0).unwrap().unwrap();
        assert_eq!(saved, state);
        assert_eq!(saved.last_success.unwrap().id, report.apply_id.unwrap());
        assert!(!p.0.join(APPLY_LOCK).exists());
    }

    #[test]
    fn repeat_apply_is_a_no_op() {
        let p = TempProject::new();
        let mut state = HarnessState::default();
        run(&p, &mut state, desired());
        let before = p.read(STATE_FILE);
        let report = run(&p, &mut state, desired());
        assert_eq!(report.apply_id, None);
        assert_eq!(report.unchanged.len(), 2);
        assert_eq!(report.preserved, vec!["instructions/completed.log"]);
        assert_eq!(p.read(STATE_FILE), before);
    }

    #[test]
    fn owned_file_is_updated_with_backup() {
        let p = TempProject::new();
        let mut state = HarnessState::default();
        run(&p, &mut state, desired());
        let mut d = desired();
        d[1] = Desired::file(rel(".skills/a/SKILL.md"), "# A v2\n");
        let report = run(&p, &mut state, d);
        assert_eq!(report.updated, vec![".skills/a/SKILL.md"]);
        assert_eq!(p.read(".skills/a/SKILL.md"), "# A v2\n");
        let backup = format!(
            ".harness/backups/{}/.skills/a/SKILL.md",
            report.apply_id.unwrap()
        );
        assert_eq!(p.read(&backup), "# A\n");
    }

    #[test]
    fn user_edits_and_unowned_files_are_conflicts() {
        let p = TempProject::new();
        let mut state = HarnessState::default();
        run(&p, &mut state, desired());
        p.write(".skills/a/SKILL.md", "# A edited by user\n");
        let mut d = desired();
        d[1] = Desired::file(rel(".skills/a/SKILL.md"), "# A v2\n");
        let root = p.root();
        let plan = plan(&root, &state, &dirs(), d).unwrap();
        assert!(matches!(&plan.items[1].action, Action::Conflict(m) if m.contains("modified")));
        assert!(matches!(
            apply(&root, &plan, &mut state),
            Err(ApplyError::Conflicts(_))
        ));
        assert_eq!(p.read(".skills/a/SKILL.md"), "# A edited by user\n");
    }

    #[test]
    fn clone_without_local_state_does_not_overwrite() {
        let p = TempProject::new();
        p.write("harness.toml", "schema_version = 1\n"); // identical: adopted
        p.write(".skills/a/SKILL.md", "# someone else's A\n"); // different: conflict
        let root = p.root();
        let state = HarnessState::default();
        let plan = plan(&root, &state, &dirs(), desired()).unwrap();
        assert_eq!(plan.items[0].action, Action::Unchanged);
        assert!(matches!(&plan.items[1].action, Action::Conflict(m) if m.contains("ownership")));
    }

    #[test]
    fn injected_failure_restores_files_and_records_nothing() {
        let p = TempProject::new();
        let mut state = HarnessState::default();
        run(&p, &mut state, desired());
        let state_before = p.read(STATE_FILE);
        let mut d = desired();
        d[0] = Desired::file(rel("harness.toml"), "schema_version = 1\n# v2\n");
        d[1] = Desired::file(rel(".skills/a/SKILL.md"), "# A v2\n");
        d.push(Desired::file(rel("new/dir/file.md"), "new\n"));
        let root = p.root();
        let plan = plan(&root, &state, &dirs(), d).unwrap();
        for fail_at in 0..4 {
            let mut s = state.clone();
            let report = apply_with(&root, &plan, &mut s, Some(fail_at)).unwrap();
            assert!(report.failed.is_some(), "fail_at {fail_at}");
            assert!(report.residual.is_empty(), "{:?}", report.residual);
            assert_eq!(s, state, "in-memory state untouched");
            assert_eq!(p.read("harness.toml"), "schema_version = 1\n");
            assert_eq!(p.read(".skills/a/SKILL.md"), "# A\n");
            assert!(!p.0.join("new").exists(), "created dirs removed");
            assert_eq!(p.read(STATE_FILE), state_before, "success not recorded");
            assert!(!p.0.join(APPLY_LOCK).exists());
        }
    }

    #[test]
    fn block_ownership_rules() {
        let state = HarnessState::default();
        let block = |current: Option<&str>| Desired {
            path: rel("CLAUDE.md"),
            content: b"user text\n<block v2>\n".to_vec(),
            ownership: Ownership::Block {
                id: "agent-harness".into(),
                new_hash: sha256(b"v2"),
                current_hash: current.map(|c| sha256(c.as_bytes())),
            },
        };
        let user_file = Some(b"user text\n".as_slice());
        // New block into a user file: allowed.
        assert_eq!(classify(&state, &block(None), user_file), Action::Update);
        // Block present but no baseline (fresh clone): cannot prove ownership.
        assert!(matches!(
            classify(&state, &block(Some("v1")), user_file),
            Action::Conflict(_)
        ));
        // Block identical while the rest differs: nothing to write.
        assert_eq!(
            classify(&state, &block(Some("v2")), user_file),
            Action::Unchanged
        );

        let mut owned = HarnessState::default();
        owned.managed.push(ManagedEntry {
            path: rel("CLAUDE.md"),
            block: Some("agent-harness".into()),
            hash: sha256(b"v1"),
        });
        assert_eq!(
            classify(&owned, &block(Some("v1")), user_file),
            Action::Update
        );
        assert!(matches!(
            classify(&owned, &block(Some("edited")), user_file),
            Action::Conflict(_)
        ));
        assert!(
            matches!(classify(&owned, &block(None), user_file), Action::Conflict(m) if m.contains("removed"))
        );
    }

    #[test]
    fn stale_lock_blocks_apply() {
        let p = TempProject::new();
        p.write(APPLY_LOCK, "");
        let root = p.root();
        let mut state = HarnessState::default();
        let plan = plan(&root, &state, &dirs(), desired()).unwrap();
        assert!(matches!(
            apply(&root, &plan, &mut state),
            Err(ApplyError::Locked(_))
        ));
        assert!(!p.0.join("harness.toml").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_a_conflict_and_permissions_survive() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let p = TempProject::new();
        let outside = TempProject::new();
        symlink(&outside.0, p.0.join(".skills")).unwrap();
        let root = p.root();
        let state = HarnessState::default();
        let plan = plan(&root, &state, &[], desired()).unwrap();
        assert!(matches!(plan.items[1].action, Action::Conflict(_)));

        let q = TempProject::new();
        let mut state = HarnessState::default();
        run(&q, &mut state, vec![Desired::file(rel("run.sh"), "v1")]);
        fs::set_permissions(q.0.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        run(&q, &mut state, vec![Desired::file(rel("run.sh"), "v2")]);
        let mode = fs::metadata(q.0.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn preview_lists_every_item() {
        let p = TempProject::new();
        let plan = plan(&p.root(), &HarnessState::default(), &dirs(), desired()).unwrap();
        let text = plan.to_string();
        assert!(text.contains("mkdir     instructions/.completed/"));
        assert!(text.contains("create    .skills/a/SKILL.md"));
    }

    #[test]
    fn utc_formatting() {
        assert_eq!(
            format_utc(0),
            ("19700101-000000".into(), "1970-01-01T00:00:00Z".into())
        );
        // 2026-11-16T05:00:00Z
        assert_eq!(format_utc(1_794_805_200).1, "2026-11-16T05:00:00Z");
        // Leap day 2024-02-29T12:34:56Z
        assert_eq!(format_utc(1_709_210_096).0, "20240229-123456");
        assert_eq!(sha256(b"").len(), 71);
    }
}
