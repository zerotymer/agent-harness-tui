# agent-harness-tui: install and use

`agent-harness-tui` is a terminal wizard that sets up an agent harness in the project you run it from. It creates instruction folders, agent guidance (`AGENTS.md`, `CLAUDE.md` and their `_ko.md` pairs), shared skills under `.skills/`, and a `.gitignore` block. Every change is previewed first and written only when you confirm.

This document covers Stage 1, initial creation, and the Stage 2 catalogs: reading the three catalogs, importing wiki skills from the wiki catalog, installing public skills with the skills CLI, and listing documents. Updating an existing harness is Stage 3.

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

Keys: ↑↓ move, Space toggles, Enter goes to the next step (on step 4 it runs the focused button), Esc goes back (on the first step it quits), Ctrl+C cancels, and `q` quits outside text fields. PgUp/PgDn scroll the preview. Korean text can be typed in path and name fields. The screen adapts to resizing, and below 80 columns the step list folds into one line.

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
| `.harness/local.toml` | Optional gateway token (owner-only permissions on Unix) | Ignore |
| `.output/` (configurable) | Temporary local output, not shared; never read back as input | Ignore |
| `.gitignore` | A managed block with the ignore rules above; your other rules are kept | Track |

The tool changes existing files only inside managed blocks that start with `agent-harness:begin` and end with `agent-harness:end`. If the markers are damaged, or you edited inside a block, the preview shows a conflict, and the tool never rewrites the whole file. File formats are documented in [`references/HARNESS_STATE_CONTRACT.md`](../references/HARNESS_STATE_CONTRACT.md).

## Skill sources

- **Local folder.** Use a folder inside the project, for example `vendor/skills`. Each subfolder that contains a `SKILL.md` is a skill. Bundled files are copied but never executed. Symlinks that point outside the package are refused. A folder outside the project can't be saved in shared settings, so copy those skills into the project first.
- **Wiki gateway (`md.zerotymer.net`).** The tool reads the gateway `https://md-gw.zerotymer.net` with `GET /api/v2/documents/<uuid>` (gateway API v2).
  - **Token (optional):** reads currently need no token. You can type one in the token row; it is never drawn on screen, and after a successful apply it is saved to `.harness/local.toml`, which Git ignores. A token in the environment variable named in the token variable row (default `GATEWAY_TOKEN`) wins over the typed or saved token. If the gateway answers 401/403, give a token. The token never appears in tracked files, screens or error messages.
  - **Type:** optional Wiki.js path prefix such as `skill` or `claude/hook`.
  - **Fetching:** enter a page UUID and press [조회] (fetch). The network is used only at that moment. The request times out after 15 seconds, and the screen does not update until the response arrives.
  - **Content:** English and Korean pages become `SKILL.md` and `SKILL_ko.md`, unchanged. If one language is missing, you must accept a partial import. The tool never creates a missing translation.
  - **Catalog:** [목록 불러오기] (load list) shows the wiki catalog. Picking an entry fills in its UUID, type and skill name. Archived entries and entries whose `target` is not `skill` are listed but cannot be imported.

## Catalogs

Three local JSON files list what you can pick: `wiki.json` (Wiki.js pages), `skills.json` (public skills) and `docs.json` (documents). Their format is in [`references/HARNESS_CATALOG_CONTRACT.md`](../references/HARNESS_CATALOG_CONTRACT.md). The tool never downloads a catalog; whoever distributes the tool also distributes the files.

Each catalog is looked up in this order, and the first one found is used:

1. The path in `AGENT_HARNESS_WIKI_CATALOG`, `AGENT_HARNESS_SKILLS_CATALOG` or `AGENT_HARNESS_DOCS_CATALOG`. A relative path is taken from the project root.
2. `catalog/<kind>.json` next to the executable.
3. The sample built into the executable (this repository's `catalog/`).

A file with an unknown key, the wrong `kind`, an unsupported `schema_version` or a duplicate `id` is rejected with an error, and the tool does not fall back to another catalog. [카탈로그 보기] (view catalogs) on the skills step shows each catalog's origin, `catalog_version` and per-entry state: available, imported, archived, conflict (the `.skills/<id>/` folder already exists), or list only. Documents are list only for now.

## Installing public skills

In [카탈로그 보기] (view catalogs), move to a public skill and press Enter to open its confirmation screen. It shows the exact command, which is built from the catalog entry and never taken as a command string:

```text
npx skills add <source> [-s <skill>]
```

- `-s` is left out when the entry has no `skill` or `*`; the CLI then offers every skill in the source.
- The TUI adds no `-a`, `-g` or `-y`. When you press Enter, the TUI screen closes and the terminal is handed to the skills CLI, which asks for the target agents, project or user scope, and skills itself. When it exits, press Enter to return; the TUI shows the exit code, and anything other than 0 is a failure.
- It runs without a shell in the project root (`npx.cmd` on Windows). If `npx` is not on `PATH`, installing shows as unavailable.
- To pin a version, the catalog writes it into `source` in the CLI's syntax, for example `owner/repo@<commit-sha>`.
- Name conflicts: if the entry's `skill` matches a skill the harness deployed or selected (ignoring case), the install is blocked. When installing every skill, names are not known in advance, so the screen warns that the check is unavailable.
- The skills CLI owns what it installs (for example `.agents/skills/`, links under `.claude/skills/`, and `skills-lock.json`). The harness does not track or remove those files.
- Entries for other installers (for example `claude plugin`) only show their command for you to run.

Checked with skills CLI 1.9.0 (2026-11-23): `npx skills add anthropics/skills@<commit-sha> -s pdf` in an empty project installed `.agents/skills/pdf`, linked it into `.claude/skills/pdf`, and recorded the commit in `skills-lock.json`. That run was inside a coding agent, where the CLI does not prompt; the interactive prompts in a normal terminal still need a manual check.

## Running again

When `harness.toml` exists, the tool shows a summary and checks whether the files still match the settings. It never applies changes in this mode. Adding or updating skills comes in a later stage. Running the same initialization again never duplicates blocks, skills or ignore rules.

## Recovery

- Before changing an existing file, the tool copies it to `.harness/backups/<apply-id>/`.
- If a write fails, already written files are restored and newly created files and empty folders are removed. Anything that could not be undone is listed, and no success is recorded.
- If `.harness/apply.lock` is left behind after a crash, the tool refuses to apply until you delete it. Delete it only when no other run is active.
- The tool never commits or pushes. If a path it would ignore is already tracked by Git, it tells you to run `git rm --cached` yourself.
