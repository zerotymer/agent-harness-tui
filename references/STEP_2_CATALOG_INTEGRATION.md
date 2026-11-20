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
- **The TUI runs the install only after the user confirms it.** The default scope is the project. When the user chooses the user scope, add `-g`.
- Build the command from the entry's structured fields and the configured agents. Never take a command string from the catalog:

  ```text
  npx skills add <source>[#<ref>] -s <skill> -a <agent>... -y [-g]
  ```

- Run it as an argument vector without a shell, with the project root as the working directory. On Windows run `npx.cmd`.
- Show the full command before running. Afterwards show its output and exit code. A non-zero exit is a failure, not a success.
- If `npx` is not found, show the install action as unavailable.
- The skills CLI owns where it installs and how it links. The harness does not take ownership of those files. If the harness already deployed a skill with the same name to an agent, show a conflict and do not run the install.
- Entries whose `installer` is not `skills-cli` (for example `claude plugin`) show their `install` text for the user to run. The TUI does not run them.
- Check at implementation time whether `#<ref>` pinning works in the CLI. If it does not, show `version` as information only and say that the install is not pinned.

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
- Skill installs: the argument vector is correct for project and user scope; nothing runs without confirmation; the exit code is reported; a missing `npx` shows as unavailable; a name conflict with a harness deployment blocks the run. Tests use a stub executable instead of the real CLI.
- English and Korean documents stay equivalent.

## Deliverables

The application changes, built-in sample catalogs under `catalog/`, focused tests, bilingual usage documentation, and the catalog contract.
