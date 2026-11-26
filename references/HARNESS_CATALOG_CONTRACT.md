# Harness project configuration contract

Schema version: **2**. Used by [Stage 2](STEP_2_SETUP_FLOW.md). Version 1 (three separate files `wiki.json`, `skills.json` and `docs.json`) is replaced and no longer read.

| File | Where | Role |
| --- | --- | --- |
| `project.json` | Project root, tracked in Git | Project configuration. Written by the project's owner |
| `default.json` | Built into the executable from `catalog/default.json` | Used only when `project.json` does not exist |

The two files are never merged. The TUI only reads them and never downloads them.

## Top level

```json
{
  "schema_version": 2,
  "catalog_version": "2026.11.24",
  "paths": { "instructions": "instructions", "artifacts": ".artifacts", "docs": "docs", "references": "references" },
  "service": { "url": "https://harness.example.internal", "project": "my-project", "token_env": "HARNESS_SERVICE_TOKEN" },
  "agents": [],
  "skills": [],
  "wiki_skills": [],
  "docs": [],
  "hooks": []
}
```

- Read `schema_version` (integer) first. Any version other than 2 is unsupported.
- `catalog_version` (string, optional) is a free-form label. When present, the TUI shows it.
- `paths` (optional) gives the initial paths on the paths screen. Each key is optional. A `null` value starts that optional path as disabled. `instructions` cannot be `null`.
- `service` (optional) gives the initial content-service settings. `token_env` is a variable name matching `[A-Z_][A-Z0-9_]*`, never a token value.
- Every list is optional and defaults to empty.
- Unknown keys are rejected at every level.
- UTF-8, no BOM.

## Common entry fields

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `id` | string | Yes | Unique within its list, ignoring case. No control characters. |
| `title` | `{ "en"?, "ko"? }` | Yes | At least one language. Omit a missing language; never invent a translation. |
| `summary` | `{ "en"?, "ko"? }` | No | Same rule as `title`. |
| `version` | string | No | Display-only version (for example a commit SHA). |
| `updated` | string | No | RFC 3339 timestamp. |

## Argument vectors

Commands are JSON arrays of strings, run without a shell.

- At least one element. No element is empty or holds a control character.
- The first element is the program. It must not start with `-`.
- The TUI never joins the elements into one string and never passes them to a shell. The one exception is a Windows `.cmd` or `.bat` program, which Node.js can start only through `cmd.exe`; then every element must be free of spaces and `cmd` metacharacters (`& | < > ^ % ! " ( )`), or the command is refused (Stage 2 spec §3).

## `agents` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `id` | string | Yes | Slug (`[A-Za-z0-9._-]+`, not starting with `.` or `-`). Stored in `harness.toml`. |
| `detect` | argument vector | Yes | Exit code 0 means installed. The first output line is shown as the version. |
| `guidance` | string | Yes | Guidance file name in the project root, for example `CLAUDE.md` or `AGENTS.md`. No path separator, ends in `.md`. The Korean pair is `<name>_ko.md`. |
| `skills_dir` | string | Yes | Project-relative directory where the agent discovers skills. |
| `install` | `{ "linux"?, "macos"?, "windows"? }` | No | An argument vector per OS. A missing OS means install is unavailable there. |
| `default` | boolean | No | Preselected on the agents screen. Default `false`. |

```json
{ "id": "claude-code", "title": { "en": "Claude Code" },
  "detect": ["claude", "--version"],
  "guidance": "CLAUDE.md", "skills_dir": ".claude/skills",
  "install": {
    "linux":   ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"],
    "macos":   ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"],
    "windows": ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"] },
  "default": true }
```

The built-in agents install through npm (see the Stage 2 spec §3 for the constraints). On Windows the program is looked up with the `PATHEXT` extensions, so `npm` finds `npm.cmd`; the vector stays the same on every OS.

## `skills` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `installer` | string | Yes | `skills-cli` is run by the TUI. Other values are shown only. |
| `source` | string | Yes | Repository URL or `owner/repo`. Shown to the user. For `skills-cli` it is passed to the CLI unchanged. |
| `skill` | string | No | `skills-cli` only. The skill name passed to `-s`: the `name` in the skill's `SKILL.md` frontmatter. When omitted or `*`, every skill in the source is installed and `-s` is not passed. |
| `install` | string | Other installers only | Command text shown for the user to run. The TUI does not run it. |

- `source` and `skill` cannot start with `-` and cannot contain whitespace or control characters. Each is passed as its own argument without a shell.
- A `skills-cli` entry cannot have `install`. Another installer's entry cannot have `skill`.
- There is no install version field. To pin a version, write it in `source` with the skills CLI syntax (for example `owner/repo@<commit SHA>` or `owner/repo#<tag>`). The TUI passes `source` unchanged. `version` is display-only.

```json
{ "id": "security-audit", "installer": "skills-cli",
  "source": "cloudflare/security-audit-skill@c1c8a8c1471069fb0e188eeaff69b8e8db6564a8", "skill": "security-audit",
  "title": { "en": "security-audit" },
  "summary": { "en": "Multi-phase security audits.", "ko": "다단계 보안 감사 스킬." } }
```

## `wiki_skills` entries

Wiki.js skill pages, imported as static copies into `.skills/<id>/`.

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `id` | string | Yes | Slug (`[A-Za-z0-9._-]+`, not starting with `.` or `-`). It becomes the directory name. |
| `uuid` | string | Yes | UUID. Compared in lowercase. |
| `type` | string | Yes | Wiki.js path type: slugs joined by `/` (`skill`, `claude/hook`). |
| `status` | `"active"` \| `"archived"` | No | Default `active`. Archived entries cannot be imported. |

```json
{ "id": "wiki-skill-import", "uuid": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67", "type": "skill",
  "title": { "en": "wiki-skill-import" }, "updated": "2026-06-28T12:25:41.928Z" }
```

## `docs` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `url` | string | Yes | `https://` URL. Pin it to a commit where possible. |
| `filename` | string | Yes | A file name without path separators. Saved as `<references>/<filename>`. |
| `sha256` | string | No | 64 lowercase hex digits for the file. |

```json
{ "id": "anthropics-skills-readme",
  "url": "https://raw.githubusercontent.com/anthropics/skills/e4bb5d960d87126ff3b3a8af1a104b7db67560ba/README.md",
  "filename": "anthropics-skills-README.md",
  "title": { "en": "Anthropic skills README" } }
```

## `hooks` entries

Reference entries only. Nothing is installed or run.

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `url` | string | No | `https://` link to a reference document. |

## Local token file

`.harness/local.toml` holds machine-specific values. `.harness/` is ignored by Git, so this file is too.

```toml
schema_version = 1

[gateway]
token = "..."   # optional. Read-only wiki gateway token

[service]
token = "..."   # optional. Content service token
```

- A token in the environment variable named by `token_env` takes precedence over the stored one.
- Unknown keys are rejected. Stage 3 may add machine and worktree override keys.
