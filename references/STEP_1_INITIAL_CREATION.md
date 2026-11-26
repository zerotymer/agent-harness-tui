# Step 1: Initial Harness Creation

## Objective and boundary

> 2026-11-24: the implementation moves to Node.js (TypeScript + Ink, instruction 0025). The Rust, Ratatui and Crossterm wording below records how Stage 1 was built and accepted; the rules themselves still apply.

Create a standalone Rust TUI for initial project harness configuration. Run it from the target project root. Use Ratatui and Crossterm, with Linux servers as the primary environment and native Windows and macOS support.

Implement this instruction in this repository. Preserve existing user files. Do not add worktree management, Worktrunk, Docktree, runtime modes, executable hooks, service management, or a general harness updater in this stage. Store the state needed by the Stage 3 updater now.

## 1. Project-root and path contract

- Treat the current working directory as the project root. Do not search upward and silently switch roots, change branches, or initialize Git automatically.
- Normalize paths for comparison while retaining portable project-relative paths in shared configuration. Support spaces, Korean text, and Windows drive paths at the input boundary.
- Keep managed, shared paths inside the project root. Reject escapes through `..` or resolved symlinks, duplicate destinations, overlapping managed directories, and collisions with existing files.
- Resolve configured relative paths from the project root, not from the executable or configuration file's parent directory.
- Stage 1 uses project-local state only. It does not discover, copy, or synchronize state from another worktree.

| Item | Default path | Configurability | Git policy |
| --- | --- | --- | --- |
| Active instructions | `instructions/` | User configurable (instruction root) | Track |
| Completed instructions | `instructions/.completed/` | Follows instruction root | Track |
| Completion log | `instructions/completed.log` | Follows instruction root | Track |
| References | `references/` | User configurable | Track |
| Temporary outputs (local, not shared) | `.output/` | User configurable | Ignore |
| Canonical skills | `.skills/` | Default fixed in Stage 1 | Track |
| Optional backlog | `instructions/agent.db` | Enable/disable only; follows instruction root | Ignore (the `agent-db.sh`/`agent-db.ps1` scripts are tracked) |

