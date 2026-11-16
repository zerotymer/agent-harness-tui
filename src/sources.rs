//! Skill sources and import (Stage 1 spec §4, instruction 0007).
//!
//! Two sources share one boundary: a local directory, and the read-only
//! `md.zerotymer.net` wiki gateway (`wiki-skill-import` contract,
//! `GET /pages/by-uuid`). The gateway has no documented catalog API yet, so
//! its list/search is a labeled dummy that never touches the network.
//! Fetching happens only on an explicit request; nothing is cached. Imported
//! content is stored verbatim (frontmatter is not interpreted) and bundled
//! files are copied, never executed.

use crate::apply::{Desired, sha256};
use crate::contract::{HarnessLock, LockedFile, LockedSkill, WIKI_GATEWAY_URL, is_slug, is_uuid};
use crate::paths::{Layout, ProjectRoot, RelPath};
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const GATEWAY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Page UUID (gateway) or directory name (local).
    pub reference: String,
    pub title: String,
    pub page_type: Option<String>,
    /// Placeholder entry, not fetched from a real catalog.
    pub dummy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    /// Relative to the skill directory.
    pub path: RelPath,
    pub bytes: Vec<u8>,
    pub locale: Option<String>,
    /// Revision reported by the source, verbatim.
    pub revision: Option<String>,
}

