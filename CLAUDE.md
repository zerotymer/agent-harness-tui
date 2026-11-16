# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

Stage 1 is in progress: the repo holds the spec (SDD) for a standalone Rust TUI (Ratatui + Crossterm) that sets up an agent harness, plus the Rust crate `agent-harness-tui` that implements it. The repo itself also follows a layout similar to what the harness manages.

- Build: `cargo build`
- Test: `cargo test` (tests build target projects in temporary directories)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt --check`

- `README.md` / `README_ko.md`: purpose, design decisions, and official sources to re-check before implementing.
- `references/STEP_1_INITIAL_CREATION*.md`: Stage 1, which covers initial harness creation.
- `references/STEP_2_ADDITIONAL_FEATURES*.md`: Stage 2, which covers the bounded updater, override layers, modes, hooks, and Git worktree/Worktrunk/Docktree support.

The spec is the source of truth. When an imported skill conflicts with the spec or with another skill, check with the user if needed, then fix the spec (both languages) or the local skill copy in `.skills/`. Record any local skill edit in that skill's `source.json` `localChanges`. Never edit the wiki originals; the user reports upstream changes, and a refresh must re-apply the local edits.

## Working rules for this repository

- **Stage order:** Stage 1 must pass its acceptance criteria before any Stage 2 work starts. Stage 1 must not include worktree/service management, runtime hook execution, a general updater, or an SQLite schema. It must still store the state that the Stage 2 updater will need.
- **Languages:**
  - Work instructions are written in Korean only.
  - Agent-facing documents (references, skills, guidance such as this file) are an English `{name}.md` plus an equivalent Korean `{name}_ko.md`, and both are edited together.
  - Never invent translations for imported source documents.
- **Do not invent external contracts:** do not guess endpoints or event names for `md.zerotymer.net`, agent hook contracts, Worktrunk (`.config/wt.toml`), or Docktree. Verify them against current official docs first. Unsupported features must show as unavailable; never add shims for them. The one exception is the gateway catalog list/search dummy that the Stage 1 spec §4 allows.
- **Git policy:**
  - Track `instructions/` (including `.completed/` and `completed.log`), `references/`, `.skills/`, and the root guidance files.
  - Ignore `.output/`, `.harness/`, `agent.db*`, agent session files (`.claude/session*`, `.codex/session*`), and credentials. `.output/` is one-way generated output and is never read back as input.
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
- **`instruction-operating-rules`:** the UUID registry is removed, so UUIDs are only generated locally. The `git-branch-guideline` UUID is corrected and branch formats are aligned. Session files go in `.claude/session`, specs live in `references/`, and skill paths point to `.skills/`.
