//! Local JSON catalogs (Stage 2 spec §1, `HARNESS_CATALOG_CONTRACT`).
//!
//! Three files share one envelope: `wiki.json`, `skills.json`, `docs.json`.
//! The distributor writes them; this module only reads local files and never
//! touches the network. Each catalog is looked up in a fixed order and the
//! first hit is used; a broken file is reported, never silently replaced.

use crate::contract::{is_page_type, is_slug, is_uuid};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wiki,
    Skills,
    Docs,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Wiki => "wiki",
            Self::Skills => "skills",
            Self::Docs => "docs",
        }
    }

    pub fn env_var(self) -> &'static str {
        match self {
            Self::Wiki => "AGENT_HARNESS_WIKI_CATALOG",
            Self::Skills => "AGENT_HARNESS_SKILLS_CATALOG",
            Self::Docs => "AGENT_HARNESS_DOCS_CATALOG",
        }
    }

    /// Sample compiled into the binary from this repository's `catalog/`.
    pub fn builtin(self) -> &'static str {
        match self {
            Self::Wiki => include_str!("../catalog/wiki.json"),
            Self::Skills => include_str!("../catalog/skills.json"),
            Self::Docs => include_str!("../catalog/docs.json"),
        }
    }
}

/// `{ "en"?, "ko"? }`; at least one language. Missing ones are never made up.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub en: Option<String>,
    pub ko: Option<String>,
}

impl Text {
    /// Korean first (the UI language), then English.
    pub fn display(&self) -> &str {
        self.ko.as_deref().or(self.en.as_deref()).unwrap_or("")
    }

