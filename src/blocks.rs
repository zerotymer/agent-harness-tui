//! Managed blocks inside user files (Stage 1 spec §3 ownership, §5;
//! instruction 0005).
//!
//! A block is the lines between a begin and an end marker that carry the
//! same block ID. Everything outside the markers belongs to the user and is
//! kept byte-for-byte. Any malformed, duplicated, nested or mismatched marker
//! is a conflict: the file is never rewritten as a whole to "fix" it.

use crate::apply::{Desired, Ownership, sha256};
use crate::contract::is_slug;
use crate::paths::{ProjectRoot, RelPath};
use std::fmt;
use std::io;

const TAG: &str = "agent-harness";

/// Comment syntax used for the marker lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// `<!-- agent-harness:begin ID -->`
    Markdown,
    /// `# agent-harness:begin ID` (`.gitignore` and similar)
    Hash,
}

impl Syntax {
    fn marker(self, kind: &str, id: &str) -> String {
        match self {
            Self::Markdown => format!("<!-- {TAG}:{kind} {id} -->"),
            Self::Hash => format!("# {TAG}:{kind} {id}"),
        }
    }

    /// Parses an exact marker line into (is_begin, id).
    fn parse(self, line: &str) -> Option<(bool, &str)> {
        let inner = match self {
            Self::Markdown => line.strip_prefix("<!-- ")?.strip_suffix(" -->")?,
            Self::Hash => line.strip_prefix("# ")?,
        };
        let rest = inner.strip_prefix(TAG)?.strip_prefix(':')?;
        let (kind, id) = rest.split_once(' ')?;
        let begin = match kind {
            "begin" => true,
            "end" => false,
            _ => return None,
        };
        is_slug(id).then_some((begin, id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    NotUtf8,
    InvalidId(String),
    /// The new body would itself contain marker text.
    BodyHasMarker,
    /// Line number (1-based) and what is wrong.
    Malformed(usize, String),
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => write!(f, "file is not UTF-8"),
            Self::InvalidId(id) => write!(f, "block id `{id}` must be a slug"),
            Self::BodyHasMarker => write!(f, "block body contains `{TAG}:` marker text"),
            Self::Malformed(line, why) => write!(f, "line {line}: {why}"),
        }
    }
}

impl std::error::Error for BlockError {}

/// Result of inserting or updating one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEdit {
    /// Full file content after the edit.
    pub content: String,
    /// Hash of the block body after the edit (LF-normalized).
    pub new_hash: String,
    /// Hash of the block body currently in the file, if present.
    pub current_hash: Option<String>,
}

/// Line span of a block: (begin line index, end line index).
type Span = (usize, usize);

/// Validates every marker in `text` and returns the span of each block.
fn scan(text: &str, syntax: Syntax) -> Result<Vec<(String, Span)>, BlockError> {
    let mut blocks: Vec<(String, Span)> = Vec::new();
    let mut open: Option<(String, usize)> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if !line.contains(&format!("{TAG}:")) {
            continue;
        }
        let malformed = |why: String| Err(BlockError::Malformed(i + 1, why));
        let Some((begin, id)) = syntax.parse(line) else {
            return malformed(format!("unrecognized marker `{line}`"));
        };
        match (begin, &open) {
            (true, Some((outer, _))) => {
                return malformed(format!("block `{id}` starts inside block `{outer}`"));
            }
            (true, None) => {
                if blocks.iter().any(|(b, _)| b == id) {
                    return malformed(format!("duplicate block `{id}`"));
                }
                open = Some((id.to_string(), i));
            }
            (false, None) => return malformed(format!("end of block `{id}` without a begin")),
            (false, Some((outer, start))) => {
                if outer != id {
                    return malformed(format!("block `{outer}` is closed by end `{id}`"));
                }
                blocks.push((id.to_string(), (*start, i)));
                open = None;
            }
        }
    }
    if let Some((id, start)) = open {
        return Err(BlockError::Malformed(
            start + 1,
            format!("block `{id}` is never closed"),
        ));
    }
    Ok(blocks)
}

fn hash_body(body: &str) -> String {
    sha256(body.replace("\r\n", "\n").as_bytes())
}

