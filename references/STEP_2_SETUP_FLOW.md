# Step 2: Setup Flow

## Prerequisite and objective

Start only after [Step 1](STEP_1_INITIAL_CREATION.md) passes its acceptance criteria (accepted 2026-11-20). This stage was redefined on 2026-11-23 (instruction 0022). It replaces the earlier catalog-integration scope.

The TUI sets up a development environment in four steps:

1. Agents: detect installed agents and install missing ones.
2. Paths: choose the standard paths.
3. Guidance: write or update the agent guidance files.
4. Installs: install skills, wiki skills and documents from the project configuration, and connect dynamic skills and references.

The Stage 1 rules stay in force: the current directory is the project root, existing files change only inside managed blocks, every apply has backups, and success is recorded only after every write succeeds.

Most of the environment is shared through Git. Two things are not: environment variables (shared by other means) and content that must not be shared or must be read live. That content comes from the **content service**, a separate program. This repository implements only its client (§7).

Out of scope: worktree and service management, executable hooks, the general harness updater (Stage 3), and any SQLite schema other than the `agent.db` schema the user supplied (issue #21).

## 1. Project configuration

- The TUI reads `project.json` from the project root. The project's owner writes it and tracks it in Git.
- When `project.json` does not exist, the TUI reads `default.json`, which is built into the executable from this repository's `catalog/default.json`.
- The two files are never merged. If `project.json` exists, `default.json` is not read.
- The format is in [HARNESS_CATALOG_CONTRACT.md](HARNESS_CATALOG_CONTRACT.md). It holds the lists `agents`, `skills`, `wiki_skills`, `docs` and `hooks`, and optional initial values for paths and the content service.
- The TUI never downloads the project configuration.
- Reject a file with an unknown key, an unsupported `schema_version` or a duplicate `id`, and show the error. Never fall back silently to `default.json` when `project.json` is invalid.
- Show which file is in use and its `catalog_version`.
- **Install commands are the user's responsibility.** The TUI shows every argument vector from the configuration and runs it only after the user confirms. It runs it without a shell. It never runs a command string.
- The configuration holds no secrets. Tokens are only named by environment variable.
- CI parses the built-in `default.json` with the same validation code the TUI uses.
- **Runtime:** the TUI runs on Node.js 22.13 or later (TypeScript + Ink, 2026-11-24; 22.13 for the built-in `node:sqlite`). Agent installs and skill installs use npm and npx from the same Node.js installation.

## 2. Flow

| Order | Screen | Content |
| --- | --- | --- |
| 1 | Agents | Detect, select, install (§3) |
| 2 | Paths | Standard paths and the `agent.db` option (§4) |
| 3 | Installs | Skills, wiki skills, documents, hooks, and the connection screen for dynamic content (§6, §7) |
| 4 | Preview | Every planned file change, install command and conflict |
| 5 | Apply and result | Writes, guidance files (§5), installs, result |

The project root and the configuration source are shown in the header on every screen.

## 3. Agents

- The agent list comes from `agents` in the project configuration. The built-in `default.json` lists Claude Code and Codex, installed through npm (verified 2026-11-24 against the official docs):

  | Agent | `detect` | `install` (every OS) |
  | --- | --- | --- |
  | Claude Code | `claude --version` | `npm install -g @anthropic-ai/claude-code@latest` |
  | Codex | `codex --version` | `npm install -g @openai/codex` |

- **Main constraints of npm installs:**
  - npm must be on `PATH`. Without it, install shows as unavailable. Claude Code needs Node.js 22 or later.
  - The install is a global npm install. It can fail with `EACCES` when the npm prefix is not writable. The TUI never uses `sudo` or elevation; it reports the exit code, and the user fixes npm or installs another way.
  - Official shell installers (`curl … | bash`, `irm … | iex`) are not supported, because the TUI never runs a shell.
- **Program lookup:** the first element of `detect` and `install` is looked up on `PATH` without a shell. On Windows the lookup also tries the `PATHEXT` extensions, so `npm` and `claude` find `npm.cmd` and `claude.cmd`. Node.js cannot start a `.cmd` or `.bat` file without `cmd.exe`, so only for such a file the TUI runs `cmd.exe /d /s /c <file> <args>`, after checking that no argument holds a space or a `cmd` metacharacter (`& | < > ^ % ! " ( )`). An argument that fails the check is refused before anything runs. It never passes a command string from the configuration and never uses the `shell` option.
- **Detection:** run each agent's `detect` argument vector without a shell and with a timeout of 5 seconds. Agents are detected in parallel so the screen does not freeze, and a process still running at the timeout is killed. Exit code 0 means installed. Show the first line of its output as the version.
- **Selection:** the user may select any detected agent. The user may also select agents that are not detected, to install them. Several agents can be installed in one step.
- The user can leave this step only when at least one selected agent is installed.
- **Install:** the confirmation screen shows the full argument vector for each agent on this OS. On confirm, the TUI hands over the terminal as in §6.1, runs the installs one by one, shows each exit code, and detects again. A non-zero exit code is a failure.
- An agent with no install command for this OS shows install as unavailable.
- `harness.toml` stores the selected agent IDs. The guidance file name and the skills directory come from the agent entry. The IDs `claude-code` and `codex` from Stage 1 stay valid, so this needs no schema change.
- If an agent ID in `harness.toml` is missing from the configuration, keep it in `harness.toml`, show it as unknown, and do not deploy to it.
- There is no separate root screen. The project root is shown in the header.
- An initialized project (`harness.toml` exists), or one whose files or configuration cannot be read, shows a summary and a no-change check in place of the agents screen (decided 2026-11-26). Paths and agents cannot change there. When the check finds no conflict, it offers only two additions (instruction 0026): **add skills** (the sources and skills screen: local folder, wiki skill, public skill) and **add hooks** (§6.4). Additions go through the usual preview and apply and only add: new skills, their copies, hook records, and the matching updates of `harness.toml`, `harness.lock.json` and the managed blocks.
- An invalid `project.json` is shown as an error on that screen; the TUI does not start the flow with `default.json` instead.

## 4. Paths

Every path can be changed by the user. No name format is enforced: the leading `.` in a default only hides the directory.

| Item | Default | Required | Purpose | Git |
| --- | --- | --- | --- | --- |
| Instructions | `instructions/` | Yes | Work instructions | Partly tracked (below) |
| Artifacts | `.artifacts/` | No | Temporary artifacts for talking with the user | Ignored |
| Documents | `docs/` | No | Documents the user publishes, mostly HTML and documents | Tracked |
| References | `references/` | No | Documents for agents: Markdown files or documents the user adds | Tracked |
| Canonical skills | `.skills/` | Fixed | Skill originals, as in Stage 1 | Tracked |

- A disabled optional path is not created, not written to `.gitignore`, and not mentioned in guidance. Features that need it (for example document downloads into references) show as unavailable.
- The Stage 1 safety checks stay: every path is project-relative and inside the root, with no `..` or symlink escape. Managed paths must not duplicate each other (ignoring case), nest inside each other, or collide with an existing entry of another kind.
- The TUI only creates paths. It never moves, renames or deletes a directory, even after a path changes; the old directory stays where it is.
- Hiding a directory is an optional, OS-specific extra. On Windows the TUI tries to set the hidden attribute on a new artifacts directory whose name starts with `.`. A failure is reported as information only and never blocks the apply. Git does not carry the attribute to other clones.

**Fixed names under the instruction root:**

| Item | Path | Git |
| --- | --- | --- |
| Completed instructions | `<instructions>/.completed/` | Tracked |
| Completion log | `<instructions>/completed.log` | Tracked |
| Backlog (optional) | `<instructions>/agent.db` | Ignored (`agent-db.sh`/`agent-db.ps1` are tracked) |

The `agent.db` rules from the Stage 1 spec §2 do not change.

**Git:**

- Active instruction files are ignored. The managed `.gitignore` block ignores every Markdown file directly in the instruction root and keeps `README.md`:
  ```gitignore
  /<instructions>/*.md
  !/<instructions>/README.md
  ```
  `<instructions>` is the configured instruction path (default `instructions`), so the default block holds `/instructions/*.md` and `!/instructions/README.md`. A new project starts with the artifacts path `.artifacts/`, which the user can change like every other path. The pattern does not reach subdirectories, so `.completed/`, `completed.log`, `.gitkeep` and the scripts stay tracked. The instruction rules generated in guidance say the same thing.
- The artifacts directory and `agent.db*` are ignored.
- **`.gitkeep`:** it exists only so Git keeps new empty directories. When the TUI creates a tracked directory, it also creates an empty `.gitkeep` in it. That covers `<instructions>/`, `<instructions>/.completed/`, documents and references. It never creates one in an ignored directory. Directories that already exist are not touched. It is create-only: an existing `.gitkeep` is left alone, and the file is not recorded as owned, like an empty `completed.log`.

**Migration:** a schema 1 `harness.toml` is still read. Its `paths.output` becomes `paths.artifacts` with the same value, so an existing `.output/` stays in use and stays ignored. The next successful apply writes schema 2, with the usual backup. See [HARNESS_STATE_CONTRACT.md](HARNESS_STATE_CONTRACT.md).

## 5. Guidance

- After the agents and paths are applied, the TUI writes or updates each selected agent's guidance file. The file name comes from the agent entry: for example `CLAUDE.md` for Claude Code and `AGENTS.md` for Codex (plural `AGENTS.md`, never `AGENT.md`). Each file has a Korean `_ko.md` pair.
- Agents with the same guidance file are treated as one: the file gets one managed block. Content specific to one agent, if any, is added at the bottom of that block under the agent's title.
- The block lists the enabled paths and their purpose, the instruction rules, the Git policy, the skills, and the dynamic skills and references with how to read them (§7).
- Existing user text outside the block is kept, as in Stage 1.
- **Refresh** (instruction 0027, 2026-11-26): on an initialized project, when the check finds no conflict but a guidance block differs from what the current configuration renders (a newer TUI, a renamed hook), the check screen offers [지침 갱신] (refresh guidance). It shows the preview and, after confirmation, rewrites the managed blocks only. Esc on the preview returns to the check without writing. A block the user edited is a conflict and is never rewritten.

## 6. Installs

### 6.1 Skills (`skills`)

- The Stage 2 skill install rules written on 2026-11-23 stay (instruction 0021): `npx skills add <source> [-s <skill>]`, no `-a`, `-g` or `-y`, an argument vector without a shell, `npx.cmd` on Windows, the name conflict check, and the full command on the confirmation screen.
- **Terminal handover:** on confirm, the TUI leaves raw mode and the alternate screen and runs the command with inherited stdin, stdout and stderr. When it exits, the TUI waits for Enter, restores its screen and shows the exit code. Agent installs (§3) use the same handover.
- **Updates:** the TUI offers an update action for installed skills through the skills CLI. The CLI's update command and its scope are verified against the official documentation before implementation. The update runs only after confirmation. There is no background scheduler.
- Entries with an `installer` other than `skills-cli` show their `install` text for the user to run.

### 6.2 Wiki skills (`wiki_skills`, static)

- The TUI fetches the page through gateway API v2 (`GET /api/v2/documents/<uuid>?type=<type>`) and stores it in `.skills/<id>/`, as in Stage 1: `SKILL.md`/`SKILL_ko.md`, partial imports only with consent, and `contentHash` recorded in `harness.lock.json`.
- Entries with `status: "archived"` cannot be imported.
- The gateway token is optional and stored in `.harness/local.toml`. A token in the variable named by `token_env` takes precedence. It is never shown on screen, in previews or in logs.

### 6.3 Documents (`docs`)

- The TUI downloads the document over `https://` into `<references>/<filename>`. References must be enabled.
- With no `sha256`, warn and continue. When `sha256` is present and does not match, do not save the file, and warn.
- An existing file with different content is a conflict. It is never overwritten.
- The download is recorded in `harness.lock.json`.
- **Flow** (instruction 0028, 2026-11-27): documents are picked in the catalog view, which is also reachable while adding to an initialized project. Nothing is fetched while picking; the picked documents are downloaded just before the preview (with an explicit `User-Agent` and a 30 s timeout). A failed download or a `sha256` mismatch is shown on the preview and left out; the rest still applies. A document already in the lock cannot be picked again; updating or removing downloads comes later.

### 6.4 Hooks (`hooks`)

- Hooks are reference entries only: a title, a summary, and an optional link to a document. The TUI lists them and installs nothing.
- For now the list is a placeholder. The built-in `default.json` holds one sample entry.
- **Adding hooks** (instruction 0026, 2026-11-26): the user picks entries; their IDs are recorded in `harness.toml` `[[hooks]]`, and the guidance block gets a "Hooks (reference)" section with each title, summary and link plus a note to add the hooks in use to the agent's own settings and adjust them there. Nothing is installed or run. An ID the configuration no longer lists is kept and shown as unknown.
- On the wiki source, the list button shows `wiki_skills` from the configuration. Picking one fills its UUID, type and name; archived and already imported entries are blocked.

## 7. Dynamic skills and references (content service client)

Dynamic content lives only in the content service. The project holds only stubs and lists. Agents read the body every time they use it.

- **Connection settings:** `harness.toml` `[service]` holds `url`, `project` and `token_env`, shared through Git. `project.json` may give initial values.
- **Token:** the variable named by `token_env` is used first, then `.harness/local.toml` `[service] token`. The token is never shown on screen, in previews or in logs, and never written to a tracked file.
- **URL rule:** `https://` is required, except `http://` to a loopback address (`localhost`, `127.0.0.0/8`, `::1`).
- **Connection screen:** edit the URL, the project ID and the token, then run a connection test. It checks health first, then authorized access. Show connected, unauthorized, unreachable and incompatible version as separate states.
- **Service contract:** the endpoints, the MCP tool names and the response formats belong to the content service project. Implement them from that project's published contract; never guess them. The TUI needs these operations: health and contract version, list dynamic skills (name, description), and list dynamic references (path, title).
- **Dynamic skill stub:** for each selected dynamic skill and each selected agent, the TUI writes `SKILL.md` into the agent's skills directory. The frontmatter `name` and `description` come from the service list. The body tells the agent to read the current body from the service (MCP or REST) before following it, and to report a failure to the user instead of guessing. The stub holds no body content. It is recorded as a deployment.
- **Untracked stubs:** when a skill's name or description must not be shared, the user can mark it untracked. Its stubs are then added to the managed `.gitignore` block.
- **Dynamic references:** the guidance block lists the selected references and how to read them. No body is stored.
- **Name conflicts:** a dynamic skill with the same name (ignoring case) as a static skill or a harness-deployed skill is a conflict. Neither is deployed until the user picks one.
- **MCP registration:** the connection screen registers the service as an MCP server in each selected agent's project MCP configuration (for example Claude Code `.mcp.json`). The token is written only as an environment variable reference, never as a value. In JSON files the tool owns only its own server entry, recorded in state, and a user change to that entry is a conflict. The file formats are verified against official documentation before implementation. An agent whose format is not verified shows manual instructions instead.
- **No cache:** the TUI never writes dynamic bodies to disk. If the service is unreachable during setup, the lists show as unavailable and existing stubs are left unchanged.

## 8. TUI

- The installs screen shows each item's state: available, installed, imported, archived, conflict, unavailable.
- Install commands run only from an explicit confirmation screen.
- The preview shows every file change, every command, the configuration source, and conflicts.

## 9. Acceptance criteria

- All Stage 1 acceptance criteria still pass, except the dot-prefix rule for the output path, which this stage removes (issue #36).
- `project.json` is used when present. Otherwise the built-in `default.json` is used, with no network request. They are never merged.
- Malformed configurations are rejected with a visible error. This covers an unknown key, an unsupported version, a duplicate `id`, an option-like value and an empty argument vector.
- Agents: detection works with a stub executable. The flow cannot continue until at least one selected agent is installed. Nothing is installed without confirmation, several installs run in one step, and exit codes are reported.
- Paths: any name is accepted, and the safety checks still reject escapes, duplicates and nesting. Disabled paths are not created. `.gitkeep` is created only in new tracked directories and never overwritten. The `.gitignore` block ignores Markdown files directly in the instruction root except `README.md`, and nothing below it. Directories are never moved or deleted.
- A schema 1 `harness.toml` is read, and `output` becomes `artifacts` with its value kept.
- Guidance files follow each agent entry and keep user text.
- Skills: the Stage 2 (0021) install tests still pass. The update action runs only after confirmation.
- Documents: an `sha256` mismatch is not saved, and an existing different file is not overwritten.
- Dynamic content: tokens are never displayed or written to tracked files. Stubs contain no body. A name conflict blocks deployment. An untracked stub is ignored by Git. MCP registration writes only an environment variable reference. Tests use a stub service.
- English and Korean documents stay equivalent.

## Deliverables

The application changes, `catalog/default.json`, focused tests, bilingual usage documentation, and the updated configuration and state contracts.

## To verify before implementation

- The skills CLI update command and its scope.
- The project MCP configuration format for each agent.
- The content service contract v1, from the service project.