/// A fetched skill, ready to preview and import.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    pub title: Option<String>,
    pub files: Vec<PackageFile>,
    /// Locales the source did not provide (or provided empty).
    pub missing_locales: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    InvalidReference(String),
    /// The environment variable that should hold the token is unset.
    MissingToken(String),
    Unreachable {
        url: String,
        uuid: String,
        detail: String,
    },
    Timeout {
        url: String,
        uuid: String,
    },
    /// 401 or 403.
    Auth(u16),
    /// `PAGE_NOT_FOUND`: no locale exists for the UUID.
    NotFound(String),
    Http(u16),
    InvalidResponse(String),
    Io(String),
    /// A package path or symlink leaves the package directory.
    PathEscape(String),
    NotAPackage(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReference(r) => write!(f, "invalid reference `{r}`"),
            Self::MissingToken(var) => {
                write!(
                    f,
                    "environment variable `{var}` is not set; the gateway needs a token"
                )
            }
            Self::Unreachable { url, uuid, detail } => {
                write!(f, "gateway {url} unreachable for {uuid}: {detail}")
            }
            Self::Timeout { url, uuid } => write!(f, "gateway {url} timed out for {uuid}"),
            Self::Auth(code) => write!(f, "gateway refused the token (HTTP {code})"),
            Self::NotFound(uuid) => write!(f, "PAGE_NOT_FOUND: {uuid}"),
            Self::Http(code) => write!(f, "gateway returned HTTP {code}"),
            Self::InvalidResponse(why) => write!(f, "unexpected gateway response: {why}"),
            Self::Io(why) => write!(f, "{why}"),
            Self::PathEscape(p) => write!(f, "`{p}` leaves the package directory"),
            Self::NotAPackage(why) => write!(f, "not a skill package: {why}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// Source boundary: list, search, and fetch (fetch doubles as preview).
pub trait SkillSource {
    fn list(&self) -> Result<Vec<CatalogEntry>, SourceError>;

    fn search(&self, query: &str) -> Result<Vec<CatalogEntry>, SourceError> {
        let q = query.to_lowercase();
        Ok(self
            .list()?
            .into_iter()
            .filter(|e| e.title.to_lowercase().contains(&q) || e.reference.contains(&q))
            .collect())
    }

    fn fetch(&self, reference: &str, page_type: Option<&str>) -> Result<Package, SourceError>;
}

// ------------------------------------------------------------------- local dir

/// A directory whose subdirectories with a `SKILL.md` are skill packages.
/// The path may be outside the project for a one-time import; only
/// project-relative paths can be saved in `harness.toml`.
#[derive(Debug, Clone)]
pub struct LocalDir {
    pub dir: PathBuf,
}

impl SkillSource for LocalDir {
    fn list(&self) -> Result<Vec<CatalogEntry>, SourceError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(io_err(&self.dir))? {
            let entry = entry.map_err(io_err(&self.dir))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().join("SKILL.md").is_file() {
                entries.push(CatalogEntry {
                    reference: name.clone(),
                    title: name,
                    page_type: None,
                    dummy: false,
                });
            }
        }
        entries.sort_by(|a, b| a.reference.cmp(&b.reference));
        Ok(entries)
    }

    fn fetch(&self, reference: &str, _page_type: Option<&str>) -> Result<Package, SourceError> {
        let rel = RelPath::parse(reference)
            .map_err(|_| SourceError::InvalidReference(reference.into()))?;
        let base = self.dir.canonicalize().map_err(io_err(&self.dir))?;
        let pkg_dir = base.join(rel.as_str());
        let pkg = pkg_dir
            .canonicalize()
            .map_err(|_| SourceError::NotAPackage(format!("`{reference}` does not exist")))?;
        if !pkg.starts_with(&base) {
            return Err(SourceError::PathEscape(reference.into()));
        }
        if !pkg.join("SKILL.md").is_file() {
            return Err(SourceError::NotAPackage(format!(
                "`{reference}` has no SKILL.md"
            )));
        }
        let mut files = Vec::new();
        walk(&pkg, &pkg, "", &mut files)?;
        Ok(Package {
            title: Some(rel.file_name().to_string()),
            files,
            ..Package::default()
        })
    }
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> SourceError + '_ {
    move |e| SourceError::Io(format!("{}: {e}", path.display()))
}

/// Collects regular files under `dir`. Symlinks must resolve to a file
/// inside the package; symlinked directories are refused.
fn walk(
    pkg: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<PackageFile>,
) -> Result<(), SourceError> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(io_err(dir))?
        .collect::<Result<_, _>>()
        .map_err(io_err(dir))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(SourceError::NotAPackage(format!(
                "non UTF-8 file name in `{prefix}`"
            )));
        };
        let rel_str = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(io_err(&path))?;
        let real = if meta.file_type().is_symlink() {
            let target = path
                .canonicalize()
                .map_err(|_| SourceError::PathEscape(rel_str.clone()))?;
            if !target.starts_with(pkg) {
                return Err(SourceError::PathEscape(rel_str));
            }
            if target.is_dir() {
                return Err(SourceError::NotAPackage(format!(
                    "symlinked directory `{rel_str}`"
                )));
            }
            target
        } else if meta.is_dir() {
            walk(pkg, &path, &rel_str, out)?;
            continue;
        } else {
            path
        };
        let rel = RelPath::parse(&rel_str).map_err(|_| SourceError::PathEscape(rel_str.clone()))?;
        // ponytail: file modes (e.g. the executable bit) are not carried
        // over; scripts are data here and are never run during import.
        let bytes = fs::read(&real).map_err(io_err(&real))?;
        out.push(PackageFile {
            path: rel,
            bytes,
            locale: None,
            revision: None,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- wiki gateway

/// Read-only wiki gateway client (REST interface of `wiki-skill-import`).
#[derive(Debug, Clone)]
pub struct WikiGateway {
    pub base_url: String,
    /// Name of the environment variable holding the token. The token
    /// itself is read per request and never stored.
    pub token_env: String,
    pub timeout: Duration,
}

impl WikiGateway {
    pub fn new(token_env: impl Into<String>) -> Self {
        Self {
            base_url: WIKI_GATEWAY_URL.into(),
            token_env: token_env.into(),
            timeout: GATEWAY_TIMEOUT,
        }
    }

    fn fetch_with_token(
        &self,
        token: &str,
        uuid: &str,
        page_type: Option<&str>,
    ) -> Result<Package, SourceError> {
        if !is_uuid(uuid) {
            return Err(SourceError::InvalidReference(uuid.into()));
        }
        let mut url = format!("{}/pages/by-uuid?uuid={uuid}", self.base_url);
        if let Some(t) = page_type {
            if !is_slug(t) {
                return Err(SourceError::InvalidReference(t.into()));
            }
            url.push_str(&format!("&type={t}"));
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build()
            .into();
        let body = agent
            .get(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .call()
            .and_then(|mut r| r.body_mut().read_to_string());
        match body {
            Ok(text) => parse_gateway(&text, uuid),
            Err(ureq::Error::StatusCode(code @ (401 | 403))) => Err(SourceError::Auth(code)),
            Err(ureq::Error::StatusCode(404)) => Err(SourceError::NotFound(uuid.into())),
            Err(ureq::Error::StatusCode(code)) => Err(SourceError::Http(code)),
            Err(ureq::Error::Timeout(_)) => Err(SourceError::Timeout {
                url: self.base_url.clone(),
                uuid: uuid.into(),
            }),
            Err(e) => Err(SourceError::Unreachable {
                url: self.base_url.clone(),
                uuid: uuid.into(),
                detail: e.to_string(),
            }),
        }
    }
}

/// Real wiki skills used as sample entries until a catalog API exists.
const DUMMY_CATALOG: &[(&str, &str)] = &[
    ("14cea96f-f1e2-461e-b9a7-96b5a6f63b67", "wiki-skill-import"),
    (
        "e03f48fb-3e00-41d7-b99d-c32854567d67",
        "git-branch-guideline",
    ),
    (
        "5c502e34-1daf-44d7-b740-1baa03e583e5",
        "git-worktree-guideline",
    ),
    ("b3230cd0-6081-4570-9bda-183543f585e9", "project-session"),
    (
        "366d5d35-7b46-4d5c-b5dd-3e53d48d7492",
        "instruction-operating-rules",
    ),
];

impl SkillSource for WikiGateway {
    /// Dummy: fixed entries, no network (no documented catalog contract).
    fn list(&self) -> Result<Vec<CatalogEntry>, SourceError> {
        Ok(DUMMY_CATALOG
            .iter()
            .map(|(uuid, title)| CatalogEntry {
                reference: (*uuid).into(),
                title: (*title).into(),
                page_type: Some("skill".into()),
                dummy: true,
            })
            .collect())
    }

    fn fetch(&self, uuid: &str, page_type: Option<&str>) -> Result<Package, SourceError> {
        let token = std::env::var(&self.token_env)
            .ok()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| SourceError::MissingToken(self.token_env.clone()))?;
        self.fetch_with_token(&token, uuid, page_type)
    }
}

#[derive(Deserialize)]
struct GatewayResponse {
    pages: GatewayPages,
}

#[derive(Deserialize)]
struct GatewayPages {
    en: Option<GatewayPage>,
    ko: Option<GatewayPage>,
}

#[derive(Deserialize)]
struct GatewayPage {
    #[serde(default)]
    title: Option<String>,
    content: String,
    #[serde(rename = "contentHash", default)]
    content_hash: Option<String>,
}

/// Maps a gateway body to a package: `en` → `SKILL.md`, `ko` → `SKILL_ko.md`.
fn parse_gateway(body: &str, uuid: &str) -> Result<Package, SourceError> {
    let resp: GatewayResponse = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) if body.contains("PAGE_NOT_FOUND") => {
            return Err(SourceError::NotFound(uuid.into()));
        }
        Err(e) => return Err(SourceError::InvalidResponse(e.to_string())),
    };
    let mut pkg = Package::default();
    for (locale, file, page) in [
        ("en", "SKILL.md", resp.pages.en),
        ("ko", "SKILL_ko.md", resp.pages.ko),
    ] {
        let Some(page) = page else {
            pkg.missing_locales.push(locale.into());
            continue;
        };
        if pkg.title.is_none() {
            pkg.title = page.title.clone();
        }
        if page.content.trim().is_empty() {
            pkg.warnings.push(format!(
                "`{locale}` content is empty; `{file}` will not be written"
            ));
            pkg.missing_locales.push(locale.into());
            continue;
        }
        pkg.files.push(PackageFile {
            path: RelPath::parse(file).expect("valid constant"),
            bytes: page.content.into_bytes(),
            locale: Some(locale.into()),
            revision: page.content_hash,
        });
    }
    if pkg.files.is_empty() {
        return Err(SourceError::NotFound(uuid.into()));
    }
    Ok(pkg)
}

// --------------------------------------------------------------------- import

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRequest<'a> {
    /// Directory name under `.skills/`, chosen by the user.
    pub skill_id: &'a str,
    pub source_id: &'a str,
    pub reference: &'a str,
    pub page_type: Option<&'a str>,
    /// Required when the source lacks a locale.
    pub accept_partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    InvalidId(String),
    /// Refresh is a Stage 2 update, not a re-import.
    AlreadyInstalled(String),
    /// `.skills/<id>/` exists but is not a skill this tool imported.
    DestinationExists(String),
    PartialNotAccepted(Vec<String>),
    Empty,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "skill name `{id}` must be a slug"),
            Self::AlreadyInstalled(id) => write!(f, "`{id}` is already installed"),
            Self::DestinationExists(p) => write!(f, "`{p}` already exists and is not managed"),
            Self::PartialNotAccepted(l) => {
                write!(
                    f,
                    "missing locale(s) {}; accept a partial import to continue",
                    l.join(", ")
                )
            }
            Self::Empty => write!(f, "the package has no files"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Default skill name from a title: lowercase ASCII slug, or `skill`.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "skill".into() } else { out }
}

