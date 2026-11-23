# Harness catalog contract

Schema version: **1**. Used by [Stage 2](STEP_2_CATALOG_INTEGRATION.md). The distributor writes these files; the TUI only reads them. The TUI never downloads a catalog.

| File | `kind` | Purpose |
| --- | --- | --- |
| `wiki.json` | `wiki` | Wiki.js pages, read through gateway API v2 |
| `skills.json` | `skills` | Public skills, installed with `npx skills add` |
| `docs.json` | `docs` | Downloadable documents |

## Envelope

```json
{
  "schema_version": 1,
  "kind": "skills",
  "catalog_version": "2026.11.20",
  "entries": []
}
```

- `schema_version` (integer) is read first. Any version other than 1 is unsupported.
- `kind` must match the file the TUI asked for.
- `catalog_version` (string) is a free-form label that the distributor chooses. The TUI shows it.
- Unknown keys are rejected at every level.
- UTF-8, no BOM.

## Common entry fields

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `id` | string | yes | Unique within the file, compared case-insensitively. No control characters. |
| `title` | `{ "en"?, "ko"? }` | yes | At least one locale. A missing locale is omitted, never invented. |
| `summary` | `{ "en"?, "ko"? }` | no | Same rule as `title`. |
| `version` | string | no | Version label for display (for example a commit SHA). |
| `updated` | string | no | RFC 3339 timestamp. |

## `wiki` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `uuid` | string | yes | UUID, compared in lowercase. |
| `type` | string | yes | Wiki.js path type: slugs joined by `/` (`skill`, `claude/hook`). |
| `target` | `"skill"` \| `"doc"` | no | `skill`: importable into `.skills/<id>/`. Otherwise list only. |
| `status` | `"active"` \| `"archived"` | no | Default `active`. Archived entries cannot be imported. |

When `target` is `skill`, `id` must be a slug (`[A-Za-z0-9._-]+`, not starting with `.` or `-`) because it becomes the directory name.

```json
{ "id": "wiki-skill-import", "uuid": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67",
  "type": "skill", "target": "skill", "status": "active",
  "title": { "en": "wiki-skill-import", "ko": "wiki-skill-import" },
  "updated": "2026-06-28T12:25:41.928Z" }
```

## `skills` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `installer` | string | yes | `skills-cli` is run by the TUI. Any other value is display only. |
| `source` | string | yes | Repository URL or `owner/repo`. Shown to the user. For `skills-cli` it is the CLI's source argument. |
| `skill` | string | `skills-cli` only | Skill name passed to `-s`. |
| `ref` | string | no | `skills-cli` only: branch, tag or commit SHA appended as `#<ref>`. |
| `install` | string | other installers only | Command text shown for the user to run. The TUI never runs it. |

- `source`, `skill` and `ref` must not start with `-` and must not contain whitespace or control characters. They are passed as separate arguments without a shell.
- A `skills-cli` entry must not have `install`; another installer must not have `skill` or `ref`.

```json
{ "id": "security-audit", "installer": "skills-cli",
  "source": "https://github.com/cloudflare/security-audit-skill", "skill": "security-audit",
  "ref": "c1c8a8c1471069fb0e188eeaff69b8e8db6564a8",
  "title": { "en": "security-audit" },
  "summary": { "en": "Multi-phase security audits.", "ko": "다단계 보안 감사 스킬." },
  "version": "c1c8a8c1471069fb0e188eeaff69b8e8db6564a8" }

{ "id": "ponytail@ponytail", "installer": "claude-code-cli",
  "source": "https://github.com/DietrichGebert/ponytail",
  "install": "claude plugin marketplace add DietrichGebert/ponytail && claude plugin install ponytail@ponytail",
  "title": { "en": "ponytail" } }
```

## `docs` entries

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `url` | string | yes | `https://` URL, preferably pinned to a commit. |
| `filename` | string | yes | Plain file name without a path separator. Saved as `references/<filename>` (backlog). |
| `sha256` | string | no | 64 lowercase hex digits of the file. |

```json
{ "id": "anthropics-skills-readme",
  "url": "https://raw.githubusercontent.com/anthropics/skills/e4bb5d960d87126ff3b3a8af1a104b7db67560ba/README.md",
  "filename": "anthropics-skills-README.md",
  "title": { "en": "Anthropic skills README" },
  "version": "e4bb5d960d87126ff3b3a8af1a104b7db67560ba" }
```

## Local token file

`.harness/local.toml` holds machine-local values. Git ignores it because `.harness/` is ignored. Stage 2 defines one key:

```toml
schema_version = 1

[gateway]
token = "..."   # optional; read-only gateway token
```

Unknown keys are rejected. Stage 3 may add keys for machine and worktree overrides.
