# Step 2: Catalog Integration

## Prerequisite and objective

Start only after [Step 1](STEP_1_INITIAL_CREATION.md) passes its acceptance criteria (accepted 2026-11-20). Add three catalogs to the standalone TUI: Wiki.js pages, public skills, and documents. The user picks entries from a catalog instead of typing UUIDs or install commands. Stage 1 initialization must keep working without any catalog file.

The bounded updater, override layers, modes, hooks, Git worktree, Worktrunk and Docktree moved to [Step 3](STEP_3_ADDITIONAL_FEATURES.md). This instruction does not authorize building or deploying any external service.

## 1. Catalog files

- Three separate JSON files share one envelope: `wiki.json`, `skills.json` and `docs.json`. The format is defined in [HARNESS_CATALOG_CONTRACT.md](HARNESS_CATALOG_CONTRACT.md).
- The distributor owns and ships the catalog files. **The TUI never downloads a catalog.** How a catalog reaches the machine (a script, a manual copy, a release asset) is outside the TUI.
- The TUI looks for each catalog in this order and uses the first one it finds:
  1. The path in the environment variable `AGENT_HARNESS_WIKI_CATALOG`, `AGENT_HARNESS_SKILLS_CATALOG` or `AGENT_HARNESS_DOCS_CATALOG`. A relative path resolves from the project root.
  2. `catalog/<kind>.json` next to the executable.
  3. The sample built into the executable from this repository's `catalog/<kind>.json`.
- The release stays a single executable. Catalog files are optional resources.
- Reject a file with an unknown key, a wrong `kind`, an unsupported `schema_version`, or a duplicate `id`. Show the error. Never fall back silently to another catalog.
- Show where each catalog came from (path or "built-in sample") and its `catalog_version`.
- CI parses the built-in samples with the same Rust types the TUI uses.

## 2. Wiki catalog

- Each entry names a Wiki.js page by `uuid` and its path `type` (for example `skill` or `claude/hook`). The JSON states the type; the TUI does not derive it.
- `target` decides what the TUI does:
  - `skill`: fetch the page through gateway API v2 (`GET /api/v2/documents/<uuid>?type=<type>`) and import it into `.skills/<id>/` exactly as Stage 1 does: `SKILL.md`/`SKILL_ko.md`, partial imports only with consent, and `contentHash` recorded in `harness.lock.json`.
  - `doc`, or no `target`: list only.
- Entries with `status: "archived"` are listed as archived and cannot be imported.
- Compare UUIDs in lowercase.
- The gateway token is optional. The user may enter it in the TUI; it is stored in `.harness/local.toml`, which Git ignores. Without a token the TUI still reads; on 401/403 it asks for one. Never show the token value on screen, in previews or in logs. A token in the variable named by `token_env` takes precedence over the stored one.
- Backlog, not Stage 2: dynamic (fetch on use) and static (persist) modes for non-skill pages.

## 3. Public skills catalog

- Install through the official skills CLI only: `npx skills add` (vercel-labs/skills). Use `npx`; do not offer other package runners.
- **The TUI runs the install only after the user confirms it.** The TUI decides only what to install. The skills CLI owns everything else and asks the user itself: target agents, project or user scope, which skills to pick, and the version given in `source`.
- Build the command from the entry's structured fields. Never take a command string from the catalog, and do not add `-a`, `-g` or `-y`:

  ```text
  npx skills add <source> [-s <skill>]
  ```

  `-s` is left out when `skill` is omitted or `*`.
- Run it as an argument vector without a shell, with the project root as the working directory. On Windows run `npx.cmd`. Do not generate a script file.
- Name conflict check before confirmation: when the entry has a `skill` name, compare it (ignoring case) with the skills the harness has deployed or selected. On a match, show a conflict and do not run the install. When installing every skill (`skill` omitted or `*`), the names are not known in advance, so show the check as unavailable and warn on the confirmation screen. The output of `npx skills add <source> -l` is not used: it is decorated text whose format is not documented (checked 2026-11-23), and `--json` cannot be combined with `-l`.
- Show the full command on the confirmation screen. On confirm, the TUI leaves raw mode and the alternate screen and hands the terminal to the CLI (inherited stdin, stdout and stderr), so the CLI's prompts reach the user directly. When the CLI exits, the TUI waits for Enter, restores its screen and shows the exit code. A non-zero exit is a failure, not a success.
- If `npx` is not found, show the install action as unavailable.
- The skills CLI owns where it installs and how it links. The harness does not take ownership of those files.
- Entries whose `installer` is not `skills-cli` (for example `claude plugin`) show their `install` text for the user to run. The TUI does not run them.

## 4. Documents catalog

- Stage 2 lists document entries only.
- Backlog, not Stage 2: the TUI downloads a document into `references/<filename>`. With no `sha256`, warn and continue. When `sha256` is present and does not match, do not save that file and warn.

## 5. TUI

- Add a catalog view with the three catalogs, their sources and versions, and per-entry state: available, imported, archived, conflict, unavailable.
- Wiki skill imports go through the existing preview and apply flow. Skill installs run only from an explicit confirmation screen.
- Stage 1 behaviour without catalogs stays the same: manual UUID entry and local-directory sources still work.

## 6. Acceptance criteria

- All Stage 1 acceptance criteria still pass.
- With no catalog files, the built-in samples load and no network request is made to load them.
- Environment variable and next-to-executable catalogs take precedence in the stated order; the screen shows which one is used.
- Malformed catalogs (unknown key, wrong `kind`, unsupported version, duplicate `id`, option-like `source` or `skill`) are rejected with a visible error.
- A wiki entry with `target: "skill"` imports through gateway API v2 and records `contentHash`; other entries cannot be imported.
- The gateway works without a token; a stored token is sent, never displayed, and never written to a tracked file.
- Skill installs: the argument vector is correct with and without `skill`, and never contains `-a`, `-g` or `-y`; nothing runs without confirmation; the exit code is reported; a missing `npx` shows as unavailable; a name conflict with a harness deployment blocks the run. Tests use a stub executable instead of the real CLI; the interactive CLI session is checked by hand.
- English and Korean documents stay equivalent.

## Deliverables

The application changes, built-in sample catalogs under `catalog/`, focused tests, bilingual usage documentation, and the catalog contract.