/// Inserts block `id` with `body` into `existing` (or a new file), or
/// replaces the body of the existing block. Text outside the block is kept.
pub fn upsert(
    existing: Option<&str>,
    syntax: Syntax,
    id: &str,
    body: &str,
) -> Result<BlockEdit, BlockError> {
    if !is_slug(id) {
        return Err(BlockError::InvalidId(id.into()));
    }
    if body.contains(&format!("{TAG}:")) {
        return Err(BlockError::BodyHasMarker);
    }
    let text = existing.unwrap_or("");
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let body = body.replace("\r\n", "\n");
    let body_lines: Vec<&str> = body.lines().collect();
    let new_hash = hash_body(
        &body_lines
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>(),
    );

    let mut block = vec![syntax.marker("begin", id)];
    block.extend(body_lines.iter().map(|l| l.to_string()));
    block.push(syntax.marker("end", id));
    let block = block.join(eol) + eol;

    let blocks = scan(text, syntax)?;
    let Some((_, (start, end))) = blocks.into_iter().find(|(b, _)| b == id) else {
        // Append, separated from user content by one blank line.
        let mut content = text.to_string();
        if !content.is_empty() {
            if !content.ends_with('\n') {
                content.push_str(eol);
            }
            content.push_str(eol);
        }
        content.push_str(&block);
        return Ok(BlockEdit {
            content,
            new_hash,
            current_hash: None,
        });
    };

    // Byte offsets of the begin line and the line after the end marker.
    let mut offsets = vec![0];
    offsets.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let begin_at = offsets[start];
    let after_end = offsets.get(end + 1).copied().unwrap_or(text.len());
    let current_body: String = text[offsets[start + 1]..offsets[end]].to_string();
    let had_eol = text[..after_end].ends_with('\n');

    let mut content = String::with_capacity(text.len() + block.len());
    content.push_str(&text[..begin_at]);
    content.push_str(if had_eol {
        &block
    } else {
        block.trim_end_matches(['\r', '\n'])
    });
    content.push_str(&text[after_end..]);
    Ok(BlockEdit {
        content,
        new_hash,
        current_hash: Some(hash_body(&current_body)),
    })
}

