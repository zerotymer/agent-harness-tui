//! Versioned shared config, lock and local state (Stage 1 spec §3,
//! instruction 0003). See `references/HARNESS_STATE_CONTRACT.md`.
//!
//! This module only parses, validates and serializes. Writing files with
//! backups and recovery is the apply engine's job (instruction 0004); it must
//! load first, so a file with an unsupported version is never overwritten.

use crate::paths::{BACKLOG_FILE, RelPath};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::Path;

pub const SCHEMA_VERSION: u32 = 1;
pub const CONFIG_FILE: &str = "harness.toml";
pub const LOCK_FILE: &str = "harness.lock.json";
pub const STATE_FILE: &str = ".harness/state.json";
pub const BACKUPS_DIR: &str = ".harness/backups";
/// The only wiki gateway (`wiki-skill-import` contract); not configurable.
pub const WIKI_GATEWAY_URL: &str = "https://md-gw.zerotymer.net";
pub const DEFAULT_TOKEN_ENV: &str = "GATEWAY_TOKEN";

#[derive(Debug)]
pub enum ContractError {
    Io(io::Error),
    Parse {
        file: &'static str,
        message: String,
    },
    UnsupportedVersion {
        file: &'static str,
        found: u32,
    },
    Invalid {
        file: &'static str,
        problems: Vec<String>,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse { file, message } => write!(f, "{file}: {message}"),
            Self::UnsupportedVersion { file, found } => write!(
                f,
                "{file}: schema_version {found} is not supported (expected {SCHEMA_VERSION}); \
                 the file is left unchanged"
            ),
            Self::Invalid { file, problems } => write!(f, "{file}: {}", problems.join("; ")),
        }
    }
}

impl std::error::Error for ContractError {}

impl From<io::Error> for ContractError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// A versioned contract file.
pub trait ContractFile: Serialize + DeserializeOwned {
    const FILE: &'static str;
    fn problems(&self) -> Vec<String>;
    fn parse_text(text: &str) -> Result<Self, String>;
    fn probe_version(text: &str) -> Result<u32, String>;
    fn render(&self) -> Result<String, String>;

