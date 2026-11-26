# agent-harness-tui: install and use

`agent-harness-tui` is a terminal wizard that sets up an agent harness in the project you run it from. It creates instruction folders, agent guidance (`AGENTS.md`, `CLAUDE.md` and their `_ko.md` pairs), shared skills under `.skills/`, and a `.gitignore` block. Every change is previewed first and written only when you confirm.

This document covers Stage 1, initial creation, and the parts of Stage 2 implemented so far: the project configuration (`project.json` or the built-in default), agent detection and install, installing public skills with the skills CLI, and listing wiki skills, documents and hooks. Updating an existing harness comes later.

## Install

Requirements: Node.js 22.13 or later (it includes `npm`).

Each GitHub Release attaches an `npm pack` tarball (`agent-harness-tui-<version>.tgz`) with a `SHA256SUMS` file to check it against. Install it globally from its URL or a downloaded copy:

```sh
npm install -g https://github.com/zerotymer/agent-harness-tui/releases/download/v<version>/agent-harness-tui-<version>.tgz
```

The package is not published to the npm registry. To build from source instead, you need pnpm (enable it with `corepack enable`):

```sh
pnpm install
pnpm build                      # JavaScript in dist/
pnpm pack                       # agent-harness-tui-<version>.tgz, then npm install -g ./agent-harness-tui-<version>.tgz
```

You don't need administrator rights. You need network access only to fetch skills from the wiki gateway.

## Run

```sh
cd /path/to/your/project
agent-harness-tui
```

- The current directory is the project root. The tool never searches parent directories, never switches branches, and never runs `git init`.
- Every screen shows a header with the project root and the configuration in use (`project.json`, or the built-in `default.json`, with its `catalog_version`).
- It needs an interactive terminal on both stdin and stdout. Otherwise it exits immediately with code 2 instead of waiting.
- `agent-harness-tui --version` prints the version, and `--help` prints usage.
- An already initialized project (with `harness.toml`) shows a summary and a no-change check, and offers only adding skills and hooks and refreshing stale guidance (see "Running again").

## The five steps

| Step | What you do |
| --- | --- |
| 1. Agents | See which agents are installed, select the ones to set up, and install missing ones (see "Agents"). For each selected agent pick **copy** (skills are copied where the agent discovers them) or **reference** (guidance points to `.skills/`). Symlink placement is shown as unavailable, with the reason. You can continue once at least one selected agent is installed. |
| 2. Paths and backlog | Set the instruction folder (required, default `instructions/`; `.completed/` and `completed.log` go under it), the artifacts folder (default `.artifacts/`, ignored by Git), the documents folder (default `docs/`) and the reference folder (default `references/`). Any name is accepted; the leading `.` only hides a folder. Leave an optional folder empty to disable it: it is not created, ignored or mentioned in guidance. Initial values come from `paths` in `project.json` (`null` starts a path disabled). The TUI only creates folders: after a change the old folder stays where it is. Turn the optional `agent.db` backlog on or off. When it is on, `agent.db`, `agent-db.sh` and `agent-db.ps1` are created in the instruction folder (an existing database is kept). The scripts need the `sqlite3` CLI. Run one without arguments to list its commands, and set `AGENT_DB_ACTOR` to record who made each change. Fixed paths are listed but cannot be changed. |
| 3. Sources and skills | Optionally add skills from a local folder inside the project, or from the wiki gateway. See "Skill sources". |
| 4. Preview | See every directory and file, each marked create, update, unchanged, preserve or CONFLICT, together with Git warnings. Nothing has been written yet. |
| 5. Apply and result | Press Enter on the preview to apply. The result lists what was created, updated, unchanged and preserved, and the backup folder. |

Keys: ↑↓ move, Space toggles, Enter goes to the next step (on steps 1 and 3 it runs the focused button), Esc goes back (on the agents step it quits), Ctrl+C cancels, and `q` quits outside text fields. PgUp/PgDn scroll the preview. Korean text can be typed in path and name fields. The screen adapts to resizing, and below 80 columns the step list folds into one line.

