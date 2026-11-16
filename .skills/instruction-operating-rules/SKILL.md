---
name: instruction-operating-rules
description: Rules for creating, running, completing, archiving, and logging project instructions under instructions/
title: instructions/ Work Instruction Operating Rules
tags: instruction.md, instructions, workflow
---

# instructions/ — Work Instruction Operating Rules

> This document defines the operating rules that **all subsequent work in this project must reference**.
> Any agent or operator executing an instruction must read this document before starting work.

## 1. Purpose

`instructions/` is the space for storing and tracking **work instructions** used to implement a project incrementally.

Each instruction defines one executable unit of work. When completed, the instruction is moved to the archive directory and recorded in the completion log.

## 2. Directory Structure

```text
instructions/
├── README.md          # This document: work operating rules — changes affect everyone
├── completed.log      # Accumulated completion log: UUID | summary | timestamp
├── NNNN_name.md       # Pending or in-progress instruction (4-digit sequence + summary)
└── .completed/        # Archive directory for completed instructions
    └── <UUID>.md
```

## 3. Instruction File Rules

- File name: `NNNN_short-name.md`. `NNNN` is a 4-digit sequence starting from `0001`, and `short-name` is a kebab-case or snake_case identifier that summarizes the instruction.
- One file represents one complete unit of work. The sequence number indicates the recommended execution order. Dependencies must be written in each instruction's `Prerequisites` section.
- Completed instructions do not keep their original file names. Move them to `instructions/.completed/<UUID>.md`.
- All prose and communication must be written in Korean. Code, comments, and identifiers must be written in English, following the global convention.

### Standard Instruction Structure

```markdown
# NNNN — <Title>

> Entity UUID: <uuid>   (generated locally)
> Status: TODO | IN_PROGRESS | DONE
> Branch: feature/<name>/<uuid>   (fill in when work starts)

## Goal
<What to do and why>

## Prerequisites
<Instruction numbers or external dependencies that must already be completed>

## Scope
- Included:
- Excluded:

## Work Items
1. ...

## Deliverables
- <File/document/endpoint/etc. + UUID for each deliverable>

## Definition of Done (DoD)
- [ ] Tests included
- [ ] Test exception reason stated (when tests are unnecessary, such as documentation, configuration, or non-functional changes)
- [ ] Guardrails confirmed (§7)
- [ ] Work completed on a branch and PR prepared (no direct push to main)

## Guardrail Check
<Confirm that this work does not touch exclusions or prohibited areas from §7>
```

## 4. Instruction Lifecycle

1. **Create**: Write the instruction as `instructions/NNNN_name.md`. Set status to `TODO`. Assign a UUID because this is a new work unit (§5).
2. **Start**: Change status to `IN_PROGRESS`, create a branch (§6), and write the branch name in the instruction. Update the session record (§8).
3. **Complete**: After all completion criteria are met, change status to `DONE`, move the file to `instructions/.completed/<UUID>.md`, and append one line to `completed.log`.
4. **Change or Resume**: If a completed instruction needs additional work, create a follow-up instruction with a new sequence number. Preserve the completion record. Reuse or update the UUID if it represents the same unit.

### completed.log Format

Use tab or pipe separators. Record one completed instruction per line. Append newest entries at the bottom.

```text
UUID | summary | timestamp(YYYY-MM-DD HH:MM:SS +0900)
```

## 5. UUID Identifier Assignment (Required)

- Assign a stable UUID to every instruction, and to any other unit that needs a stable reference.
- Generate it locally before work starts, for example with `python3 -c "import uuid;print(uuid.uuid4())"`. No external registry is used.
- UUIDs are **immutable**. If the same unit changes, keep its UUID. Do not create a new one.
- Write assigned UUIDs into the related instruction, reference documents and code as headers or comments.

## 6. Branch Strategy (Required for Feature Additions and Changes)

> Skill: `.skills/git-branch-guideline` (UUID `e03f48fb-3e00-41d7-b99d-c32854567d67`)

- Direct commits or pushes to `main` and `staging` are **prohibited**. Feature work must always be done on a branch and prepared as a PR.
- Feature branches: `feature/<name>/<uuid>` from `main`.
- Bugfix branches: `fix/<name>/<uuid>`.
- Temporary branches: `temp/<YYYYMMDD-HHmmss>-<uuid>`.
- Use the first 8 characters of the instruction UUID as `<uuid>` to link the instruction and branch.
- Finish work in a PR-ready state. Feature PRs must include tests. If tests are unnecessary, such as for documentation, configuration, or non-functional changes, state the exception reason.

## 7. Scope Guardrails (Strictly Required)

These rules apply to all instructions, code, and proposals.

- **Prohibited**: IP hiding or anonymization, bypassing access restrictions, bypassing authentication or payment barriers, evading detection or blocking, traffic distribution for evasion purposes, unauthorized collection from third-party equipment, and automation that violates terms of service or law.
- **Required**: Respect `robots.txt` and terms of service, apply per-domain rate limits, prevent SSRF by blocking localhost/private/link-local IPs and cloud metadata endpoints, revalidate IPs after redirects, enforce a port allowlist, use TLS and mutual identity verification for all node communication, allow only commands from an allowlist, prohibit arbitrary code execution on nodes, and audit-log administrative actions.
- If an instruction would weaken or violate these guardrails, do **not** implement it. Report the issue first.

## 8. Session Record (Required When Starting a New Session)

> Skill: `.skills/project-session` (UUID `b3230cd0-6081-4570-9bda-183543f585e9`)

- When a new agent session starts, create or update `.claude/session` for current session details and `.claude/session.log` (`.codex/session*` for Codex) with one line in the format `<session-id> - <topic>`.
- These files are **local runtime state** and must not be committed to Git. They are handled by `.gitignore`.

## 9. Specifications

When implementing, the authoritative specifications are in `references/`.