    /// Parses, checks the version first, then validates.
    fn from_text(text: &str) -> Result<Self, ContractError> {
        let parse_err = |message| ContractError::Parse {
            file: Self::FILE,
            message,
        };
        let found = Self::probe_version(text).map_err(parse_err)?;
        if found != SCHEMA_VERSION {
            return Err(ContractError::UnsupportedVersion {
                file: Self::FILE,
                found,
            });
        }
        let value = Self::parse_text(text).map_err(parse_err)?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ContractError> {
        let problems = self.problems();
        if problems.is_empty() {
            Ok(())
        } else {
            Err(ContractError::Invalid {
                file: Self::FILE,
                problems,
            })
        }
    }

    /// Validates, then serializes.
    fn to_text(&self) -> Result<String, ContractError> {
        self.validate()?;
        self.render().map_err(|message| ContractError::Parse {
            file: Self::FILE,
            message,
        })
    }

    /// `Ok(None)` when the file does not exist.
    fn load(root: &Path) -> Result<Option<Self>, ContractError> {
        match std::fs::read_to_string(root.join(Self::FILE)) {
            Ok(text) => Self::from_text(&text).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

// ---------------------------------------------------------------- harness.toml

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentId {
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    Copy,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    LocalDir,
    WikiGateway,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathsConfig {
    /// Absent in configs written before it became configurable.
    #[serde(default = "default_instructions")]
    pub instructions: RelPath,
    pub references: RelPath,
    pub output: RelPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BacklogConfig {
    pub enabled: bool,
    pub path: RelPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub id: AgentId,
    pub placement: Placement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub id: String,
    pub kind: SourceKind,
    /// `local-dir` only: project-relative directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<RelPath>,
    /// `wiki-gateway` only: name of the environment variable, never a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillConfig {
    /// Stable ID; also the directory name under `.skills/`.
    pub id: String,
    pub source: String,
    /// Wiki page UUID, or a directory relative to a local source.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Wiki path prefix such as `skill`; only for `wiki-gateway`.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessConfig {
    pub schema_version: u32,
    pub paths: PathsConfig,
    pub backlog: BacklogConfig,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub skills: Vec<SkillConfig>,
}

impl HarnessConfig {
    pub fn layout(&self) -> crate::paths::Layout {
        crate::paths::Layout {
            instructions: self.paths.instructions.clone(),
            references: self.paths.references.clone(),
            output: self.paths.output.clone(),
            backlog_enabled: self.backlog.enabled,
        }
    }
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let layout = crate::paths::Layout::default();
        Self {
            schema_version: SCHEMA_VERSION,
            paths: PathsConfig {
                instructions: layout.instructions,
                references: layout.references,
                output: layout.output,
            },
            backlog: BacklogConfig {
                enabled: false,
                path: crate::paths::Layout::backlog(),
            },
            agents: Vec::new(),
            sources: Vec::new(),
            skills: Vec::new(),
        }
    }
}

fn default_instructions() -> RelPath {
    crate::paths::Layout::default().instructions
}

impl ContractFile for HarnessConfig {
    const FILE: &'static str = CONFIG_FILE;

    fn parse_text(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }
    fn probe_version(text: &str) -> Result<u32, String> {
        toml::from_str::<VersionProbe>(text)
            .map(|p| p.schema_version)
            .map_err(|e| e.to_string())
    }
    fn render(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if self.backlog.path.as_str() != BACKLOG_FILE {
            p.push(format!(
                "backlog.path is fixed to `{BACKLOG_FILE}` in Stage 1, got `{}`",
                self.backlog.path
            ));
        }
        unique(
            &mut p,
            "agent",
            self.agents.iter().map(|a| format!("{:?}", a.id)),
        );
        unique(&mut p, "source", self.sources.iter().map(|s| s.id.clone()));
        unique(&mut p, "skill", self.skills.iter().map(|s| s.id.clone()));

        for s in &self.sources {
            if !is_slug(&s.id) {
                p.push(format!("source id `{}` must be a slug", s.id));
            }
            match s.kind {
                SourceKind::LocalDir => {
                    if s.path.is_none() {
                        p.push(format!("source `{}`: local-dir needs `path`", s.id));
                    }
                    if s.token_env.is_some() {
                        p.push(format!("source `{}`: local-dir takes no `token_env`", s.id));
                    }
                }
                SourceKind::WikiGateway => {
                    if s.path.is_some() {
                        p.push(format!("source `{}`: wiki-gateway takes no `path`", s.id));
                    }
                    match &s.token_env {
                        Some(name) if is_env_name(name) => {}
                        Some(name) => p.push(format!(
                            "source `{}`: token_env `{name}` must be an environment variable \
                             name (A-Z, 0-9, _), never a token value",
                            s.id
                        )),
                        None => {
                            p.push(format!("source `{}`: wiki-gateway needs `token_env`", s.id))
                        }
                    }
                }
            }
        }

        for k in &self.skills {
            if !is_slug(&k.id) {
                p.push(format!("skill id `{}` must be a slug", k.id));
            }
            let Some(src) = self.sources.iter().find(|s| s.id == k.source) else {
                p.push(format!("skill `{}`: unknown source `{}`", k.id, k.source));
                continue;
            };
            match src.kind {
                SourceKind::WikiGateway => {
                    if !is_uuid(&k.reference) {
                        p.push(format!("skill `{}`: ref must be a page UUID", k.id));
                    }
                    if k.page_type.as_deref().is_some_and(|t| !is_slug(t)) {
                        p.push(format!("skill `{}`: type must be a slug", k.id));
                    }
                }
                SourceKind::LocalDir => {
                    if let Err(e) = RelPath::parse(&k.reference) {
                        p.push(format!("skill `{}`: ref {e}", k.id));
                    }
                    if k.page_type.is_some() {
                        p.push(format!("skill `{}`: type is only for wiki-gateway", k.id));
                    }
                }
            }
        }
        p
    }
}

// ----------------------------------------------------------- harness.lock.json

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedFile {
    /// Relative to `.skills/<id>/`.
    pub path: RelPath,
    /// `sha256:<hex>` of the bytes written locally.
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Revision reported by the source (gateway `contentHash`), verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSkill {
    pub id: String,
    pub source: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<String>,
    pub files: Vec<LockedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessLock {
    pub schema_version: u32,
    #[serde(default)]
    pub skills: Vec<LockedSkill>,
}

impl Default for HarnessLock {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            skills: Vec::new(),
        }
    }
}

impl ContractFile for HarnessLock {
    const FILE: &'static str = LOCK_FILE;

    fn parse_text(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
    fn probe_version(text: &str) -> Result<u32, String> {
        json_version(text)
    }
    fn render(&self) -> Result<String, String> {
        json_render(self)
    }

    fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        unique(&mut p, "skill", self.skills.iter().map(|s| s.id.clone()));
        for s in &self.skills {
            if !is_slug(&s.id) {
                p.push(format!("skill id `{}` must be a slug", s.id));
            }
            if s.files.is_empty() {
                p.push(format!("skill `{}` lists no files", s.id));
            }
            unique(
                &mut p,
                "file",
                s.files.iter().map(|f| format!("{}/{}", s.id, f.path)),
            );
            for f in &s.files {
                if !is_sha256(&f.hash) {
                    p.push(format!(
                        "skill `{}` file `{}`: hash must be sha256:<64 hex>",
                        s.id, f.path
                    ));
                }
            }
        }
        p
    }
}

// ------------------------------------------------------- .harness/state.json

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedRecord {
    /// Also the backup directory name under `.harness/backups/`.
    pub id: String,
    /// RFC 3339 timestamp with offset.
    pub at: String,
    pub tool_version: String,
}

/// Ownership is per file (`block: None`) or per managed block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedEntry {
    pub path: RelPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// Baseline: `sha256:<hex>` of the owned file or block content.
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub skill: String,
    pub agent: AgentId,
    pub placement: Placement,
    pub target: RelPath,
    /// Copies only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterRecord {
    pub agent: AgentId,
    /// Adapter contract version this tool implements for the agent.
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessState {
    pub schema_version: u32,
    /// Only successful applies are recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<AppliedRecord>,
    #[serde(default)]
    pub managed: Vec<ManagedEntry>,
    #[serde(default)]
    pub deployments: Vec<Deployment>,
    #[serde(default)]
    pub adapters: Vec<AdapterRecord>,
}

impl Default for HarnessState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            last_success: None,
            managed: Vec::new(),
            deployments: Vec::new(),
            adapters: Vec::new(),
        }
    }
}

impl ContractFile for HarnessState {
    const FILE: &'static str = STATE_FILE;

    fn parse_text(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
    fn probe_version(text: &str) -> Result<u32, String> {
        json_version(text)
    }
    fn render(&self) -> Result<String, String> {
        json_render(self)
    }

    fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if let Some(r) = &self.last_success
            && !is_slug(&r.id)
        {
            p.push(format!("last_success.id `{}` must be a slug", r.id));
        }
        unique(
            &mut p,
            "managed entry",
            self.managed
                .iter()
                .map(|m| format!("{}#{}", m.path, m.block.as_deref().unwrap_or("*"))),
        );
        for m in &self.managed {
            if !is_sha256(&m.hash) {
                p.push(format!(
                    "managed `{}`: hash must be sha256:<64 hex>",
                    m.path
                ));
            }
            if m.block.is_some()
                && self
                    .managed
                    .iter()
                    .any(|o| o.path == m.path && o.block.is_none())
            {
                p.push(format!(
                    "`{}` is owned both as a whole file and by block `{}`",
                    m.path,
                    m.block.as_deref().unwrap_or_default()
                ));
            }
        }
        unique(
            &mut p,
            "deployment target",
            self.deployments.iter().map(|d| d.target.to_string()),
        );
        for d in &self.deployments {
            match (d.placement, &d.hash) {
                (Placement::Copy, Some(h)) if is_sha256(h) => {}
                (Placement::Copy, _) => p.push(format!(
                    "deployment `{}`: copy needs a sha256 hash",
                    d.target
                )),
                (Placement::Reference, Some(_)) => p.push(format!(
                    "deployment `{}`: reference takes no hash",
                    d.target
                )),
                (Placement::Reference, None) => {}
            }
        }
        unique(
            &mut p,
            "adapter",
            self.adapters.iter().map(|a| format!("{:?}", a.agent)),
        );
        p
    }
}

/// `.harness/backups/<apply-id>/<rel>`: the pre-apply copy of `rel`.
pub fn backup_path(apply_id: &str, rel: &RelPath) -> Option<RelPath> {
    is_slug(apply_id)
        .then(|| RelPath::parse(&format!("{BACKUPS_DIR}/{apply_id}/{rel}")).ok())
        .flatten()
}

// --------------------------------------------------------------------- helpers

fn json_version(text: &str) -> Result<u32, String> {
    serde_json::from_str::<VersionProbe>(text)
        .map(|p| p.schema_version)
        .map_err(|e| e.to_string())
}

fn json_render<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

/// Reports case-insensitive duplicates.
fn unique(problems: &mut Vec<String>, what: &str, ids: impl Iterator<Item = String>) {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.to_lowercase()) {
            problems.push(format!("duplicate {what} `{id}`"));
        }
    }
}