## What gets created

| Path | Purpose | Git |
| --- | --- | --- |
| `instructions/`, `instructions/.completed/` | Work instructions (instruction root, configurable on step 2) | Track, except `instructions/*.md` directly in the root (active instructions stay local; `README.md` is tracked) |
| `instructions/completed.log` | Completion log; created empty only if missing, never rewritten | Track |
| `docs/` (configurable, optional) | Documents you publish, mostly HTML and documents | Track |
| `references/` (configurable, optional) | Documents for agents | Track |
| `.gitkeep` | Empty file in each tracked folder the tool creates, so Git keeps it; never overwritten, not owned | Track |
| `.skills/<id>/` | Shared skill packages | Track |
| `AGENTS.md`, `AGENTS_ko.md` (Codex) / `CLAUDE.md`, `CLAUDE_ko.md` (Claude Code) | A managed guidance block; the rest of each file is yours | Track |
| `.claude/skills/<id>/`, `.agents/skills/<id>/` | Skill copies for agents set to copy placement | Ignore (regenerated) |
| `harness.toml`, `harness.lock.json` | Shared settings and resolved imports | Track |
| `.harness/state.json`, `.harness/backups/` | Local applied state and backups | Ignore |
| `.harness/local.toml` | Optional gateway token (owner-only permissions on Unix) | Ignore |
| `.artifacts/` (configurable, optional) | Temporary artifacts for talking with the user, not shared; never read back as input. On Windows a new one whose name starts with `.` is also marked hidden; a failure is only reported | Ignore |
| `.gitignore` | A managed block with the ignore rules above; your other rules are kept | Track |

The tool changes existing files only inside managed blocks that start with `agent-harness:begin` and end with `agent-harness:end`. If the markers are damaged, or you edited inside a block, the preview shows a conflict, and the tool never rewrites the whole file. File formats are documented in [`references/HARNESS_STATE_CONTRACT.md`](../references/HARNESS_STATE_CONTRACT.md).

## Skill sources

- **Local folder.** Use a folder inside the project, for example `vendor/skills`. Each subfolder that contains a `SKILL.md` is a skill. Bundled files are copied but never executed. Symlinks that point outside the package are refused. A folder outside the project can't be saved in shared settings, so copy those skills into the project first.
- **Wiki gateway (`md.zerotymer.net`).** The tool reads the gateway `https://md-gw.zerotymer.net` with `GET /api/v2/documents/<uuid>` (gateway API v2).
  - **Token (optional):** reads currently need no token. You can type one in the token row; it is never drawn on screen, and after a successful apply it is saved to `.harness/local.toml`, which Git ignores. A token in the environment variable named in the token variable row (default `GATEWAY_TOKEN`) wins over the typed or saved token. If the gateway answers 401/403, give a token. The token never appears in tracked files, screens or error messages.
  - **Type:** optional Wiki.js path prefix such as `skill` or `claude/hook`.
  - **Fetching:** enter a page UUID and press [조회] (fetch). The network is used only at that moment. The request times out after 15 seconds; the key bar shows 처리 중… (working) and other keys are ignored until the response arrives.
  - **Content:** English and Korean pages become `SKILL.md` and `SKILL_ko.md`, unchanged. If one language is missing, you must accept a partial import. The tool never creates a missing translation.
  - **List:** [목록 불러오기] (load list) shows `wiki_skills` from the project configuration. Picking one fills its UUID, type and skill name; archived and already imported entries are blocked. You can also type a UUID.

## Project configuration

