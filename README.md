# Agent Harness TUI

## Purpose

Build a standalone Rust TUI that configures an agent harness from the current project root. The tool manages instruction, reference, output, and skill paths and generates project-wide agent guidance.

These documents are the implementation instructions for this repository. The application is implemented here, in stage order. Do not initialize Git or register services.

## Document map

| Stage | English instruction | Korean translation | Scope |
| --- | --- | --- | --- |
| 1 | [STEP_1_INITIAL_CREATION.md](references/STEP_1_INITIAL_CREATION.md) | [STEP_1_INITIAL_CREATION_ko.md](references/STEP_1_INITIAL_CREATION_ko.md) | Standalone TUI, initial generation, remote skills, configuration and baseline state |
| 2 | [STEP_2_ADDITIONAL_FEATURES.md](references/STEP_2_ADDITIONAL_FEATURES.md) | [STEP_2_ADDITIONAL_FEATURES_ko.md](references/STEP_2_ADDITIONAL_FEATURES_ko.md) | Bounded updates, overrides, modes, hooks, Git worktree, Worktrunk and Docktree |

Complete and validate Stage 1 before starting Stage 2. Worktrunk belongs to Stage 2 because it manages Git worktrees; it is not a prerequisite for initial harness creation.

## Decisions

- Implementation: Rust, Ratatui and Crossterm. Linux servers are the primary environment; provide native Windows and macOS support.
- The current working directory is the project root. Stage 1 must not redirect execution to a parent repository or primary worktree.
- Track pending and completed instructions, instruction history, references, skills and shared harness configuration in the target project's Git repository.
- Keep generated outputs, optional SQLite backlog data, credentials and machine-local state outside Git.
- Write work instructions in Korean only (`instructions/NNNN_name.md`). Write agent-facing reference, skill and guidance documents in English with equivalent `_ko.md` translations. Preserve imported source documents rather than inventing translations.
- Use maintained stable dependencies. Do not introduce deprecated APIs or compatibility shims for end-of-support operating systems.
- External systems are optional integration candidates, not services to build as part of these instructions.

## Sources and compatibility checks

The following official sources informed the plan on 2026-11-15. Recheck supported versions and integration contracts at implementation time; do not copy legacy event names or configuration examples without verification.

- [Ratatui backends](https://ratatui.rs/concepts/backends/comparison/)
- [Crossterm](https://github.com/crossterm-rs/crossterm)
- [Project AGENTS.md guidance](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
- [Claude Code project guidance](https://code.claude.com/docs/en/memory)
- [Claude Code hooks](https://code.claude.com/docs/en/hooks)
- [Git ignore rules](https://git-scm.com/docs/gitignore)
- [Worktrunk configuration](https://worktrunk.dev/config/)
- [Docktree](https://docktree.dev/)
