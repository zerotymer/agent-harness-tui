//! `.harness/local.toml`: machine-local values (Stage 2 spec §2,
//! `HARNESS_CATALOG_CONTRACT`). `.harness/` is ignored by Git, so the
//! optional gateway token stored here is never tracked.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const LOCAL_FILE: &str = ".harness/local.toml";

/// A secret that never shows up in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// `None` for an empty or blank value.
    pub fn new(value: &str) -> Option<Self> {
        let v = value.trim();
        (!v.is_empty()).then(|| Self(v.to_string()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalFile {
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gateway: Option<GatewaySection>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GatewaySection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

fn path(root: &Path) -> PathBuf {
    root.join(LOCAL_FILE)
}

/// The stored gateway token, if any. Errors never quote the file, because
/// parser messages could echo the token.
pub fn load_token(root: &Path) -> Result<Option<Secret>, String> {
    let text = match fs::read_to_string(path(root)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{LOCAL_FILE}: {e}")),
    };
    let file: LocalFile = toml::from_str(&text).map_err(|_| {
        format!("{LOCAL_FILE} is invalid (unknown key or wrong value); the token is not used")
    })?;
    if file.schema_version != 1 {
        return Err(format!(
            "{LOCAL_FILE}: unsupported schema_version {}",
            file.schema_version
        ));
    }
    Ok(file
        .gateway
        .and_then(|g| g.token)
        .and_then(|t| Secret::new(&t)))
}

/// Writes the token, replacing the file. Owner-only permissions on Unix.
pub fn save_token(root: &Path, token: &Secret) -> Result<(), String> {
    let file = LocalFile {
        schema_version: 1,
        gateway: Some(GatewaySection {
            token: Some(token.expose().to_string()),
        }),
    };
    let text = toml::to_string(&file).map_err(|e| e.to_string())?;
    let target = path(root);
    let dir = target.parent().expect("constant has a parent");
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = dir.join("local.toml.tmp");
    let write = || -> std::io::Result<()> {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, &target)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("{LOCAL_FILE}: {e}")
    })
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
                "aht-local-{}-{}",
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

    const TOKEN: &str = "tok-5c1e7a";

    #[test]
    fn round_trip_in_ignored_harness_dir() {
        let d = TempDir::new();
        assert_eq!(load_token(&d.0).unwrap(), None);
        save_token(&d.0, &Secret::new(TOKEN).unwrap()).unwrap();
        assert_eq!(load_token(&d.0).unwrap().unwrap().expose(), TOKEN);
        // `.harness/` is in the managed `.gitignore` block, so this file is
        // never tracked.
        assert!(LOCAL_FILE.starts_with(".harness/"));
        let body = crate::gitignore::ignore_body(&crate::contract::HarnessConfig::default(), &[]);
        assert!(body.lines().any(|l| l == "/.harness/"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(d.0.join(LOCAL_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn errors_and_debug_never_show_the_token() {
        let d = TempDir::new();
        fs::create_dir_all(d.0.join(".harness")).unwrap();
        for bad in [
            format!("schema_version = 1\n[gateway]\ntoken = \"{TOKEN}\"\nextra = 1\n"),
            format!("schema_version = 1\ntoken = \"{TOKEN}\"\n"),
            format!("schema_version = 1\n[gateway]\ntoken = {TOKEN}\n"),
        ] {
            fs::write(d.0.join(LOCAL_FILE), &bad).unwrap();
            let err = load_token(&d.0).unwrap_err();
            assert!(!err.contains(TOKEN), "{err}");
        }
        fs::write(d.0.join(LOCAL_FILE), "schema_version = 2\n").unwrap();
        assert!(load_token(&d.0).unwrap_err().contains("schema_version"));
        let s = Secret::new(TOKEN).unwrap();
        assert!(!format!("{s:?}").contains(TOKEN));
        assert_eq!(Secret::new("  "), None);
    }
}