The TUI reads `project.json` from the project root. Without it, it reads `default.json`, which is built into the package (this repository's `catalog/default.json`). The two are never merged, and the TUI never downloads either. The format is in [`references/HARNESS_CATALOG_CONTRACT.md`](../references/HARNESS_CATALOG_CONTRACT.md): `agents`, `skills`, `wiki_skills`, `docs` and `hooks`, plus optional initial `paths` and `service` values.

A file with an unknown key, an unsupported `schema_version`, a duplicate `id`, an option-like value or an empty argument vector is rejected. An invalid `project.json` is shown as an error and the TUI does not fall back to `default.json`; fix the file and run again.

[카탈로그 보기] (view catalogs) on the skills step shows the lists. Public skills show available, unavailable (no `npx`), conflict or manual install. Wiki skills, documents and hooks are listed only here: wiki skills are imported with [목록 불러오기] (load list) on the wiki source, hooks are added with [훅 추가] (add hooks), and downloading documents comes later.

## Agents

The agents come from `agents` in the configuration. The built-in default lists Claude Code (`claude --version`, `npm install -g @anthropic-ai/claude-code@latest`) and Codex (`codex --version`, `npm install -g @openai/codex`).

- **Detection** runs each agent's `detect` command without a shell, all in parallel, with a 5-second timeout. Exit code 0 means installed, and the first output line is shown as the version. [다시 감지] (detect again) repeats it.
- **Program lookup** uses `PATH` only. On Windows it also tries the `PATHEXT` extensions, so `npm` finds `npm.cmd`.
- **Installing:** select the agents that are not detected and press [설치] (install). The confirmation screen shows the full command for each one on this OS. On Enter the TUI hands over the terminal and runs them one by one. When they finish, press Enter to return; the TUI shows each exit code and detects again. Anything other than 0 is a failure.
- These are global npm installs: npm must be on `PATH`, Claude Code needs Node.js 22 or later, and an unwritable npm prefix fails with `EACCES`. The TUI never uses `sudo` or elevation. Shell installers (`curl … | bash`) are not supported.
- An agent with no install command for this OS shows install as unavailable.
- `harness.toml` stores the selected agent IDs. The guidance file and skills directory come from the agent entry. Agents that share a guidance file share one managed block, with each agent's own lines at the bottom under its title.
- An agent ID in `harness.toml` that the configuration no longer lists is kept, shown as unknown, and gets no guidance or skill copies.

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

A `harness.toml` written with schema 1 (with `output`) is still read: `output` becomes `artifacts` with the same value, so an existing `.output/` stays in use. It is rewritten as schema 2 by the next apply, for example when you add a skill or a hook.

When `harness.toml` exists, the tool shows a summary and checks whether the files still match the settings. Paths and agents cannot change here. If the check finds no conflict, these buttons are offered:

- **[스킬 추가] (add skills)** opens the sources and skills screen with the recorded paths and agents: a local folder, a wiki skill (the list button shows `wiki_skills` from the configuration; archived and already imported ones are blocked) or a public skill. Esc returns to the check.
- **[훅 추가] (add hooks)** lists the configuration's `hooks`. The picked IDs are recorded in `harness.toml`, and the guidance gets a "Hooks (reference)" section that tells you to add the hooks you use to the agent's own settings and adjust them there. Nothing is installed or run.
- **[지침 갱신] (refresh guidance)** appears only when a guidance block differs from what the current configuration renders, for example after a newer version of this tool or after `project.json` renamed a hook. It rewrites the managed blocks after the preview; your text outside the blocks is kept. Esc on the preview returns to the check without writing. A block you edited yourself is a conflict, and then no button is offered.

All of them go through the preview and apply as usual and only add; skills already installed cannot be added again, and updating or removing skills comes later. Running the same initialization again never duplicates blocks, skills or ignore rules.

## Recovery

- Before changing an existing file, the tool copies it to `.harness/backups/<apply-id>/`.
- If a write fails, already written files are restored and newly created files and empty folders are removed. Anything that could not be undone is listed, and no success is recorded.
- If `.harness/apply.lock` is left behind after a crash, the tool refuses to apply until you delete it. Delete it only when no other run is active.
- The tool never commits or pushes. If a path it would ignore is already tracked by Git, it tells you to run `git rm --cached` yourself.