Stage 1 required a dot-prefixed output directory name. The [Stage 2 spec](STEP_2_SETUP_FLOW.md) §4 removes that rule (issue #36): every path is user-defined, and the output directory becomes the artifacts directory. Windows hidden attributes are a separate platform concern; a leading dot alone does not set that attribute. Explain and report any inability to apply the selected hidden attribute.

## 2. Instruction workflow to generate

Generate project guidance describing these rules; the initializer itself is not a task execution or instruction-completion system.

- In this section `instructions/` means the configured instruction root (default `instructions/`).
- Instructions are Markdown documents written in Korean, one file per instruction: `instructions/NNNN_short-name.md`, where `NNNN` is a 4-digit sequence starting at `0001`. Each instruction header carries a stable UUID and a status (`TODO`, `IN_PROGRESS` or `DONE`). Instructions are not translated. English/Korean pairs apply only to agent-facing reference, skill and guidance documents.
- Pending and completed instructions are both tracked in Git.
- On explicit task completion, set the status to `DONE` and move the file to `instructions/.completed/<UUID>.md`. Do not infer completion from an agent response or session ending.
- Append one line per completed instruction to `instructions/completed.log` in recording order, using the format `UUID | summary | timestamp with offset`.
- Instruction UUIDs must remain unique across independent working copies. Log line order and `NNNN` sequences are local; they are not a globally unique or chronological order across Git branches.
- A writer must lock the log, avoid overwriting old records, and expose merge conflicts rather than silently discard or renumber history. Cross-worktree reconciliation belongs to Stage 3.
- The initializer may create a missing empty log, but must not truncate an existing log or record initialization as a task event.
- `agent.db` is optional and lives in the instruction root. Store the choice and reserve its project-relative path. When enabled, create it with the schema the user supplied (GitHub issue #21): a `backlog` table for work the agent should do and an `instructions` table for instruction status, both with `status` (default `created`), creation/update timestamps and `created_by`/`updated_by`, plus status indexes and WAL mode. Next to it, create `agent-db.sh` (bash) and `agent-db.ps1` (PowerShell) management scripts that call the user's `sqlite3` CLI. The database and scripts are create-only: never migrate or modify an existing database, and never overwrite edited scripts.
- `completed.log` stays the source of truth for completed instructions. The scripts sync the database from it and only append to it. They record the author from `AGENT_DB_ACTOR` (default `agent`).

## 3. Shared configuration and local state

Use the following initial file contract, with explicit schema versions:

| File | Responsibility | Git policy |
| --- | --- | --- |
| `harness.toml` | Paths, selected agents, external source identifiers, selected skills, deployment strategies, backlog option | Track |
| `harness.lock.json` | Resolved source IDs, revisions/content hashes, and imported file inventory | Track |
| `.harness/state.json` | Last successful application, managed files/blocks, baseline hashes and generated-copy mappings | Ignore |
| `.harness/backups/` | Local recovery copies of files modified by the tool | Ignore |

- Do not store tokens, machine-specific absolute paths, or transient service allocation data in shared files.
- Record managed ownership at file or managed-block level. Never mark an entire existing user file as tool-owned merely because one block was added.
- A missing local state file after clone must not authorize overwriting existing files. Inspect configuration and lock data and require conflict resolution where ownership cannot be established.
- Record success only after all intended writes succeed. On partial failure, restore touched files where possible and report residual changes. Do not claim whole-directory transactional guarantees that the filesystem cannot provide.

## 4. External skills and references

- Provide a source adapter boundary for listing, searching, previewing, and fetching skill documents and bundled files.
- The primary source is the `md.zerotymer.net` Wiki.js, read only through its read-only gateway `https://md-gw.zerotymer.net`. The verified contract is gateway API v2 (`wikijs-agent-gateway` 2.0.0, checked 2026-11-20 against `/api/v2/openapi.json`):
  - `GET /api/v2/documents/<uuid>[?type=<type>]`. The UUID is case-insensitive. `type` is the Wiki.js path prefix, which may have several segments (`skill`, `claude/hook`); pass it only when the user gives one.
  - The token is optional. Reads currently need no authentication. When the configured environment variable holds a token, send `Authorization: Bearer <token>`; on 401/403, tell the user a token is required.
  - A missing page is a 404 problem document of type `page-not-found`. Report any other 404 as a gateway API mismatch, not as a missing page.
  - The response holds `pages.en` and `pages.ko`, each with `content` (Markdown) and `contentHash`; a missing locale is `null`, and both missing means `PAGE_NOT_FOUND`.
  - Store `pages.en.content` as `SKILL.md` and `pages.ko.content` as `SKILL_ko.md`, unchanged. How to use frontmatter (for example, to derive the skill name or strip Wiki.js page frontmatter) is decided later. Import one locale only if the user accepts a partial import. Never write an empty file.
  - Never call the Wiki.js GraphQL API or scrape `md.zerotymer.net` HTML. That site is a human-facing reference only.
  - Catalog listing and search have no documented REST contract yet. Until one is supplied, back them with a clearly labeled dummy that returns fixed entries without any network call, and import by UUID. Do not invent endpoints.
- Allow a local-directory source so initialization remains usable before remote API details are supplied.
- Fetch remote catalogs dynamically. Import selected content into `.skills/`, retaining source identity and resolved revision or hash. Do not change installed skills merely because the remote source changed.
- Keep ordinary reference documents under `references/`; reserve `.skills/` for canonical skill packages. Preserve bundled scripts and templates where present, but do not execute them during import.
- Use bounded network timeouts and explicit errors. If cache fallback is available, show its age and let the user select it; do not silently present cached content as current.
- Read credentials through a local environment/credential reference. Do not write credential values into Git-managed configuration, generated guidance, previews or logs.
- Reject package file paths that escape the destination. Preserve source language content; do not fabricate missing translations.

## 5. Agent guidance and skill deployment

- Generate `AGENTS.md` / `AGENTS_ko.md` and `CLAUDE.md` / `CLAUDE_ko.md` for the selected agents.
- Use `AGENTS.md`, not singular `AGENT.md`. These are project-wide files, not user-home global instructions.
- Derive both agent variants and both languages from the same structured harness configuration so their policies remain equivalent.
- Include path meanings, instruction lifecycle, log policy, reference/skill discovery, output policy, backlog option and Git policy. Keep skill bodies out of the always-loaded root guidance; include their purpose and location instead.
- For existing files, preview a bounded managed-block insertion. Preserve all content outside that block. Conflicting or malformed markers must produce a conflict, not a full-file replacement.
- Choose skill copy or reference deployment through an agent-specific adapter. Verify current discovery rules and reference support; a textual path in guidance is not proof that the agent automatically discovers a skill.
- Keep `.skills/` as the canonical shared source. Record deployed file mappings and hashes. Do not overwrite an existing unrelated skill. Any generated duplicate deployment files must have an explicit Git policy, using ignored reproducible copies by default.
- Unsupported deployment strategies must be unavailable with a reason. Do not silently require symlink privileges on Windows.
- Project guidance expresses policy; it does not by itself enforce filesystem or command permissions.

## 6. Git policy

Track active/completed instructions, `completed.log`, references, canonical skills, shared configuration/lock files and root guidance.

Generate bounded `.gitignore` entries for configured outputs, `.harness/`, the backlog database and its `-wal`, `-shm` and `-journal` files, and reproducible adapter copies where appropriate. Ignore backlog paths even before enabling the database to prevent accidental later addition. Preserve unrelated ignore rules.

Inspect nested or parent ignore effects when Git is available. If a supposedly shared path is ignored, report it. If an ignored destination is already tracked, explain that `.gitignore` does not untrack it and do not change the index automatically. Do not commit or push. Empty directories are not Git objects; do not claim they will be shared until they contain tracked files.

## 7. TUI flow

> 2026-11-26 (instruction 0023): Stage 2 changes this order. Agents come first (detect, select, install), and there is no root screen: the project root and the configuration source are shown in a header on every screen. An initialized project still gets only the summary and no-op verification below.

1. Confirm the current project root and existing configuration.
2. Configure instruction/reference/output paths and optional backlog use; show fixed paths explicitly.
3. Select agents and supported skill deployment strategies.
4. Browse sources, preview and select initial skills.
5. Preview directories, files, managed guidance blocks, Git rules and conflicts.
6. Apply explicitly and report created, preserved, skipped and failed items.

Support keyboard-only use over SSH, terminal resize, Korean display/input, cancellation, and terminal restoration after failure. Do not fetch on every redraw. If no interactive terminal is available, fail clearly rather than hang.

If initialized state exists, show its summary and permit inspection or a no-op verification. Do not expose Stage 2/3 mutations through an implicit re-initialization path. Repeating the same initialization must not duplicate blocks, skills or ignore entries.

## 8. Acceptance criteria

- A temporary project can be initialized without administrator rights or mandatory network access when a local source is used.
- Configuration survives restart and resolves paths identically; active/completed instruction policies match in both guidance languages.
- Existing guidance and logs remain intact; repeated initialization creates no duplicate managed entries.
- Meaningful tests cover path escape/collision rejection, generated guidance boundaries, import traversal rejection, Git-ignore behavior, and recovery from interrupted application.
- Validate the supported agent adapters against their current official contracts. Document unsupported strategies instead of claiming support.
- Exercise the root/path and terminal behavior on Linux, Windows and macOS; list any platform not actually verified.
- The delivered application contains no worktree/service management or runtime hook execution, and creates no SQLite schema other than the supplied `agent.db` schema.

## Deliverables

Application source, versioned configuration/state contracts, relevant tests, and English/Korean setup and usage documentation. Report verification results and unresolved integration prerequisites. Preserve the state contract for [Step 3](STEP_3_ADDITIONAL_FEATURES.md).
