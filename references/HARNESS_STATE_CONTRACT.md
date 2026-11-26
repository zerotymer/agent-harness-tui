# Harness config, lock and state contract

Schema version: **2** (Stage 2, 2026-11-24). Version 1 is still read (see Common rules). Implemented in `src/contract.ts` (instruction 0003). It covers what the TUI writes into a **target project**, as described in the Stage 1 spec §3 and the [Stage 2 spec](STEP_2_SETUP_FLOW.md). It is not this repository's own configuration.

| File | Role | Git |
| --- | --- | --- |
| `harness.toml` | Shared intent: paths, agents, sources, selected skills, placement, backlog option, content service, dynamic skills and references | Track |
| `harness.lock.json` | Resolved imports: source and ref, file list, hashes | Track |
| `.harness/state.json` | Last successful apply, ownership, baseline hashes, deployments, adapters | Ignore |
| `.harness/backups/<apply-id>/<path>` | Pre-apply copy of each file the tool changed | Ignore |
| `.harness/apply.lock` | Present only while an apply runs; a leftover file blocks applying until the user removes it | Ignore |

## Common rules

- Every file has an integer `schema_version`. The version is read **before** any other field. A missing version is a parse error. Versions 1 and 2 are read; any other version is reported as unsupported, and the file is left unchanged. A version 1 file is upgraded in memory: in `harness.toml`, `paths.output` becomes `paths.artifacts` with its value kept (directories are never moved), a missing `paths.references` becomes `references`, and a missing `paths.instructions` becomes `instructions`. Lock and state files need no field changes. The next successful apply writes version 2, with the usual backup. General migration with backups is Stage 3.
- Unknown keys are rejected in every table. This is how transient data (ports, service allocations) and stray secrets are kept out of shared files.
- Every path is a project-relative `RelPath`: `/`-separated, normalized, never absolute, never escaping the root. Windows drive paths and `..` escapes are rejected when the file is parsed. Symlinks are checked against the real filesystem when paths are resolved (instruction 0002).
- IDs (`source`, `skill`, apply `id`) are slugs: `[A-Za-z0-9._-]+`, not starting with `.` or `-`. IDs are compared case-insensitively for duplicates.
- Hashes are `sha256:` followed by 64 lowercase hex digits, computed over the exact bytes written.
- The contract module only parses, validates and serializes. Writes go through the apply engine (instruction 0004). The engine loads each file first, so a file with an unsupported version is never overwritten.

## `harness.toml`

```toml
schema_version = 2

[paths]
instructions = "instructions"   # required, user configurable
artifacts = ".artifacts"        # optional; omit the key to disable
docs = "docs"                   # optional; omit the key to disable
references = "references"       # optional; omit the key to disable

[backlog]
enabled = false
path = "agent.db"               # fixed

[[agents]]
id = "claude-code"              # an agent id from the project configuration
placement = "copy"              # copy | reference

[service]                       # optional: content service (Stage 2 spec §7)
url = "https://harness.example.internal"
project = "my-project"
token_env = "HARNESS_SERVICE_TOKEN"  # variable name only, never a value

[[dynamic_skills]]
id = "team-release"             # skill name from the service
track = true                    # false: stubs are added to .gitignore

[[dynamic_references]]
path = "runbooks/deploy.md"     # reference path in the service

[[sources]]
id = "wiki"
kind = "wiki-gateway"           # https://md-gw.zerotymer.net (fixed, not stored)
token_env = "GATEWAY_TOKEN"     # variable name only, never a value

[[sources]]
id = "vendored"
kind = "local-dir"
path = "vendor/skills"          # project-relative

[[skills]]
id = "wiki-skill-import"        # also the directory .skills/<id>/
source = "wiki"
ref = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67"  # page UUID for wiki-gateway
type = "skill"                  # optional, wiki-gateway only

[[skills]]
id = "local-one"
source = "vendored"
ref = "local-one"               # directory relative to the local source

[[hooks]]                       # optional: hook reference entries (instruction 0026)
id = "claude-code-hooks"        # a hook id from the project configuration; nothing is installed
```

