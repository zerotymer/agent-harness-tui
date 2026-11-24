# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

Stage 1 is implemented and accepted by the user (2026-11-20; see `docs/STAGE1_REPORT.md`). The stages after Stage 1 are being redefined around the flow redesign in `instructions/0022_flow-redesign.md` (2026-11-23): Stage 2 becomes that flow (agents, paths, guidance, catalogs), Stage 3 keeps the remaining extras. The content service for dynamic skills and references is a separate program; this repository implements only its client. The repo holds the spec (SDD) for a standalone Rust TUI (Ratatui + Crossterm) that sets up an agent harness, plus the Rust crate `agent-harness-tui` that implements it. The repo itself also follows a layout similar to what the harness manages.

- Build: `cargo build`
- Test: `cargo test` (tests build target projects in temporary directories)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt --check`
- Toolchain: pinned in `rust-toolchain.toml` (Rust 1.97.1 with rustfmt and clippy).
- CI: `.github/workflows/ci.yml` runs clippy, tests and a release build on Linux, Windows and macOS (fmt on Linux) for pull requests and pushes to `main`, and uploads the binaries as workflow artifacts. Actions are official `actions/*` only, pinned to commit SHAs.
- Release: pushing a `v*` tag that matches the `Cargo.toml` version (for example `git tag v0.1.0 && git push origin v0.1.0`) runs `.github/workflows/release.yml`, which tests and builds on the three OSes and publishes the binaries with `SHA256SUMS` as a GitHub Release. Only its publish job gets `contents: write`.

- `README.md` / `README_ko.md`: purpose, design decisions, and official sources to re-check before implementing.
- `references/STEP_1_INITIAL_CREATION*.md`: Stage 1, which covers initial harness creation.
- `docs/USAGE*.md`: install and usage. `docs/STAGE1_REPORT*.md`: Stage 1 verification report.
- `references/STEP_2_SETUP_FLOW*.md`: Stage 2, the setup flow: agent detection and install, freely configurable paths, guidance files, catalog installs (skills, wiki skills, docs, hooks) and the content-service client for dynamic skills and references. `references/HARNESS_CATALOG_CONTRACT*.md`: the `project.json` / `default.json` format.
- `references/STEP_3_ADDITIONAL_FEATURES*.md`: Stage 3, which covers the bounded updater, override layers, modes, and Git worktree/Worktrunk/Docktree support.

The spec is the source of truth. When an imported skill conflicts with the spec or with another skill, check with the user if needed, then fix the spec (both languages) or the local skill copy in `.skills/`. Record any local skill edit in that skill's `source.json` `localChanges`. Never edit the wiki originals; the user reports upstream changes, and a refresh must re-apply the local edits.

## Working rules for this repository

- **Stage order:** Each stage must pass its acceptance criteria before the next starts (Stage 1 was accepted on 2026-11-20). Stage 2 follows 0022. Stage 2 adds no worktree/service management, runtime hook execution or general harness updater (Stage 3), and no SQLite schema other than the `agent.db` schema the user supplied (issue #21). The project configuration (`project.json`, or the built-in `default.json`) is a local JSON file, and the TUI never downloads it. The content service is a separate program; this repository implements only its client. The TUI runs only argument arrays from a catalog, without a shell, after the user confirms.
- **Languages:**
  - Work instructions are written in Korean only.
  - Agent-facing documents (references, skills, guidance such as this file) are an English `{name}.md` plus an equivalent Korean `{name}_ko.md`, and both are edited together.
  - Never invent translations for imported source documents.
- **Do not invent external contracts:** do not guess endpoints or event names for `md.zerotymer.net`, agent hook contracts, Worktrunk (`.config/wt.toml`), or Docktree. Verify them against current official docs first. Unsupported features must show as unavailable; never add shims for them. Wiki pages are listed from the local wiki catalog, never from a network listing. Dynamic skills and references are read from the content service each time they are used. The service contract is shared with the service project; never guess it.
- **Git policy:**
  - Track `instructions/` (including `.completed/` and `completed.log`), `references/`, `.skills/`, and the root guidance files.
  - Ignore `.output/`, `.harness/`, `agent.db*`, agent session files (`.claude/session*`, `.codex/session*`), and credentials. `.output/` holds temporary, local-only output that is not shared; it is one-way and never read back as input.
  - Keep `.gitignore` in sync whenever a new local-state or output path appears.
  - Never commit or push on the user's behalf.
- **Instruction lifecycle** (follows the `instruction-operating-rules` skill):
  - Each instruction is one file, `instructions/NNNN_short-name.md`. Its header holds a UUID and a status (`TODO` → `IN_PROGRESS` → `DONE`).
  - An instruction is complete only when the user explicitly says so. Session end or an agent's reply never counts as completion.
  - On completion, move the file to `instructions/.completed/<UUID>.md` and append `UUID | summary | timestamp(+0900)` to `instructions/completed.log`.
  - `completed.log` is append-only: never truncate, rewrite, or reorder it.
- **Branches:** follow `git-branch-guideline`, for example `feature/{NAME}/{UUID}`, using the instruction's UUID. Worktrees go in `../.worktree/{BRANCH}` (see `git-worktree-guideline`). Never commit directly to `main` or `staging`.

## Product requirements (what the TUI does to target projects)

The specs in `references/` describe the rules the TUI creates and enforces in the target project it runs on. They are not this repository's working rules, even where they look alike. Implement and test them as product behavior against temporary target projects; never use this repository's own `instructions/`, `.gitignore` or guidance files as fixtures. Examples:

- **Project root:** the current working directory is the project root. Never search upward, switch roots, or run `git init` automatically.
- **Managed edits:** change existing user files only inside bounded managed blocks. Malformed markers count as a conflict and must never cause a full-file overwrite. Only record success after every write has succeeded.

## Skills

Canonical skills live in `.skills/<name>/` as a `SKILL.md` + `SKILL_ko.md` pair. Each one was imported from `md.zerotymer.net` by UUID using `wiki-skill-import`, and its source UUID, revision time and hash are in its `source.json`. Claude Code does not discover `.skills/` automatically, so read the matching `SKILL.md` when its topic comes up.

When you import a skill, add or update its row in this table and in `CLAUDE_ko.md`. If the UUID is already listed, update that row instead of adding a new one.

| UUID | Name | Purpose |
| --- | --- | --- |
| 14cea96f-f1e2-461e-b9a7-96b5a6f63b67 | wiki-skill-import | Read Wiki.js skills and documents (English and Korean) by UUID through the read-only `md-gw` MCP gateway. Never use curl or HTML scraping. |
| e03f48fb-3e00-41d7-b99d-c32854567d67 | git-branch-guideline | Branch naming and usage rules: protected `main`/`staging`/`dev`; `feature`/`fix`/`hotfix`/`merge` use `{NAME}/{UUID}`; `temp` uses `{DATETIME}-{UUID}`; plus tagging rules. |
| 5c502e34-1daf-44d7-b740-1baa03e583e5 | git-worktree-guideline | Worktree layout: each branch is checked out at `../.worktree/{BRANCH}`, with commands to create, rename and remove worktrees. |
| b3230cd0-6081-4570-9bda-183543f585e9 | project-session | At session start, record the session in project-local `.claude/session` and `.claude/session.log` (`<session-id> - <topic>`). These are local runtime state; never commit them. |
| 366d5d35-7b46-4d5c-b5dd-3e53d48d7492 | instruction-operating-rules | Lifecycle for `instructions/NNNN_name.md`: template, statuses, completion archive and log, UUID registry, branch linkage, and scope guardrails. |

Local copies that differ from the wiki (details in each `source.json`):
- **`git-worktree-guideline`:** branch names, the `{DATETIME}` format, and the `dev`/`fix` rules now match `git-branch-guideline` v1.1.0. The worktree path mapping `../.worktree/{BRANCH}` is unchanged.
- **`wiki-skill-import`:** uses gateway API v2: MCP at `/api/v2/mcp`, REST `GET /api/v2/documents/<uuid>`. A token is optional for reads, and scripts must set an explicit `User-Agent`.
- **`instruction-operating-rules`:** the UUID registry is removed, so UUIDs are only generated locally. The `git-branch-guideline` UUID is corrected and branch formats are aligned. Session files go in `.claude/session`, specs live in `references/`, and skill paths point to `.skills/`.
