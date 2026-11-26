# Stage 1 verification report

> 2026-11-26: the implementation moved to Node.js (TypeScript + Ink, instruction 0025) and the Rust crate was removed. This report is kept as the record of the Rust build that was accepted; the same 11 acceptance checks now run as `src/stage1.test.ts`.

Date: 2026-11-16. Scope: instructions 0001–0011, built on `main` at `bdafb75` plus the 0011 branch. Spec: [`references/STEP_1_INITIAL_CREATION.md`](../references/STEP_1_INITIAL_CREATION.md) §8.

## Environment

| Item | Value |
| --- | --- |
| OS | Rocky Linux 9.8 (kernel 5.14), x86_64 |
| Toolchain | Rust 1.97.1, edition 2024 |
| Key dependencies | ratatui 0.30.2 (crossterm 0.29.0), serde, toml 1.2, serde_json, sha2 0.11, ureq 3 |
| User | Regular user (no administrator rights) |
| Git | 2.52.0 |

## Test results

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all pass. There are 101 tests: 87 unit tests, 3 binary tests and 11 Stage 1 acceptance tests (`tests/stage1.rs`). Every test builds its target project in a temporary directory. Automated tests use no network: the gateway client is tested against a local mock HTTP server.

## Acceptance criteria (spec §8)

| Criterion | Status | Evidence |
| --- | --- | --- |
| A temporary project initializes with a local source, without admin rights or network | Pass | `stage1::initializes_with_local_source_and_no_network` (regular user, local source only) |
| Settings and path resolution survive a re-run | Pass | `stage1::rerun_keeps_config_and_path_resolution`: Korean path with spaces, no absolute paths in `harness.toml`, re-run plans no writes |
| Active and completed instruction policy matches in both languages | Pass | `stage1::english_and_korean_guidance_state_the_same_policy`, `guidance::tests::english_and_korean_share_every_path_and_value` |
| Existing guidance and logs are preserved; repeats add nothing | Pass | `stage1::existing_guidance_and_log_survive_and_repeats_add_nothing`: user text, CRLF, existing log and `.gitignore` rules kept; one block per file |
| Path escapes and collisions are rejected | Pass | `stage1::rejects_path_escapes_and_collisions`, `paths::tests::*` (`..`, symlinks, Windows drive paths, duplicates, nesting, file collisions) |
| Generated guidance stays within its boundary | Pass | `stage1::guidance_edits_inside_the_block_are_conflicts`, `blocks::tests::*` (malformed, duplicate and nested markers are conflicts; no full-file rewrite) |
| Package path escapes are rejected | Pass | `stage1::rejects_package_paths_that_escape`, `sources::tests::local_rejects_escaping_symlinks` |
| Git ignore behavior | Pass | `stage1::git_tracks_shared_files_and_ignores_local_state` and `gitignore::tests::*` against real Git: shared files visible, local state ignored, nothing staged or committed, parent-rule and tracked-path warnings |
| Recovery after an interrupted apply | Pass | `stage1::interrupted_apply_restores_files_and_records_no_success` (real permission failure mid-apply), `apply::tests::injected_failure_restores_files_and_records_nothing` (failure at every write) |
| Agent adapters checked against current official rules; unsupported methods recorded | Pass | See "Agent adapters" |
| Root, path and terminal behavior on Linux, Windows and macOS | Linux only | See "Platforms" |
| No worktree or service management, no executable hooks, no arbitrary SQLite schema | Pass | No such code exists. `agent.db` is only an option and an ignore rule; the acceptance test asserts no database file is created |

Also verified:
- A fresh clone without local state does not overwrite changed files (`stage1::clone_without_local_state_does_not_overwrite`).
- The state Stage 2 needs is recorded (`stage1::state_keeps_what_stage2_needs`).

## Platforms