    fn valid(&self) -> bool {
        [&self.en, &self.ko]
            .iter()
            .any(|t| t.as_deref().is_some_and(|s| !s.trim().is_empty()))
            && [&self.en, &self.ko]
                .iter()
                .all(|t| t.as_deref().is_none_or(|s| !s.trim().is_empty()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Skill,
    Doc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Active,
    Archived,
}

// ponytail: the common fields are repeated per entry type because serde's
// `flatten` does not work together with `deny_unknown_fields`.

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiEntry {
    pub id: String,
    pub title: Text,
    pub summary: Option<Text>,
    pub version: Option<String>,
    pub updated: Option<String>,
    pub uuid: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub target: Option<Target>,
    #[serde(default)]
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillsEntry {
    pub id: String,
    pub title: Text,
    pub summary: Option<Text>,
    pub version: Option<String>,
    pub updated: Option<String>,
    pub installer: String,
    pub source: String,
    pub skill: Option<String>,
    pub install: Option<String>,
}

pub const SKILLS_CLI: &str = "skills-cli";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocsEntry {
    pub id: String,
    pub title: Text,
    pub summary: Option<Text>,
    pub version: Option<String>,
    pub updated: Option<String>,
    pub url: String,
    pub filename: String,
    pub sha256: Option<String>,
}

/// One entry type of a catalog file.
pub trait Entry: DeserializeOwned {
    const KIND: Kind;
    fn id(&self) -> &str;
    fn title(&self) -> &Text;
    fn summary(&self) -> Option<&Text>;
    fn updated(&self) -> Option<&str>;
    /// Rules specific to this entry type.
    fn check(&self, problems: &mut Vec<String>);
}

macro_rules! common_fields {
    () => {
        fn id(&self) -> &str {
            &self.id
        }
        fn title(&self) -> &Text {
            &self.title
        }
        fn summary(&self) -> Option<&Text> {
            self.summary.as_ref()
        }
        fn updated(&self) -> Option<&str> {
            self.updated.as_deref()
        }
    };
}

impl Entry for WikiEntry {
    const KIND: Kind = Kind::Wiki;
    common_fields!();

    fn check(&self, p: &mut Vec<String>) {
        if !is_uuid(&self.uuid) {
            p.push(format!("`{}`: `uuid` is not a UUID", self.id));
        }
        if !is_page_type(&self.page_type) {
            p.push(format!("`{}`: `type` must be slugs joined by `/`", self.id));
        }
        if self.target == Some(Target::Skill) && !is_slug(&self.id) {
            p.push(format!(
                "`{}`: a `skill` target needs a slug `id` (it becomes the directory name)",
                self.id
            ));
        }
    }
}

impl Entry for SkillsEntry {
    const KIND: Kind = Kind::Skills;
    common_fields!();

    fn check(&self, p: &mut Vec<String>) {
        let id = &self.id;
        if self.installer.is_empty() || self.installer.chars().any(char::is_control) {
            p.push(format!("`{id}`: invalid `installer`"));
        }
        if !is_cli_arg(&self.source) {
            p.push(format!(
                "`{id}`: `source` must not be empty, start with `-`, or contain spaces or control characters"
            ));
        }
        if self.installer == SKILLS_CLI {
            if self.install.is_some() {
                p.push(format!(
                    "`{id}`: a `{SKILLS_CLI}` entry must not have `install`"
                ));
            }
            if self.skill.as_deref().is_some_and(|s| !is_cli_arg(s)) {
                p.push(format!(
                    "`{id}`: `skill` must not be empty, start with `-`, or contain spaces or control characters"
                ));
            }
        } else {
            if self.skill.is_some() {
                p.push(format!(
                    "`{id}`: only `{SKILLS_CLI}` entries may have `skill`"
                ));
            }
            if self.install.as_deref().is_none_or(|s| s.trim().is_empty()) {
                p.push(format!(
                    "`{id}`: installer `{}` needs `install` text",
                    self.installer
                ));
            }
        }
    }
}

impl Entry for DocsEntry {
    const KIND: Kind = Kind::Docs;
    common_fields!();

    fn check(&self, p: &mut Vec<String>) {
        let id = &self.id;
        let url_ok = self
            .url
            .strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty())
            && !self
                .url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control());
        if !url_ok {
            p.push(format!("`{id}`: `url` must be an `https://` URL"));
        }
        let f = &self.filename;
        if f.is_empty()
            || f == "."
            || f == ".."
            || f.chars().any(|c| matches!(c, '/' | '\\') || c.is_control())
        {
            p.push(format!(
                "`{id}`: `filename` must be a plain file name without path separators"
            ));
        }
        if self.sha256.as_deref().is_some_and(|h| {
            h.len() != 64
                || !h
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        }) {
            p.push(format!("`{id}`: `sha256` must be 64 lowercase hex digits"));
        }
    }
}

/// A source argument passed to the CLI without a shell.
fn is_cli_arg(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`.
// ponytail: shape only; field ranges (month 13, hour 25) are not checked.
fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r)
            .is_some_and(|x| !x.is_empty() && x.iter().all(u8::is_ascii_digit))
    };
    let at = |i: usize, c: &[u8]| b.get(i).is_some_and(|x| c.contains(x));
    if !(digits(0..4)
        && at(4, b"-")
        && digits(5..7)
        && at(7, b"-")
        && digits(8..10)
        && at(10, b"Tt")
        && digits(11..13)
        && at(13, b":")
        && digits(14..16)
        && at(16, b":")
        && digits(17..19))
    {
        return false;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return false;
        }
        rest = &frac[n..];
    }
    let r = rest.as_bytes();
    matches!(rest, "Z" | "z")
        || (r.len() == 6
            && matches!(r[0], b'+' | b'-')
            && r[1..3].iter().all(u8::is_ascii_digit)
            && r[3] == b':'
            && r[4..6].iter().all(u8::is_ascii_digit))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File<E> {
    #[allow(dead_code)] // checked through the version probe first
    schema_version: u64,
    kind: String,
    catalog_version: Option<String>,
    entries: Vec<E>,
}

/// A validated catalog without its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<E> {
    pub catalog_version: Option<String>,
    pub entries: Vec<E>,
}

/// Parses and validates one catalog file. `schema_version` is read first.
pub fn parse<E: Entry>(text: &str) -> Result<Parsed<E>, String> {
    if text.starts_with('\u{feff}') {
        return Err("UTF-8 byte order mark is not allowed".into());
    }
    let probe: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    match probe
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(SCHEMA_VERSION) => {}
        Some(v) => return Err(format!("unsupported schema_version {v}")),
        None => return Err("`schema_version` (integer) is missing".into()),
    }
    let file: File<E> = serde_json::from_value(probe).map_err(|e| e.to_string())?;
    if file.kind != E::KIND.name() {
        return Err(format!(
            "`kind` is `{}`, expected `{}`",
            file.kind,
            E::KIND.name()
        ));
    }
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    for e in &file.entries {
        let id = e.id();
        if id.is_empty() || id.chars().any(char::is_control) {
            problems.push(format!("invalid `id` `{}`", id.escape_debug()));
        }
        if !seen.insert(id.to_lowercase()) {
            problems.push(format!("duplicate `id` `{id}`"));
        }
        if !e.title().valid() {
            problems.push(format!(
                "`{id}`: `title` needs at least one non-empty language"
            ));
        }
        if e.summary().is_some_and(|s| !s.valid()) {
            problems.push(format!(
                "`{id}`: `summary` needs at least one non-empty language"
            ));
        }
        if e.updated().is_some_and(|u| !is_rfc3339(u)) {
            problems.push(format!("`{id}`: `updated` is not an RFC 3339 time"));
        }
        e.check(&mut problems);
    }
    if problems.is_empty() {
        Ok(Parsed {
            catalog_version: file.catalog_version,
            entries: file.entries,
        })
    } else {
        Err(problems.join("; "))
    }
}

/// Where a catalog came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Path from the kind's environment variable.
    Env(PathBuf),
    /// `catalog/<kind>.json` next to the executable.
    BesideExe(PathBuf),
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog<E> {
    pub origin: Origin,
    pub catalog_version: Option<String>,
    pub entries: Vec<E>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError {
    pub origin: Origin,
    pub message: String,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.origin {
            Origin::Env(p) | Origin::BesideExe(p) => write!(f, "{}: {}", p.display(), self.message),
            Origin::Builtin => write!(f, "built-in sample: {}", self.message),
        }
    }
}

/// Looks up one catalog: environment variable (relative paths are taken
/// from the project root) → `catalog/<kind>.json` next to the executable →
/// built-in sample. The first hit is used even when it is invalid.
pub fn load<E: Entry>(
    root: &Path,
    env: Option<OsString>,
    exe_dir: Option<&Path>,
) -> Result<Catalog<E>, CatalogError> {
    let kind = E::KIND;
    let (origin, text) = if let Some(p) = env.filter(|v| !v.is_empty()) {
        let path = root.join(p);
        let text = fs::read_to_string(&path);
        (Origin::Env(path), text)
    } else if let Some(path) = exe_dir
        .map(|d| d.join("catalog").join(format!("{}.json", kind.name())))
        .filter(|p| p.is_file())
    {
        let text = fs::read_to_string(&path);
        (Origin::BesideExe(path), text)
    } else {
        (Origin::Builtin, Ok(kind.builtin().to_string()))
    };
    let text = text.map_err(|e| CatalogError {
        origin: origin.clone(),
        message: e.to_string(),
    })?;
    match parse::<E>(&text) {
        Ok(p) => Ok(Catalog {
            origin,
            catalog_version: p.catalog_version,
            entries: p.entries,
        }),
        Err(message) => Err(CatalogError { origin, message }),
    }
}

/// All three catalogs, each loaded independently.
#[derive(Debug, Clone)]
pub struct Catalogs {
    pub wiki: Result<Catalog<WikiEntry>, CatalogError>,
    pub skills: Result<Catalog<SkillsEntry>, CatalogError>,
    pub docs: Result<Catalog<DocsEntry>, CatalogError>,
}

impl Catalogs {
    /// Reads the process environment and the executable's directory.
    pub fn load(root: &Path) -> Self {
        let exe = std::env::current_exe().ok();
        let exe_dir = exe.as_deref().and_then(Path::parent);
        Self {
            wiki: load(root, std::env::var_os(Kind::Wiki.env_var()), exe_dir),
            skills: load(root, std::env::var_os(Kind::Skills.env_var()), exe_dir),
            docs: load(root, std::env::var_os(Kind::Docs.env_var()), exe_dir),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-catalog-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const UUID: &str = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";

    fn wiki(entries: &str) -> String {
        format!(r#"{{"schema_version":1,"kind":"wiki","entries":[{entries}]}}"#)
    }

    fn wiki_entry(extra: &str) -> String {
        format!(r#"{{"id":"a","uuid":"{UUID}","type":"skill","title":{{"en":"A"}}{extra}}}"#)
    }

    fn skills(entry: &str) -> Result<Parsed<SkillsEntry>, String> {
        parse(&format!(
            r#"{{"schema_version":1,"kind":"skills","entries":[{entry}]}}"#
        ))
    }

    fn docs(entry: &str) -> Result<Parsed<DocsEntry>, String> {
        parse(&format!(
            r#"{{"schema_version":1,"kind":"docs","entries":[{entry}]}}"#
        ))
    }

    #[test]
    fn builtin_samples_parse() {
        // CI check: the embedded samples follow the contract.
        let w = parse::<WikiEntry>(Kind::Wiki.builtin()).unwrap();
        assert!(w.entries.iter().any(|e| e.target == Some(Target::Skill)));
        let s = parse::<SkillsEntry>(Kind::Skills.builtin()).unwrap();
        assert!(s.entries.iter().any(|e| e.installer == SKILLS_CLI));
        assert!(s.entries.iter().any(|e| e.installer != SKILLS_CLI));
        parse::<DocsEntry>(Kind::Docs.builtin()).unwrap();
        assert!(w.catalog_version.is_some());
    }

    #[test]
    fn optional_fields_and_defaults() {
        let p = parse::<WikiEntry>(&wiki(&wiki_entry(""))).unwrap();
        assert_eq!(p.catalog_version, None, "catalog_version is optional");
        assert_eq!(p.entries[0].status, Status::Active);
        assert_eq!(p.entries[0].target, None);
        assert_eq!(p.entries[0].title.display(), "A");
        let p = parse::<WikiEntry>(&wiki(&wiki_entry(
            r#","target":"skill","status":"archived","updated":"2026-06-28T12:25:41.928+09:00","summary":{"ko":"요약"}"#,
        )))
        .unwrap();
        assert_eq!(p.entries[0].status, Status::Archived);
    }

    #[test]
    fn rejects_bad_envelopes() {
        for (text, why) in [
            (
                r#"{"schema_version":2,"kind":"wiki","entries":[]}"#,
                "unsupported",
            ),
            (
                r#"{"schema_version":"1","kind":"wiki","entries":[]}"#,
                "schema_version",
            ),
            (r#"{"kind":"wiki","entries":[]}"#, "schema_version"),
            (r#"{"schema_version":1,"kind":"docs","entries":[]}"#, "kind"),
            (
                r#"{"schema_version":1,"kind":"wiki","entries":[],"extra":1}"#,
                "extra",
            ),
            ("\u{feff}{}", "byte order mark"),
            ("{", "invalid JSON"),
        ] {
            let err = parse::<WikiEntry>(text).unwrap_err();
            assert!(err.contains(why), "{text}: {err}");
        }
    }

    #[test]
    fn rejects_bad_wiki_entries() {
        for (text, why) in [
            (wiki(&wiki_entry(r#","colour":"red""#)), "colour"),
            (
                wiki(&format!(
                    "{},{}",
                    wiki_entry(""),
                    wiki_entry("").replace("\"a\"", "\"A\"")
                )),
                "duplicate",
            ),
            (
                wiki(&wiki_entry(r#","target":"skill""#).replace("\"a\"", "\"a b\"")),
                "slug",
            ),
            (wiki(&wiki_entry("").replace(UUID, "nope")), "uuid"),
            (
                wiki(&wiki_entry("").replace("\"skill\"", "\"../x\"")),
                "type",
            ),
            (
                wiki(&wiki_entry("").replace(r#"{"en":"A"}"#, "{}")),
                "title",
            ),
            (
                wiki(&wiki_entry("").replace(r#"{"en":"A"}"#, r#"{"ja":"A"}"#)),
                "ja",
            ),
            (wiki(&wiki_entry(r#","updated":"yesterday""#)), "RFC 3339"),
            (wiki(&wiki_entry(r#","status":"gone""#)), "gone"),
            (wiki(&wiki_entry("").replace("\"a\"", "\"a\\u0007\"")), "id"),
        ] {
            let err = parse::<WikiEntry>(&text).unwrap_err();
            assert!(err.contains(why), "{text}: {err}");
        }
    }

    #[test]
    fn skills_entry_rules() {
        let cli = |extra: &str| {
            format!(
                r#"{{"id":"x","installer":"skills-cli","source":"owner/repo","title":{{"en":"x"}}{extra}}}"#
            )
        };
        skills(&cli("")).expect("skill is optional");
        skills(&cli(r#","skill":"*""#)).expect("`*` installs every skill");
        skills(&cli(r#","skill":"pdf""#)).unwrap();
        for (entry, why) in [
            (cli(r#","skill":"-g""#), "`skill`"),
            (cli(r#","skill":"a b""#), "`skill`"),
            (cli("").replace("owner/repo", "--yes"), "`source`"),
            (cli("").replace("owner/repo", "owner/repo x"), "`source`"),
            (cli(r#","install":"npx x""#), "must not have `install`"),
            (cli(r#","ref":"main""#), "ref"),
            (
                r#"{"id":"p","installer":"claude-code-cli","source":"o/r","title":{"en":"p"}}"#.into(),
                "needs `install`",
            ),
            (
                r#"{"id":"p","installer":"claude-code-cli","source":"o/r","skill":"p","install":"x","title":{"en":"p"}}"#.into(),
                "only `skills-cli`",
            ),
        ] {
            let err = skills(&entry).unwrap_err();
            assert!(err.contains(why), "{entry}: {err}");
        }
    }

    #[test]
    fn docs_entry_rules() {
        let doc = |url: &str, file: &str, extra: &str| {
            format!(r#"{{"id":"d","url":"{url}","filename":"{file}","title":{{"en":"d"}}{extra}}}"#)
        };
        let hash = "0e306d8fba8b7d173ab0e85ab2d40a93ec393cbc814b586025de5c6f6d5f9c1a";
        docs(&doc(
            "https://x.test/a.md",
            "a.md",
            &format!(r#","sha256":"{hash}""#),
        ))
        .unwrap();
        for (entry, why) in [
            (doc("http://x.test/a.md", "a.md", ""), "https"),
            (doc("https://", "a.md", ""), "https"),
            (doc("https://x.test/a.md", "dir/a.md", ""), "filename"),
            (doc("https://x.test/a.md", "..", ""), "filename"),
            (
                doc(
                    "https://x.test/a.md",
                    "a.md",
                    &format!(r#","sha256":"{}""#, hash.to_uppercase()),
                ),
                "sha256",
            ),
        ] {
            let err = docs(&entry).unwrap_err();
            assert!(err.contains(why), "{entry}: {err}");
        }
    }

    #[test]
    fn lookup_order_env_then_beside_exe_then_builtin() {
        let root = TempDir::new();
        let exe = TempDir::new();
        let custom =
            wiki(&wiki_entry("")).replace("\"wiki\",", "\"wiki\",\"catalog_version\":\"env\",");
        root.write("cat/wiki.json", &custom);
        let beside = exe.write(
            "catalog/wiki.json",
            &custom.replace("\"env\"", "\"beside\""),
        );

        // Relative environment path is taken from the project root.
        let c = load::<WikiEntry>(&root.0, Some("cat/wiki.json".into()), Some(&exe.0)).unwrap();
        assert_eq!(c.origin, Origin::Env(root.0.join("cat/wiki.json")));
        assert_eq!(c.catalog_version.as_deref(), Some("env"));

        let c = load::<WikiEntry>(&root.0, Some("".into()), Some(&exe.0)).unwrap();
        assert_eq!(c.origin, Origin::BesideExe(beside));
        assert_eq!(c.catalog_version.as_deref(), Some("beside"));

        let c = load::<WikiEntry>(&root.0, None, Some(&root.0)).unwrap();
        assert_eq!(c.origin, Origin::Builtin);
        let c = load::<SkillsEntry>(&root.0, None, None).unwrap();
        assert_eq!(c.origin, Origin::Builtin);
    }

    #[test]
    fn broken_catalog_is_reported_not_replaced() {
        let root = TempDir::new();
        let exe = TempDir::new();
        exe.write("catalog/wiki.json", &wiki(&wiki_entry("")));
        root.write(
            "bad.json",
            r#"{"schema_version":1,"kind":"docs","entries":[]}"#,
        );
        let err = load::<WikiEntry>(&root.0, Some("bad.json".into()), Some(&exe.0)).unwrap_err();
        assert_eq!(err.origin, Origin::Env(root.0.join("bad.json")));
        assert!(err.to_string().contains("kind"), "{err}");
        // A missing file named by the variable is an error too.
        let err =
            load::<WikiEntry>(&root.0, Some("missing.json".into()), Some(&exe.0)).unwrap_err();
        assert!(matches!(err.origin, Origin::Env(_)));
        // A broken file next to the executable is not skipped either.
        exe.write("catalog/docs.json", "{");
        let err = load::<DocsEntry>(&root.0, None, Some(&exe.0)).unwrap_err();
        assert!(matches!(err.origin, Origin::BesideExe(_)));
    }

    #[test]
    fn rfc3339_shapes() {
        for ok in [
            "2026-06-28T12:25:41Z",
            "2026-06-28T12:25:41.928Z",
            "2026-06-28t12:25:41+09:00",
        ] {
            assert!(is_rfc3339(ok), "{ok}");
        }
        for bad in [
            "2026-06-28",
            "2026-06-28 12:25:41Z",
            "2026-06-28T12:25:41",
            "2026-06-28T12:25:41.Z",
            "2026-06-28T12:25:41+0900",
        ] {
            assert!(!is_rfc3339(bad), "{bad}");
        }
    }
}
