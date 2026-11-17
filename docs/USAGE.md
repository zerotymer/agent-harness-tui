# agent-harness-tui: install and use

`agent-harness-tui` is a terminal wizard that sets up an agent harness in the project you run it from. It creates instruction folders, agent guidance (`AGENTS.md`, `CLAUDE.md` and their `_ko.md` pairs), shared skills under `.skills/`, and a `.gitignore` block. Every change is previewed first and written only when you confirm.

This document covers Stage 1, initial creation. Updating an existing harness is Stage 2 and is not available yet.

## Install

Prebuilt binaries for Linux, Windows and macOS are attached to each GitHub Release (`.tar.gz`, or `.zip` on Windows), with a `SHA256SUMS` file to check them against. Or build from source:

Requirements: a stable Rust toolchain. Stage 1 was built and tested with Rust 1.97.1, edition 2024.

```sh
cargo install --path .          # installs `agent-harness-tui` into ~/.cargo/bin
# or
cargo build --release           # binary at target/release/agent-harness-tui
```

You don't need administrator rights. You need network access only to fetch skills from the wiki gateway.

## Run

```sh
cd /path/to/your/project
agent-harness-tui
```

- The current directory is the project root. The tool never searches parent directories, never switches branches, and never runs `git init`.
- It needs an interactive terminal on both stdin and stdout. Otherwise it exits immediately with code 2 instead of waiting.
- `agent-harness-tui --version` prints the version, and `--help` prints usage.

## The six steps

| Step | What you do |
| --- | --- |
| 1. Root and settings | Check the project root. An already initialized project only shows a summary and a no-change check (see below). |
| 2. Paths and backlog | Set the reference folder (default `references/`), the output folder (default `.output/`; its name must start with `.`), and the instruction folder (default `instructions/`; `.completed/` and `completed.log` go under it). Turn the optional `agent.db` backlog on or off. When it is on, `agent.db`, `agent-db.sh` and `agent-db.ps1` are created in the instruction folder (an existing database is kept). The scripts need the `sqlite3` CLI. Run one without arguments to list its commands, and set `AGENT_DB_ACTOR` to record who made each change. Fixed paths are listed but cannot be changed. |
| 3. Agents and placement | Choose Claude Code and/or Codex. For each, pick **copy** (skills are copied where the agent discovers them) or **reference** (guidance points to `.skills/`). Symlink placement is shown as unavailable, with the reason. |
| 4. Sources and skills | Optionally add skills from a local folder inside the project, or from the wiki gateway. See "Skill sources". |
| 5. Preview | See every directory and file, each marked create, update, unchanged, preserve or CONFLICT, together with Git warnings. Nothing has been written yet. |
| 6. Apply and result | Press Enter on the preview to apply. The result lists what was created, updated, unchanged and preserved, and the backup folder. |

Keys: ↑↓ move, Space toggles, Enter goes to the next step (on step 4 it runs the focused button), Esc goes back, Ctrl+C cancels. PgUp/PgDn scroll the preview. Korean text can be typed in path and name fields. The screen adapts to resizing, and below 80 columns the step list folds into one line.

## What gets created

| Path | Purpose | Git |
| --- | --- | --- |
| `instructions/`, `instructions/.completed/` | Work instructions (instruction root, configurable on step 2) | Track |
| `instructions/completed.log` | Completion log; created empty only if missing, never rewritten | Track |
| `references/` (configurable) | Reference documents | Track |
| `.skills/<id>/` | Shared skill packages | Track |
| `AGENTS.md`, `AGENTS_ko.md` (Codex) / `CLAUDE.md`, `CLAUDE_ko.md` (Claude Code) | A managed guidance block; the rest of each file is yours | Track |
| `.claude/skills/<id>/`, `.agents/skills/<id>/` | Skill copies for agents set to copy placement | Ignore (regenerated) |
| `harness.toml`, `harness.lock.json` | Shared settings and resolved imports | Track |
| `.harness/state.json`, `.harness/backups/` | Local applied state and backups | Ignore |
| `.output/` (configurable) | Generated output; never read back as input | Ignore |
| `.gitignore` | A managed block with the ignore rules above; your other rules are kept | Track |

The tool changes existing files only inside managed blocks that start with `agent-harness:begin` and end with `agent-harness:end`. If the markers are damaged, or you edited inside a block, the preview shows a conflict, and the tool never rewrites the whole file. File formats are documented in [`references/HARNESS_STATE_CONTRACT.md`](../references/HARNESS_STATE_CONTRACT.md).

## Skill sources

- **Local folder.** Use a folder inside the project, for example `vendor/skills`. Each subfolder that contains a `SKILL.md` is a skill. Bundled files are copied but never executed. Symlinks that point outside the package are refused. A folder outside the project can't be saved in shared settings, so copy those skills into the project first.
- **Wiki gateway (`md.zerotymer.net`).** The tool reads the gateway `https://md-gw.zerotymer.net` with `GET /pages/by-uuid`.
  - **Token:** enter the *name* of the environment variable that holds your token. The default is `GATEWAY_TOKEN`; use any name your shell already sets. The token value itself is never written to files, screens or logs.
  - **Fetching:** enter a page UUID and press [조회] (fetch). The network is used only at that moment. The request times out after 15 seconds, and the screen does not update until the response arrives.
  - **Content:** English and Korean pages become `SKILL.md` and `SKILL_ko.md`, unchanged. If one language is missing, you must accept a partial import. The tool never creates a missing translation.
  - **Catalog:** the gateway has no catalog API yet, so [목록 불러오기] (load list) shows fixed sample entries marked "(더미)" (dummy).

## Running again

When `harness.toml` exists, the tool shows a summary and checks whether the files still match the settings. It never applies changes in this mode. Adding or updating skills is a Stage 2 feature. Running the same initialization again never duplicates blocks, skills or ignore rules.

## Recovery

- Before changing an existing file, the tool copies it to `.harness/backups/<apply-id>/`.
- If a write fails, already written files are restored and newly created files and empty folders are removed. Anything that could not be undone is listed, and no success is recorded.
- If `.harness/apply.lock` is left behind after a crash, the tool refuses to apply until you delete it. Delete it only when no other run is active.
- The tool never commits or pushes. If a path it would ignore is already tracked by Git, it tells you to run `git rm --cached` yourself.