/// Plans the files for `.skills/<id>/` and the lock entry. The caller adds
/// the updated lock (see `lock_with`) to the same apply.
pub fn import_items(
    root: &ProjectRoot,
    lock: &HarnessLock,
    req: &ImportRequest<'_>,
    pkg: &Package,
) -> Result<(Vec<Desired>, LockedSkill), ImportError> {
    if !is_slug(req.skill_id) {
        return Err(ImportError::InvalidId(req.skill_id.into()));
    }
    if lock
        .skills
        .iter()
        .any(|s| s.id.eq_ignore_ascii_case(req.skill_id))
    {
        return Err(ImportError::AlreadyInstalled(req.skill_id.into()));
    }
    let dest = Layout::skills().join(req.skill_id);
    if root.abs(&dest).exists() {
        return Err(ImportError::DestinationExists(dest.to_string()));
    }
    if !pkg.missing_locales.is_empty() && !req.accept_partial {
        return Err(ImportError::PartialNotAccepted(pkg.missing_locales.clone()));
    }
    if pkg.files.is_empty() {
        return Err(ImportError::Empty);
    }
    let desired = pkg
        .files
        .iter()
        .map(|f| Desired::file(dest.join(f.path.as_str()), f.bytes.clone()))
        .collect();
    let locked = LockedSkill {
        id: req.skill_id.into(),
        source: req.source_id.into(),
        reference: req.reference.into(),
        page_type: req.page_type.map(str::to_string),
        files: pkg
            .files
            .iter()
            .map(|f| LockedFile {
                path: f.path.clone(),
                hash: sha256(&f.bytes),
                locale: f.locale.clone(),
                revision: f.revision.clone(),
            })
            .collect(),
    };
    Ok((desired, locked))
}

