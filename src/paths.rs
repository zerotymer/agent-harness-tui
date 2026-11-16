//! Project-root and path rules (Stage 1 spec §1, instruction 0002).
//!
//! Every managed path is stored as a portable, project-relative `RelPath`
//! (`/`-separated) and resolved against the project root, never against the
//! executable or a config file location.

use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Fixed in Stage 1.
pub const INSTRUCTIONS_DIR: &str = "instructions";
/// Fixed in Stage 1.
pub const SKILLS_DIR: &str = ".skills";
/// Enable/disable only in Stage 1.
pub const BACKLOG_FILE: &str = "agent.db";
pub const DEFAULT_REFERENCES_DIR: &str = "references";
pub const DEFAULT_OUTPUT_DIR: &str = ".output";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    /// The input resolves to the project root itself.
    IsRoot(String),
    /// `..` or an absolute path leaves the project root.
    OutsideRoot(String),
    /// A symlink (or dangling symlink) resolves outside the project root.
    SymlinkEscape(String),
    /// Two managed paths share a destination (compared case-insensitively).
    Duplicate(String, String),
    /// One managed path is nested inside another.
    Nested {
        outer: String,
        inner: String,
    },
    /// An existing filesystem entry has the wrong kind.
    Collision(String),
    /// Output directory names must start with a dot.
    OutputNotDotPrefixed(String),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "path is empty"),
            Self::IsRoot(p) => write!(f, "`{p}` is the project root itself"),
            Self::OutsideRoot(p) => write!(f, "`{p}` is outside the project root"),
            Self::SymlinkEscape(p) => {
                write!(
                    f,
                    "`{p}` resolves through a symlink outside the project root"
                )
            }
            Self::Duplicate(a, b) => write!(f, "`{a}` and `{b}` are the same destination"),
            Self::Nested { outer, inner } => {
                write!(f, "`{inner}` is nested inside managed path `{outer}`")
            }
            Self::Collision(p) => write!(f, "`{p}` collides with an existing entry"),
            Self::OutputNotDotPrefixed(p) => {
                write!(f, "output directory `{p}` must have a dot-prefixed name")
            }
        }
    }
}

impl std::error::Error for PathError {}

/// Normalized project-relative path: non-empty, `/`-separated, no `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(String);

impl RelPath {
    /// Lexically validates a stored project-relative path. No filesystem
    /// access, so symlinks are checked later by `ProjectRoot::resolve`.
    pub fn parse(input: &str) -> Result<RelPath, PathError> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err(PathError::Empty);
        }
        let unified = raw.replace('\\', "/");
        if is_windows_drive(&unified) || unified.starts_with('/') {
            return Err(PathError::OutsideRoot(raw.to_string()));
        }
        normalize_components(Path::new(&unified), raw)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Last path component.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub fn join(&self, child: &str) -> RelPath {
        RelPath(format!("{}/{child}", self.0))
    }

    /// True if `self` is `other` or lies under it.
    fn is_within(&self, other: &RelPath) -> bool {
        let (a, b) = (self.0.to_lowercase(), other.0.to_lowercase());
        a == b || a.starts_with(&format!("{b}/"))
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for RelPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for RelPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        RelPath::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// The project root: always the directory the tool runs in.
#[derive(Debug, Clone)]
pub struct ProjectRoot {
    root: PathBuf,
}

impl ProjectRoot {
    /// Uses the current working directory. Never searches upward.
    pub fn from_cwd() -> io::Result<Self> {
        Self::new(std::env::current_dir()?)
    }

    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: path.as_ref().canonicalize()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn abs(&self, rel: &RelPath) -> PathBuf {
        rel.0.split('/').fold(self.root.clone(), |p, c| p.join(c))
    }

    /// Validates user input (relative, or absolute inside the root) and
    /// returns its project-relative form.
    pub fn resolve(&self, input: &str) -> Result<RelPath, PathError> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err(PathError::Empty);
        }
        // ponytail: `\` is always a separator so configs stay portable;
        // a literal backslash in a Unix file name is not supported.
        let unified = raw.replace('\\', "/");

        let drive = is_windows_drive(&unified);
        let rel = if drive && !cfg!(windows) {
            return Err(PathError::OutsideRoot(raw.to_string()));
        } else if drive || Path::new(&unified).is_absolute() || unified.starts_with('/') {
            // Windows parses both separators natively and needs `\` intact for
            // verbatim `\\?\` prefixes. Canonicalizing the existing part lets
            // `C:\..`, `\\?\C:\..`, other letter case and symlinked aliases
            // match the canonical root (0013).
            let native = if cfg!(windows) { raw } else { unified.as_str() };
            let abs = lexical_normalize(Path::new(native))
                .map(|p| canonicalize_existing(&p))
                .ok_or_else(|| PathError::OutsideRoot(raw.to_string()))?;
            let inner = abs
                .strip_prefix(&self.root)
                .map_err(|_| PathError::OutsideRoot(raw.to_string()))?;
            normalize_components(inner, raw)?
        } else {
            normalize_components(Path::new(&unified), raw)?
        };
        self.check_symlinks(&rel)?;
        Ok(rel)
    }

    /// Rejects paths whose existing part resolves outside the root.
    fn check_symlinks(&self, rel: &RelPath) -> Result<(), PathError> {
        let mut current = self.root.clone();
        for part in rel.0.split('/') {
            current.push(part);
            match current.symlink_metadata() {
                Err(_) => return Ok(()), // rest does not exist yet
                Ok(_) => match current.canonicalize() {
                    Ok(real) if real.starts_with(&self.root) => {}
                    // Escaping or dangling symlink: its target is outside our control.
                    _ => return Err(PathError::SymlinkEscape(rel.to_string())),
                },
            }
        }
        Ok(())
    }
}