/// `[A-Za-z0-9._-]+`, not starting with `.` or `-`. Safe as a directory name.
pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(['.', '-'])
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `[A-Z_][A-Z0-9_]*`.
pub fn is_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Hyphenated hex UUID, any version.
pub fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `sha256:` followed by 64 lowercase hex digits.
pub fn is_sha256(s: &str) -> bool {
    s.strip_prefix("sha256:").is_some_and(|h| {
        h.len() == 64
            && h.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "sha256:0e306d8fba8b7d173ab0e85ab2d40a93ec393cbc814b586025de5c6f6d5f9c1a";

    const CONFIG: &str = r#"
schema_version = 1

[paths]
references = "references"
output = ".output"

[backlog]
enabled = false
path = "agent.db"

[[agents]]
id = "claude-code"
placement = "copy"

[[agents]]
id = "codex"
placement = "reference"

[[sources]]
id = "wiki"
kind = "wiki-gateway"
token_env = "GATEWAY_TOKEN"

[[sources]]
id = "vendored"
kind = "local-dir"
path = "vendor/skills"

[[skills]]
id = "wiki-skill-import"
source = "wiki"
ref = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67"
type = "skill"

[[skills]]
id = "local-one"
source = "vendored"
ref = "local-one"
"#;

    fn config() -> HarnessConfig {
        HarnessConfig::from_text(CONFIG).unwrap()
    }

    fn invalid(text: &str) -> Vec<String> {
        match HarnessConfig::from_text(text) {
            Err(ContractError::Invalid { problems, .. }) => problems,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn config_round_trips() {
        let c = config();
        assert_eq!(c.skills[0].page_type.as_deref(), Some("skill"));
        let again = HarnessConfig::from_text(&c.to_text().unwrap()).unwrap();
        assert_eq!(again, c);
        let d = HarnessConfig::default();
        assert_eq!(HarnessConfig::from_text(&d.to_text().unwrap()).unwrap(), d);
    }

    #[test]
    fn instructions_dir_defaults_when_absent() {
        // CONFIG predates the field.
        assert!(!CONFIG.contains("instructions"));
        assert_eq!(config().paths.instructions.as_str(), "instructions");
        let custom = CONFIG.replace("[paths]", "[paths]\ninstructions = \"docs/tasks\"");
        let c = HarnessConfig::from_text(&custom).unwrap();
        assert_eq!(
            c.layout().completed_log().as_str(),
            "docs/tasks/completed.log"
        );
    }

    #[test]
    fn version_is_checked_before_fields() {
        // A newer file with unknown fields reports the version, not the fields.
        let newer = "schema_version = 2\nfuture_key = true\n";
        assert!(matches!(
            HarnessConfig::from_text(newer),
            Err(ContractError::UnsupportedVersion { found: 2, .. })
        ));
        assert!(matches!(
            HarnessConfig::from_text("[paths]\n"),
            Err(ContractError::Parse { .. })
        ));
        assert!(matches!(
            HarnessLock::from_text(r#"{"schema_version": 9}"#),
            Err(ContractError::UnsupportedVersion { found: 9, .. })
        ));
        assert!(matches!(
            HarnessState::from_text(r#"{"schema_version": 0}"#),
            Err(ContractError::UnsupportedVersion { found: 0, .. })
        ));
    }

    #[test]
    fn rejects_absolute_and_escaping_paths() {
        for bad in ["/srv/refs", "C:\\work\\refs", "../refs", ""] {
            let text = CONFIG.replace(
                r#"references = "references""#,
                &format!("references = {bad:?}"),
            );
            assert!(
                matches!(
                    HarnessConfig::from_text(&text),
                    Err(ContractError::Parse { .. })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_secrets_and_unknown_keys() {
        // No field can hold a token value: unknown keys are refused...
        let text = CONFIG.replace(
            r#"token_env = "GATEWAY_TOKEN""#,
            "token_env = \"GATEWAY_TOKEN\"\ntoken = \"gho_secret\"",
        );
        assert!(matches!(
            HarnessConfig::from_text(&text),
            Err(ContractError::Parse { .. })
        ));
        // ...and token_env only accepts a variable name.
        let text = CONFIG.replace(
            r#"token_env = "GATEWAY_TOKEN""#,
            r#"token_env = "gho_abc123""#,
        );
        assert!(invalid(&text)[0].contains("token_env"));
        // Transient allocations such as ports have no field either.
        let text = CONFIG.replace("[backlog]", "[service]\nport = 8080\n\n[backlog]");
        assert!(matches!(
            HarnessConfig::from_text(&text),
            Err(ContractError::Parse { .. })
        ));
    }

    #[test]
    fn validates_references_between_entries() {
        let text = CONFIG.replace(r#"source = "vendored""#, r#"source = "missing""#);
        assert!(invalid(&text)[0].contains("unknown source"));
        let text = CONFIG.replace("14cea96f-f1e2-461e-b9a7-96b5a6f63b67", "not-a-uuid");
        assert!(invalid(&text)[0].contains("UUID"));
        let text = CONFIG.replace(r#"id = "local-one""#, r#"id = "Wiki-Skill-Import""#);
        assert!(invalid(&text)[0].contains("duplicate skill"));
        let text = CONFIG.replace(r#"path = "agent.db""#, r#"path = "data/agent.db""#);
        assert!(invalid(&text)[0].contains("backlog.path"));
        let text = CONFIG.replace(r#"path = "vendor/skills""#, r#"token_env = "X""#);
        let problems = invalid(&text);
        assert!(
            problems.iter().any(|p| p.contains("needs `path`")),
            "{problems:?}"
        );
    }

    fn lock() -> HarnessLock {
        HarnessLock {
            schema_version: SCHEMA_VERSION,
            skills: vec![LockedSkill {
                id: "wiki-skill-import".into(),
                source: "wiki".into(),
                reference: "14cea96f-f1e2-461e-b9a7-96b5a6f63b67".into(),
                page_type: Some("skill".into()),
                files: vec![LockedFile {
                    path: RelPath::parse("SKILL.md").unwrap(),
                    hash: HASH.into(),
                    locale: Some("en".into()),
                    revision: Some("sha256:upstream".into()),
                }],
            }],
        }
    }

    #[test]
    fn lock_round_trips_and_checks_hashes() {
        let l = lock();
        assert_eq!(HarnessLock::from_text(&l.to_text().unwrap()).unwrap(), l);
        let mut bad = lock();
        bad.skills[0].files[0].hash = "md5:abc".into();
        assert!(matches!(bad.to_text(), Err(ContractError::Invalid { .. })));
        let mut dup = lock();
        let first = dup.skills[0].files[0].clone();
        dup.skills[0].files.push(first);
        assert!(dup.problems()[0].contains("duplicate file"));
    }

    fn state() -> HarnessState {
        HarnessState {
            schema_version: SCHEMA_VERSION,
            last_success: Some(AppliedRecord {
                id: "20261116-140000".into(),
                at: "2026-11-16T14:00:00+09:00".into(),
                tool_version: env!("CARGO_PKG_VERSION").into(),
            }),
            managed: vec![
                ManagedEntry {
                    path: RelPath::parse("CLAUDE.md").unwrap(),
                    block: Some("agent-harness".into()),
                    hash: HASH.into(),
                },
                ManagedEntry {
                    path: RelPath::parse("instructions/completed.log").unwrap(),
                    block: None,
                    hash: HASH.into(),
                },
            ],
            deployments: vec![Deployment {
                skill: "wiki-skill-import".into(),
                agent: AgentId::ClaudeCode,
                placement: Placement::Copy,
                target: RelPath::parse(".claude/skills/wiki-skill-import/SKILL.md").unwrap(),
                hash: Some(HASH.into()),
            }],
            adapters: vec![AdapterRecord {
                agent: AgentId::ClaudeCode,
                version: "1".into(),
            }],
        }
    }

    #[test]
    fn state_round_trips() {
        let s = state();
        assert_eq!(HarnessState::from_text(&s.to_text().unwrap()).unwrap(), s);
        let d = HarnessState::default();
        assert_eq!(HarnessState::from_text(&d.to_text().unwrap()).unwrap(), d);
    }

    #[test]
    fn ownership_is_file_or_block_never_both() {
        let mut s = state();
        s.managed.push(ManagedEntry {
            path: RelPath::parse("CLAUDE.md").unwrap(),
            block: None,
            hash: HASH.into(),
        });
        assert!(
            s.problems()
                .iter()
                .any(|p| p.contains("both as a whole file"))
        );
        let mut s = state();
        s.deployments[0].hash = None;
        assert!(s.problems()[0].contains("copy needs"));
    }

    #[test]
    fn backup_paths_stay_under_backups_dir() {
        let rel = RelPath::parse("docs/CLAUDE.md").unwrap();
        assert_eq!(
            backup_path("20261116-140000", &rel).unwrap().as_str(),
            ".harness/backups/20261116-140000/docs/CLAUDE.md"
        );
        assert!(backup_path("../x", &rel).is_none());
        assert!(backup_path("", &rel).is_none());
    }

    #[test]
    fn load_missing_file_is_none() {
        let dir = std::env::temp_dir().join(format!("aht-contract-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(HarnessConfig::load(&dir).unwrap().is_none());
        std::fs::write(dir.join(CONFIG_FILE), CONFIG).unwrap();
        assert_eq!(HarnessConfig::load(&dir).unwrap(), Some(config()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn format_helpers() {
        assert!(is_uuid("01a1e24a-c2fa-72f4-914c-d9af2cc792e2"));
        assert!(!is_uuid("01a1e24a-c2fa-72f4-914c"));
        assert!(is_env_name("GATEWAY_TOKEN") && !is_env_name("gateway") && !is_env_name("1X"));
        assert!(is_slug("wiki-skill-import") && !is_slug(".hidden") && !is_slug("a/b"));
        assert!(is_sha256(HASH) && !is_sha256(&HASH.to_uppercase()));
    }
}