/// Reads `path` and builds the apply-engine item for block `id`. Problems
/// with the existing file become a planned conflict, never a rewrite.
pub fn desired(
    root: &ProjectRoot,
    path: RelPath,
    syntax: Syntax,
    id: &str,
    body: &str,
) -> io::Result<Desired> {
    let current = match std::fs::read(root.abs(&path)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let text = match current.map(String::from_utf8).transpose() {
        Ok(t) => t,
        Err(_) => return Ok(unresolvable(path, BlockError::NotUtf8)),
    };
    Ok(match upsert(text.as_deref(), syntax, id, body) {
        Ok(edit) => Desired {
            path,
            content: edit.content.into_bytes(),
            ownership: Ownership::Block {
                id: id.to_string(),
                new_hash: edit.new_hash,
                current_hash: edit.current_hash,
            },
        },
        Err(e) => unresolvable(path, e),
    })
}

fn unresolvable(path: RelPath, e: BlockError) -> Desired {
    Desired {
        path,
        content: Vec::new(),
        ownership: Ownership::Unresolvable(format!("managed block: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{Action, apply, plan};
    use crate::contract::HarnessState;
    use std::path::PathBuf;

    const MD: Syntax = Syntax::Markdown;

    #[test]
    fn creates_new_file() {
        let e = upsert(None, MD, "agent-harness", "line 1\nline 2").unwrap();
        assert_eq!(
            e.content,
            "<!-- agent-harness:begin agent-harness -->\nline 1\nline 2\n<!-- agent-harness:end agent-harness -->\n"
        );
        assert_eq!(e.current_hash, None);
    }

    #[test]
    fn appends_to_user_file_and_keeps_it() {
        let user = "# My notes\n\nkeep me";
        let e = upsert(Some(user), MD, "h", "managed").unwrap();
        assert!(
            e.content
                .starts_with("# My notes\n\nkeep me\n\n<!-- agent-harness:begin h -->")
        );
        // Second run replaces in place; no duplicate block.
        let again = upsert(Some(&e.content), MD, "h", "managed").unwrap();
        assert_eq!(again.content, e.content);
        assert_eq!(again.current_hash.as_deref(), Some(e.new_hash.as_str()));
    }

    #[test]
    fn replaces_body_only() {
        let text =
            "top\n<!-- agent-harness:begin h -->\nold\n<!-- agent-harness:end h -->\nbottom\n";
        let e = upsert(Some(text), MD, "h", "new 1\nnew 2\n").unwrap();
        assert_eq!(
            e.content,
            "top\n<!-- agent-harness:begin h -->\nnew 1\nnew 2\n<!-- agent-harness:end h -->\nbottom\n"
        );
        assert_eq!(e.current_hash, Some(sha256(b"old\n")));
        assert_eq!(e.new_hash, sha256(b"new 1\nnew 2\n"));
    }

    #[test]
    fn preserves_crlf_and_missing_final_newline() {
        let text = "top\r\n<!-- agent-harness:begin h -->\r\nold\r\n<!-- agent-harness:end h -->";
        let e = upsert(Some(text), MD, "h", "new").unwrap();
        assert_eq!(
            e.content,
            "top\r\n<!-- agent-harness:begin h -->\r\nnew\r\n<!-- agent-harness:end h -->"
        );
        // CRLF bodies hash like LF bodies, so line endings alone are no edit.
        assert_eq!(e.current_hash, Some(sha256(b"old\n")));
        let appended = upsert(Some("a\r\nb"), MD, "h", "x").unwrap();
        assert_eq!(
            appended.content,
            "a\r\nb\r\n\r\n<!-- agent-harness:begin h -->\r\nx\r\n<!-- agent-harness:end h -->\r\n"
        );
    }

    #[test]
    fn keeps_other_blocks_and_bom() {
        let text =
            "\u{feff}<!-- agent-harness:begin other -->\nx\n<!-- agent-harness:end other -->\n";
        let e = upsert(Some(text), MD, "h", "y").unwrap();
        assert!(e.content.starts_with(text));
    }

    #[test]
    fn hash_syntax_for_gitignore() {
        let e = upsert(
            Some("target/\n"),
            Syntax::Hash,
            "ignore",
            ".output/\n.harness/",
        )
        .unwrap();
        assert_eq!(
            e.content,
            "target/\n\n# agent-harness:begin ignore\n.output/\n.harness/\n# agent-harness:end ignore\n"
        );
    }

    #[test]
    fn malformed_markers_are_errors() {
        let cases = [
            ("<!-- agent-harness:begin h -->\nx\n", "never closed"),
            ("<!-- agent-harness:end h -->\n", "without a begin"),
            (
                "<!-- agent-harness:begin h -->\n<!-- agent-harness:begin i -->\n",
                "inside block",
            ),
            (
                "<!-- agent-harness:begin h -->\n<!-- agent-harness:end i -->\n",
                "closed by end",
            ),
            (
                "<!-- agent-harness:begin h -->\n<!-- agent-harness:end h -->\n\
                 <!-- agent-harness:begin h -->\n<!-- agent-harness:end h -->\n",
                "duplicate",
            ),
            ("<!-- agent-harness:begin -->\n", "unrecognized"),
            ("# agent-harness:begin h\n", "unrecognized"), // wrong syntax for Markdown
        ];
        for (text, expect) in cases {
            match upsert(Some(text), MD, "h", "body") {
                Err(BlockError::Malformed(_, why)) => assert!(why.contains(expect), "{why}"),
                other => panic!("{text:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_bad_id_and_marker_in_body() {
        assert_eq!(
            upsert(None, MD, "../x", "b"),
            Err(BlockError::InvalidId("../x".into()))
        );
        assert_eq!(
            upsert(None, MD, "h", "<!-- agent-harness:end h -->"),
            Err(BlockError::BodyHasMarker)
        );
    }

    struct TempProject(PathBuf);

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn works_with_apply_engine() {
        let dir = std::env::temp_dir().join(format!("aht-blocks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = TempProject(dir);
        let root = ProjectRoot::new(&p.0).unwrap();
        let file = p.0.join("CLAUDE.md");
        std::fs::write(&file, "# Project\n\nuser rules\n").unwrap();
        let rel = RelPath::parse("CLAUDE.md").unwrap();
        let mut state = HarnessState::default();
        let step = |state: &mut HarnessState, body: &str| {
            let d = desired(&root, rel.clone(), MD, "agent-harness", body).unwrap();
            plan(&root, state, &[], vec![d]).unwrap()
        };

        // Insert into a user file.
        let pl = step(&mut state, "policy v1");
        assert_eq!(pl.items[0].action, Action::Update);
        apply(&root, &pl, &mut state).unwrap();
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .starts_with("# Project\n\nuser rules\n\n")
        );

        // User edits outside the block: the block is still ours, nothing to do.
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, text.replace("user rules", "user rules, edited")).unwrap();
        assert_eq!(
            step(&mut state, "policy v1").items[0].action,
            Action::Unchanged
        );

        // New body: updated in place, user edit kept.
        let pl = step(&mut state, "policy v2");
        assert_eq!(pl.items[0].action, Action::Update);
        apply(&root, &pl, &mut state).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("user rules, edited") && text.contains("policy v2"));

        // User edits inside the block: conflict.
        std::fs::write(&file, text.replace("policy v2", "policy hacked")).unwrap();
        assert!(matches!(
            step(&mut state, "policy v3").items[0].action,
            Action::Conflict(_)
        ));

        // Broken marker: conflict, never a full rewrite.
        std::fs::write(&file, "<!-- agent-harness:begin agent-harness -->\n").unwrap();
        let pl = step(&mut state, "policy v3");
        assert!(matches!(&pl.items[0].action, Action::Conflict(m) if m.contains("never closed")));
        assert!(apply(&root, &pl, &mut state).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "<!-- agent-harness:begin agent-harness -->\n"
        );
    }
}