`.skills/` is fixed, so it is not stored. A missing optional path key means that path is disabled. No name format is enforced on any path (issue #36). Validation rules:

- `paths.instructions` is required. Managed paths must not duplicate (ignoring case) or nest inside each other.
- `backlog.path` must be `agent.db`.
- Agent, source, skill, dynamic skill and hook IDs must be unique. Dynamic reference paths must be unique. `hooks` is written only when it has entries.
- `[service]`: `url` is `https://`, or `http://` to a loopback address only. `project` is a slug. `token_env` matches `[A-Z_][A-Z0-9_]*`.
- `dynamic_skills` and `dynamic_references` need `[service]`. A dynamic skill ID is a slug and must not equal a skill ID (ignoring case).
- `local-dir` needs `path` and takes no `token_env`.
- `wiki-gateway` needs `token_env` (matching `[A-Z_][A-Z0-9_]*`) and takes no `path`. The variable may be unset: the token is optional.
- Each skill must reference an existing source. A `wiki-gateway` skill's `ref` must be a UUID, and its `type` must be slugs joined by `/` (for example `skill` or `claude/hook`).

A local directory outside the project cannot be stored here, because shared files never hold absolute paths. The Stage 1 TUI therefore accepts only local source directories inside the project; copy outside skills into the project first.

## `harness.lock.json`

```json
{
  "schema_version": 1,
  "skills": [
    {
      "id": "wiki-skill-import",
      "source": "wiki",
      "ref": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67",
      "type": "skill",
      "files": [
        { "path": "SKILL.md", "hash": "sha256:…", "locale": "en", "revision": "sha256:…" },
        { "path": "SKILL_ko.md", "hash": "sha256:…", "locale": "ko", "revision": "sha256:…" }
      ]
    }
  ]
}
```

- `files[].path` is relative to `.skills/<id>/`.
- `hash` covers the local bytes.
- `revision` is the source's own revision (the gateway's `contentHash`), copied verbatim.
- `locale` is optional.
- Each skill lists at least one file, and file paths are unique.
- `docs` (optional) records downloaded documents: `{ "id", "url", "path", "hash" }`. `path` is project-relative under the references path, and `hash` covers the saved bytes. IDs and paths are unique.

## `.harness/state.json`

```json
{
  "schema_version": 1,
  "last_success": { "id": "20261116-140000", "at": "2026-11-16T14:00:00+09:00", "tool_version": "0.1.0" },
  "managed": [
    { "path": "CLAUDE.md", "block": "agent-harness", "hash": "sha256:…" },
    { "path": ".skills/wiki-skill-import/SKILL.md", "hash": "sha256:…" }
  ],
  "deployments": [
    { "skill": "wiki-skill-import", "agent": "claude-code", "placement": "copy",
      "target": ".claude/skills/wiki-skill-import/SKILL.md", "hash": "sha256:…" }
  ],
  "adapters": [ { "agent": "claude-code", "version": "1" } ]
}
```

- **`last_success`**: written only after every planned write succeeded. Its `id` also names the backup directory. `at` is an RFC 3339 timestamp in UTC (`…Z`), and `id` is the same instant as `YYYYMMDD-HHMMSS`, with `-2`, `-3`, … appended if that backup directory already exists.
- **`managed`**: records ownership. An entry without `block` means the tool owns the whole file. An entry with `block` means the tool owns only that managed block, and the rest of the file belongs to the user. A path is never owned both ways. `hash` is the baseline of the owned content. Create-only files, such as an empty `instructions/completed.log` or a `.gitkeep`, are never owned or recorded. An entry with `key` (instead of `block`) means the tool owns only that entry of a JSON file, for example `mcpServers.agent-harness` in `.mcp.json`. `hash` then covers that entry's canonical JSON.
- **`deployments`**: agent-specific skill placements. A `copy` deployment must have a hash; a `reference` deployment has none. A `dynamic` deployment is a dynamic skill stub (Stage 2 spec §7) and has a hash. Targets are unique.
- **`adapters`**: the adapter contract version this tool implemented for each agent.

## Managed block markers

A managed block is the lines between a begin and an end marker with the same ID. The ID is a slug. Each marker sits on its own line, and surrounding whitespace is ignored.

| File type | Begin | End |
| --- | --- | --- |
| Markdown (`AGENTS.md`, `CLAUDE.md`, …) | `<!-- agent-harness:begin ID -->` | `<!-- agent-harness:end ID -->` |
| `#` comments (`.gitignore`) | `# agent-harness:begin ID` | `# agent-harness:end ID` |

- Text outside the markers belongs to the user and is kept byte for byte, including CRLF line endings, a missing final newline, and a BOM.
- A new block is appended after the user content, separated by one blank line. On later runs it is replaced in place, so it is never duplicated.
- The block hash in `managed[].hash` covers the lines between the markers, with line endings normalized to LF. A change of line endings alone is therefore not an edit.
- Any line containing `agent-harness:` that is not an exact marker is a conflict. So are duplicate IDs, nested blocks, a missing end, an end without a begin, and an end whose ID does not match. The file is never rewritten as a whole to repair markers.
- A block body may not contain `agent-harness:` text.

## Stage 3 coverage

The Stage 3 spec §8 (the former Stage 2 spec) asks for state items. Stage 1 records them as follows:

| Stage 3 item | Stage 1 field |
| --- | --- |
| Stable IDs | source/skill `id` |
| Source revisions | lock `ref`, `files[].revision`, `files[].hash` |
| Enablement | presence in `harness.toml` (explicit disable comes with Stage 3) |
| Managed files/blocks | state `managed` |
| Baseline hashes | state `managed[].hash`, `deployments[].hash` |
| Copy/reference mappings | state `deployments` |
| Supported adapter versions | state `adapters` |
| Application results | state `last_success` (failures are reported, not stored) |
| Repository/worktree identity | not recorded: Stage 1 is project-local only |