/// `lock` plus `skill`.
pub fn lock_with(lock: &HarnessLock, skill: LockedSkill) -> HarnessLock {
    let mut next = lock.clone();
    next.skills.push(skill);
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply, plan};
    use crate::contract::{ContractFile, HarnessState, LOCK_FILE};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-sources-{}-{}",
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
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const TOKEN: &str = "test-token-9f8e7d";
    const UUID: &str = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";

    fn body(en: Option<&str>, ko: Option<&str>) -> String {
        let page = |c: &str| {
            format!(
                r#"{{"id":1,"path":"skill/x","locale":"l","title":"Wiki Skill Import","content":{c:?},"tags":[],"contentHash":"sha256:abc"}}"#
            )
        };
        format!(
            r#"{{"uuid":"{UUID}","type":"skill","pages":{{"en":{},"ko":{}}}}}"#,
            en.map(page).unwrap_or("null".into()),
            ko.map(page).unwrap_or("null".into())
        )
    }

    /// Serves one canned HTTP response and returns the raw request.
    fn serve_once(status: u16, body: String) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let mut req = Vec::new();
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
            }
            let resp = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });
        (url, handle)
    }

    fn gateway(url: &str) -> WikiGateway {
        WikiGateway {
            base_url: url.into(),
            token_env: "AHT_TEST_UNSET_TOKEN_VAR".into(),
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn gateway_request_shape_and_both_locales() {
        let (url, req) = serve_once(200, body(Some("# en\n"), Some("# ko\n")));
        let pkg = gateway(&url)
            .fetch_with_token(TOKEN, UUID, Some("skill"))
            .unwrap();
        let req = req.join().unwrap();
        assert!(req.starts_with(&format!(
            "GET /pages/by-uuid?uuid={UUID}&type=skill HTTP/1.1"
        )));
        assert!(
            req.contains(&format!("authorization: Bearer {TOKEN}"))
                || req.contains(&format!("Authorization: Bearer {TOKEN}"))
        );
        let names: Vec<_> = pkg.files.iter().map(|f| f.path.to_string()).collect();
        assert_eq!(names, ["SKILL.md", "SKILL_ko.md"]);
        assert_eq!(pkg.files[0].bytes, b"# en\n", "stored verbatim");
        assert_eq!(pkg.files[1].revision.as_deref(), Some("sha256:abc"));
        assert_eq!(pkg.title.as_deref(), Some("Wiki Skill Import"));
        assert!(pkg.missing_locales.is_empty());
    }

    #[test]
    fn gateway_without_type_omits_parameter() {
        let (url, req) = serve_once(200, body(Some("x"), None));
        gateway(&url).fetch_with_token(TOKEN, UUID, None).unwrap();
        assert!(
            req.join()
                .unwrap()
                .starts_with(&format!("GET /pages/by-uuid?uuid={UUID} "))
        );
    }

    #[test]
    fn gateway_status_errors() {
        for (status, expect) in [
            (401, SourceError::Auth(401)),
            (403, SourceError::Auth(403)),
            (404, SourceError::NotFound(UUID.into())),
            (500, SourceError::Http(500)),
        ] {
            let (url, _req) = serve_once(status, r#"{"error":"x"}"#.into());
            assert_eq!(
                gateway(&url).fetch_with_token(TOKEN, UUID, None),
                Err(expect)
            );
        }
    }

    #[test]
    fn gateway_timeout_and_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hold = thread::spawn(move || {
            let (s, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(800));
            drop(s);
        });
        let mut g = gateway(&url);
        g.timeout = Duration::from_millis(200);
        assert!(matches!(
            g.fetch_with_token(TOKEN, UUID, None),
            Err(SourceError::Timeout { .. })
        ));
        hold.join().unwrap();

        // Nothing listens on this port any more.
        let err = gateway(&url)
            .fetch_with_token(TOKEN, UUID, None)
            .unwrap_err();
        assert!(
            matches!(
                err,
                SourceError::Unreachable { .. } | SourceError::Timeout { .. }
            ),
            "{err:?}"
        );
        assert!(
            !format!("{err} {err:?}").contains(TOKEN),
            "token never in errors"
        );
    }

    #[test]
    fn gateway_requires_token_variable() {
        let err = gateway("http://127.0.0.1:9").fetch(UUID, None).unwrap_err();
        assert_eq!(
            err,
            SourceError::MissingToken("AHT_TEST_UNSET_TOKEN_VAR".into())
        );
        assert!(err.to_string().contains("AHT_TEST_UNSET_TOKEN_VAR"));
        assert!(matches!(
            gateway("http://127.0.0.1:9").fetch_with_token(TOKEN, "not-a-uuid", None),
            Err(SourceError::InvalidReference(_))
        ));
    }

    #[test]
    fn gateway_body_mapping() {
        let one = parse_gateway(&body(Some("# en"), None), UUID).unwrap();
        assert_eq!(one.missing_locales, ["ko"]);
        assert_eq!(one.files.len(), 1);
        let empty = parse_gateway(&body(Some("  \n"), Some("# ko")), UUID).unwrap();
        assert_eq!(empty.missing_locales, ["en"]);
        assert_eq!(empty.warnings.len(), 1);
        assert_eq!(empty.files[0].path.as_str(), "SKILL_ko.md");
        assert_eq!(
            parse_gateway(&body(None, None), UUID),
            Err(SourceError::NotFound(UUID.into()))
        );
        assert_eq!(
            parse_gateway(r#"{"error":"PAGE_NOT_FOUND"}"#, UUID),
            Err(SourceError::NotFound(UUID.into()))
        );
        assert!(matches!(
            parse_gateway("<html>", UUID),
            Err(SourceError::InvalidResponse(_))
        ));
    }

    #[test]
    fn dummy_catalog_uses_no_network() {
        // An unroutable base URL proves list/search never connect.
        let g = gateway("http://127.0.0.1:9");
        let all = g.list().unwrap();
        assert_eq!(all.len(), DUMMY_CATALOG.len());
        assert!(all.iter().all(|e| e.dummy && is_uuid(&e.reference)));
        let found = g.search("worktree").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "git-worktree-guideline");
    }

    #[test]
    fn local_list_and_fetch() {
        let src = TempDir::new();
        src.write("review/SKILL.md", "# Review\n");
        src.write(
            "review/scripts/run.sh",
            "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n",
        );
        src.write("review/templates/a.md", "tpl\n");
        src.write("notes/readme.md", "not a skill\n");
        let local = LocalDir { dir: src.0.clone() };
        let list = local.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].reference, "review");
        let pkg = local.fetch("review", None).unwrap();
        let names: Vec<_> = pkg.files.iter().map(|f| f.path.to_string()).collect();
        assert_eq!(names, ["SKILL.md", "scripts/run.sh", "templates/a.md"]);
        assert!(matches!(
            local.fetch("notes", None),
            Err(SourceError::NotAPackage(_))
        ));
        assert!(matches!(
            local.fetch("../x", None),
            Err(SourceError::InvalidReference(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn local_rejects_escaping_symlinks() {
        use std::os::unix::fs::symlink;
        let src = TempDir::new();
        let outside = TempDir::new();
        outside.write("secret.txt", "secret");
        src.write("a/SKILL.md", "# A\n");
        symlink(outside.0.join("secret.txt"), src.0.join("a/leak.txt")).unwrap();
        src.write("b/SKILL.md", "# B\n");
        symlink(src.0.join("b/missing"), src.0.join("b/dangling")).unwrap();
        src.write("c/SKILL.md", "# C\n");
        symlink(src.0.join("c/SKILL.md"), src.0.join("c/alias.md")).unwrap();
        symlink(&outside.0, src.0.join("d")).unwrap();
        let local = LocalDir { dir: src.0.clone() };
        assert_eq!(
            local.fetch("a", None),
            Err(SourceError::PathEscape("leak.txt".into()))
        );
        assert_eq!(
            local.fetch("b", None),
            Err(SourceError::PathEscape("dangling".into()))
        );
        assert_eq!(local.fetch("c", None).unwrap().files.len(), 2);
        assert!(matches!(
            local.fetch("d", None),
            Err(SourceError::PathEscape(_)) | Err(SourceError::NotAPackage(_))
        ));
    }

    #[test]
    fn import_plans_files_and_lock_without_running_scripts() {
        let src = TempDir::new();
        src.write("review/SKILL.md", "# Review\n");
        src.write(
            "review/scripts/run.sh",
            "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n",
        );
        let pkg = LocalDir { dir: src.0.clone() }
            .fetch("review", None)
            .unwrap();

        let project = TempDir::new();
        let root = ProjectRoot::new(&project.0).unwrap();
        let lock = HarnessLock::default();
        let req = ImportRequest {
            skill_id: "review",
            source_id: "vendored",
            reference: "review",
            page_type: None,
            accept_partial: false,
        };
        let (mut desired, locked) = import_items(&root, &lock, &req, &pkg).unwrap();
        assert_eq!(locked.files[0].hash, sha256(b"# Review\n"));
        let next = lock_with(&lock, locked);
        desired.push(Desired::file(
            RelPath::parse(LOCK_FILE).unwrap(),
            next.to_text().unwrap(),
        ));
        let mut state = HarnessState::default();
        let pl = plan(&root, &state, &[], desired).unwrap();
        apply(&root, &pl, &mut state).unwrap();

        assert_eq!(
            fs::read_to_string(project.0.join(".skills/review/scripts/run.sh")).unwrap(),
            "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n"
        );
        assert!(!project.0.join("SHOULD_NOT_EXIST").exists());
        assert!(!src.0.join("review/SHOULD_NOT_EXIST").exists());
        assert_eq!(HarnessLock::load(&project.0).unwrap().unwrap(), next);

        // Same skill again: refused (refresh is Stage 2).
        assert_eq!(
            import_items(&root, &next, &req, &pkg),
            Err(ImportError::AlreadyInstalled("review".into()))
        );
    }

    #[test]
    fn import_refuses_unmanaged_destination_and_partial() {
        let project = TempDir::new();
        project.write(".skills/mine/SKILL.md", "user's own skill\n");
        let root = ProjectRoot::new(&project.0).unwrap();
        let pkg = parse_gateway(&body(Some("# en"), None), UUID).unwrap();
        let mut req = ImportRequest {
            skill_id: "mine",
            source_id: "wiki",
            reference: UUID,
            page_type: Some("skill"),
            accept_partial: false,
        };
        let lock = HarnessLock::default();
        assert!(matches!(
            import_items(&root, &lock, &req, &pkg),
            Err(ImportError::DestinationExists(_))
        ));
        req.skill_id = "wiki-skill-import";
        assert_eq!(
            import_items(&root, &lock, &req, &pkg),
            Err(ImportError::PartialNotAccepted(vec!["ko".into()]))
        );
        req.accept_partial = true;
        let (desired, locked) = import_items(&root, &lock, &req, &pkg).unwrap();
        assert_eq!(desired.len(), 1, "no fabricated translation");
        assert_eq!(locked.files[0].locale.as_deref(), Some("en"));
        req.skill_id = "../x";
        assert!(matches!(
            import_items(&root, &lock, &req, &pkg),
            Err(ImportError::InvalidId(_))
        ));
    }

    #[test]
    fn slugify_titles() {
        assert_eq!(slugify("Wiki Skill Import"), "wiki-skill-import");
        assert_eq!(
            slugify("  Git: Branch / Guideline! "),
            "git-branch-guideline"
        );
        assert_eq!(slugify("한글 제목"), "skill");
    }
}