/// Folds `.`/`..` of a relative path into a `RelPath`; `raw` is for errors.
fn normalize_components(relative: &Path, raw: &str) -> Result<RelPath, PathError> {
    let mut parts: Vec<String> = Vec::new();
    for c in relative.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts
                    .pop()
                    .ok_or_else(|| PathError::OutsideRoot(raw.to_string()))?;
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PathError::OutsideRoot(raw.to_string()));
            }
        }
    }
    if parts.is_empty() {
        return Err(PathError::IsRoot(raw.to_string()));
    }
    Ok(RelPath(parts.join("/")))
}

fn is_windows_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Canonicalizes the longest existing ancestor and re-appends the rest.
fn canonicalize_existing(path: &Path) -> PathBuf {
    let mut base = path;
    let mut tail = Vec::new();
    loop {
        if let Ok(real) = base.canonicalize() {
            return tail.iter().rev().fold(real, |p, c| p.join(c));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                base = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Resolves `.` and `..` without touching the filesystem.
fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    Some(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

/// Stage 1 managed layout. Fixed entries are not configurable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub references: RelPath,
    pub output: RelPath,
    pub backlog_enabled: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            references: RelPath(DEFAULT_REFERENCES_DIR.into()),
            output: RelPath(DEFAULT_OUTPUT_DIR.into()),
            backlog_enabled: false,
        }
    }
}

impl Layout {
    pub fn instructions() -> RelPath {
        RelPath(INSTRUCTIONS_DIR.into())
    }
    pub fn completed() -> RelPath {
        Self::instructions().join(".completed")
    }
    pub fn completed_log() -> RelPath {
        Self::instructions().join("completed.log")
    }
    pub fn skills() -> RelPath {
        RelPath(SKILLS_DIR.into())
    }
    pub fn backlog() -> RelPath {
        RelPath(BACKLOG_FILE.into())
    }

    /// Top-level managed entries that must not overlap.
    pub fn entries(&self) -> Vec<(RelPath, EntryKind)> {
        vec![
            (Self::instructions(), EntryKind::Dir),
            (self.references.clone(), EntryKind::Dir),
            (self.output.clone(), EntryKind::Dir),
            (Self::skills(), EntryKind::Dir),
            // Reserved even when disabled, so it is always checked.
            (Self::backlog(), EntryKind::File),
        ]
    }

    /// Checks dot prefix, duplicates, nesting and collisions with existing
    /// entries. Returns every problem, not just the first.
    pub fn validate(&self, root: &ProjectRoot) -> Result<(), Vec<PathError>> {
        let mut errors = Vec::new();
        if !self.output.file_name().starts_with('.') {
            errors.push(PathError::OutputNotDotPrefixed(self.output.to_string()));
        }
        let entries = self.entries();
        for (i, (a, _)) in entries.iter().enumerate() {
            for (b, _) in &entries[i + 1..] {
                if a.as_str().to_lowercase() == b.as_str().to_lowercase() {
                    errors.push(PathError::Duplicate(a.to_string(), b.to_string()));
                } else if b.is_within(a) {
                    errors.push(PathError::Nested {
                        outer: a.to_string(),
                        inner: b.to_string(),
                    });
                } else if a.is_within(b) {
                    errors.push(PathError::Nested {
                        outer: b.to_string(),
                        inner: a.to_string(),
                    });
                }
            }
        }
        for (rel, kind) in &entries {
            if let Some(e) = collision(root, rel, *kind) {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// An ancestor that is a file, or the entry itself with the wrong kind.
fn collision(root: &ProjectRoot, rel: &RelPath, kind: EntryKind) -> Option<PathError> {
    let mut current = root.path().to_path_buf();
    let parts: Vec<&str> = rel.as_str().split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part);
        let Ok(meta) = std::fs::metadata(&current) else {
            return None;
        };
        let last = i + 1 == parts.len();
        let ok = if last {
            match kind {
                EntryKind::Dir => meta.is_dir(),
                EntryKind::File => meta.is_file(),
            }
        } else {
            meta.is_dir()
        };
        if !ok {
            return Some(PathError::Collision(rel.to_string()));
        }
    }
    None
}

/// Result of trying to hide a directory with a platform attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HiddenAttr {
    Applied,
    /// The platform hides dot-prefixed names; no attribute exists.
    NotApplicable,
    Failed(String),
}

/// Applies the Windows hidden attribute. A leading dot alone does not hide
/// a directory on Windows, so callers must report `Failed` to the user.
pub fn apply_hidden_attribute(path: &Path) -> HiddenAttr {
    if !cfg!(windows) {
        return HiddenAttr::NotApplicable;
    }
    // ponytail: built-in `attrib` instead of a Win32 binding crate;
    // switch to windows-sys if this needs to be silent or faster.
    match std::process::Command::new("attrib")
        .arg("+h")
        .arg(path)
        .output()
    {
        Ok(out) if out.status.success() => HiddenAttr::Applied,
        Ok(out) => HiddenAttr::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => HiddenAttr::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Temporary target project, removed on drop.
    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-paths-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
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

    fn layout(references: &str, output: &str) -> Layout {
        Layout {
            references: RelPath(references.into()),
            output: RelPath(output.into()),
            backlog_enabled: false,
        }
    }

    #[test]
    fn resolves_relative_and_normalizes() {
        let p = TempProject::new();
        let root = p.root();
        assert_eq!(root.resolve("references").unwrap().as_str(), "references");
        assert_eq!(root.resolve("./a/./b/../c").unwrap().as_str(), "a/c");
        assert_eq!(root.resolve("docs\\ref").unwrap().as_str(), "docs/ref");
        assert_eq!(root.resolve("  .output  ").unwrap().as_str(), ".output");
    }

    #[test]
    fn supports_spaces_and_korean() {
        let p = TempProject::new();
        let root = p.root();
        assert_eq!(
            root.resolve("참조 문서/설계").unwrap().as_str(),
            "참조 문서/설계"
        );
        fs::create_dir_all(p.0.join("참조 문서")).unwrap();
        assert_eq!(root.resolve("참조 문서").unwrap().as_str(), "참조 문서");
    }

    #[test]
    fn rejects_empty_and_root() {
        let root = TempProject::new().root();
        assert_eq!(root.resolve("  "), Err(PathError::Empty));
        assert!(matches!(root.resolve("."), Err(PathError::IsRoot(_))));
        assert!(matches!(root.resolve("a/.."), Err(PathError::IsRoot(_))));
    }

    #[test]
    fn rejects_parent_escape() {
        let root = TempProject::new().root();
        assert!(matches!(root.resolve(".."), Err(PathError::OutsideRoot(_))));
        assert!(matches!(
            root.resolve("a/../../x"),
            Err(PathError::OutsideRoot(_))
        ));
        assert!(matches!(
            root.resolve("..\\x"),
            Err(PathError::OutsideRoot(_))
        ));
    }

    #[test]
    fn absolute_inside_root_becomes_relative() {
        let p = TempProject::new();
        let root = p.root();
        let inside = root.path().join("refs").join("a");
        assert_eq!(
            root.resolve(inside.to_str().unwrap()).unwrap().as_str(),
            "refs/a"
        );
        let outside = root.path().parent().unwrap().join("other");
        assert!(matches!(
            root.resolve(outside.to_str().unwrap()),
            Err(PathError::OutsideRoot(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn absolute_through_symlinked_alias_becomes_relative() {
        let p = TempProject::new();
        let alias = p.0.with_extension("alias");
        std::os::unix::fs::symlink(&p.0, &alias).unwrap();
        let input = alias.join("refs").join("a");
        let got = p.root().resolve(input.to_str().unwrap());
        let _ = fs::remove_file(&alias);
        assert_eq!(got.unwrap().as_str(), "refs/a");
    }

    /// Plain `C:\..` input in another letter case, as users type it.
    #[cfg(windows)]
    #[test]
    fn plain_drive_path_in_other_case_becomes_relative() {
        let p = TempProject::new();
        let root = p.root();
        let plain = root.path().to_str().unwrap().trim_start_matches(r"\\?\");
        let input = format!(r"{}\refs\a", plain.to_lowercase());
        assert_eq!(root.resolve(&input).unwrap().as_str(), "refs/a");
        let slashed = format!("{}/refs/a", plain.replace('\\', "/"));
        assert_eq!(root.resolve(&slashed).unwrap().as_str(), "refs/a");
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_drive_path_is_outside_on_unix() {
        let root = TempProject::new().root();
        for input in ["C:\\project\\refs", "d:/data", "C:"] {
            assert!(
                matches!(root.resolve(input), Err(PathError::OutsideRoot(_))),
                "{input}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let p = TempProject::new();
        let outside = TempProject::new();
        symlink(&outside.0, p.0.join("link")).unwrap();
        symlink(p.0.join("missing"), p.0.join("dangling")).unwrap();
        fs::create_dir(p.0.join("real")).unwrap();
        symlink(p.0.join("real"), p.0.join("inner-link")).unwrap();
        let root = p.root();
        assert!(matches!(
            root.resolve("link"),
            Err(PathError::SymlinkEscape(_))
        ));
        assert!(matches!(
            root.resolve("link/sub"),
            Err(PathError::SymlinkEscape(_))
        ));
        assert!(matches!(
            root.resolve("dangling"),
            Err(PathError::SymlinkEscape(_))
        ));
        assert_eq!(
            root.resolve("inner-link/x").unwrap().as_str(),
            "inner-link/x"
        );
    }

    #[test]
    fn default_layout_is_valid_and_derives_fixed_paths() {
        let root = TempProject::new().root();
        assert_eq!(Layout::default().validate(&root), Ok(()));
        assert_eq!(Layout::completed().as_str(), "instructions/.completed");
        assert_eq!(
            Layout::completed_log().as_str(),
            "instructions/completed.log"
        );
        assert_eq!(
            root.abs(&Layout::completed_log()),
            root.path().join("instructions").join("completed.log")
        );
    }

    #[test]
    fn output_must_be_dot_prefixed() {
        let root = TempProject::new().root();
        let errs = layout("references", "build/out")
            .validate(&root)
            .unwrap_err();
        assert!(errs.contains(&PathError::OutputNotDotPrefixed("build/out".into())));
        assert_eq!(layout("references", "build/.out").validate(&root), Ok(()));
    }

    #[test]
    fn rejects_duplicates_case_insensitively() {
        let root = TempProject::new().root();
        let errs = layout(".Skills", ".output").validate(&root).unwrap_err();
        assert!(matches!(errs[0], PathError::Duplicate(_, _)), "{errs:?}");
    }

    #[test]
    fn rejects_nesting() {
        let root = TempProject::new().root();
        let errs = layout("instructions/refs", ".output")
            .validate(&root)
            .unwrap_err();
        assert!(errs.contains(&PathError::Nested {
            outer: "instructions".into(),
            inner: "instructions/refs".into()
        }));
        let errs = layout("docs", "docs/.out").validate(&root).unwrap_err();
        assert!(errs.contains(&PathError::Nested {
            outer: "docs".into(),
            inner: "docs/.out".into()
        }));
        // Sibling with a shared name prefix is not nesting.
        assert_eq!(layout("refs", "refs-x/.out").validate(&root), Ok(()));
    }

    #[test]
    fn rejects_collisions_with_existing_files() {
        let p = TempProject::new();
        fs::write(p.0.join("references"), "file, not dir").unwrap();
        fs::create_dir(p.0.join("agent.db")).unwrap();
        fs::write(p.0.join("parent"), "").unwrap();
        let root = p.root();
        let errs = Layout::default().validate(&root).unwrap_err();
        assert!(errs.contains(&PathError::Collision("references".into())));
        assert!(errs.contains(&PathError::Collision("agent.db".into())));
        let errs = layout("parent/child", ".output")
            .validate(&root)
            .unwrap_err();
        assert!(errs.contains(&PathError::Collision("parent/child".into())));
    }

    #[test]
    fn existing_directories_are_preserved_not_collisions() {
        let p = TempProject::new();
        fs::create_dir_all(p.0.join("instructions/.completed")).unwrap();
        fs::create_dir(p.0.join("references")).unwrap();
        assert_eq!(Layout::default().validate(&p.root()), Ok(()));
    }

    #[cfg(not(windows))]
    #[test]
    fn hidden_attribute_not_applicable_off_windows() {
        assert_eq!(
            apply_hidden_attribute(Path::new(".output")),
            HiddenAttr::NotApplicable
        );
    }
}