| Platform | Status | What was checked |
| --- | --- | --- |
| Linux (Rocky 9.8) | Verified | All tests, including Unix-only symlink tests. The real binary ran in a pseudo-terminal (`script`, 120×40): Enter-only initialization, files created, exit code 0, alternate screen left (`ESC[?1049l`) and cursor restored (`ESC[?25h`). Running without a TTY exits with code 2 and writes nothing. |
| Windows | Automated tests pass in CI (run 43979645645) | Clippy, tests and release build on `windows-latest`. Absolute input against the canonical `\\?\C:\` root and case folding was fixed in 0013. Still to check by hand: `attrib +h` on the output folder, console rendering and Korean input. Symlink tests are skipped there |
| macOS | Automated tests pass in CI (run 43979645645) | Clippy, tests and release build on `macos-latest`. Still to check by hand: terminal behavior, case-insensitive file system behavior for duplicate detection, symlink handling under `/private/var` temp paths |

## Agent adapters (checked 2026-11-16, adapter version 1)

| Agent | Guidance file | Skill directory | Copy | Reference | Symlink |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `CLAUDE.md` (+ `CLAUDE_ko.md`) | `.claude/skills/<name>/SKILL.md` | Supported | Guidance only | Unavailable |
| Codex | `AGENTS.md` (+ `AGENTS_ko.md`) | `.agents/skills/<name>/SKILL.md` | Supported | Guidance only | Unavailable |

- **Claude Code** (Claude Code CHANGELOG and skill docs): `SKILL.md` needs `name` and `description` frontmatter.
- **Codex** (learn.chatgpt.com "Build skills" and "Customization overview"): Codex scans `.agents/skills` from the current directory up to the repository root, and `name` and `description` are required. The Codex source also has a `<repo>/.codex/skills` root that the user docs do not mention, so it is not used.
- **Reference placement:** neither agent discovers `.skills/` just because guidance lists it. Reference placement therefore only states policy in the guidance.
- **Symlink placement:** not offered, because it needs extra privileges on Windows and copies work everywhere.
- **Frontmatter:** not parsed (deferred by the user). Copy placement only warns when `SKILL.md` does not start with `---`.

## Wiki gateway (live check 2026-11-16)

The skill `wiki-skill-import` (`14cea96f-f1e2-461e-b9a7-96b5a6f63b67`, `type = "skill"`) was fetched with the token read from the user's environment variable. The variable was given by name only, and the value was never printed.
- Both locales were returned, with no missing locale and no warning.
- The per-locale `contentHash` values equal those in this repository's `.skills/wiki-skill-import/source.json`.
- The SHA-256 of the received bytes equals this repository's `SKILL.md` and `SKILL_ko.md`, which confirms the content is stored unchanged.

## Open preconditions

| Item | Current behavior | Needed |
| --- | --- | --- |
| Gateway catalog list/search | Labeled dummy with fixed entries, no network | A REST contract for listing and searching (user will provide) |
| Frontmatter use | Content stored verbatim; the skill name is typed by the user | A policy (user will decide) |
| Windows and macOS verification | Automated tests pass in CI; manual terminal checks pending | Separate runs (user decision) |
| Codex `.codex/skills` | Not used | Appears in official docs |

## Known limitations

- Fetching a skill is synchronous, so the screen does not update until the response arrives or the 15-second timeout ends.
- One local folder and one token variable name per session. They are fixed after the first skill from that source is added.
- Only local skill folders inside the project are accepted.
- File modes, such as the executable bit, are not carried over when importing skills.
- Any `agent-harness:` text in a managed file that is not an exact marker is treated as a conflict. This is deliberately conservative.
- Apply IDs and timestamps are UTC.
- An initialized project gets verification only. Adding or updating skills is Stage 2.

## State contract for Stage 2

[`references/HARNESS_STATE_CONTRACT.md`](../references/HARNESS_STATE_CONTRACT.md) defines schema version 1. It maps every state item that Stage 2 spec §8 requires to a Stage 1 field: stable IDs, source revisions, managed files and blocks, baseline hashes, copy/reference mappings, adapter versions and the last successful apply. Repository and worktree identity is intentionally not recorded, because Stage 1 uses project-local state only.
