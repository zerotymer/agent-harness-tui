---
name: git-worktree-guideline
description: Naming, path mapping, and usage rules for Git worktrees and project branches.
title: Git Worktree Guideline
version: 1.0.0
language: en
status: active
last_updated: 2026-05-25
---

# Git Worktree Guideline

## Purpose

Use Git worktrees to keep multiple branches checked out at the same time without repeatedly switching the main working directory.

Each branch that is created for work should have a corresponding worktree directory under the parent-level `.worktree` directory.

## Predefined Variables

- `{DATE}`: Maps to the current date in `YYYY-MM-DD` format.
- `{DATETIME}`: Maps to the current date and time in `YYYYMMDD-HHmmss` format.
- `{UUID}`: A short UUID, using the first 8 characters of a UUIDv7 value.
- `{UUIDv7}`: A full UUID v7 value.
- `{NAME}`: A descriptive name that represents the purpose or content of the branch.
- `{BRANCH}`: The full branch name.
- `{WORKTREE_PATH}`: The worktree path mapped from the branch name.

## Directory Layout

The canonical worktree root is:

```bash
../.worktree
```

The canonical mapping is:

```text
{BRANCH} -> ../.worktree/{BRANCH}
```

Examples:

```text
feature/payment-api/a1b2c3d4 -> ../.worktree/feature/payment-api/a1b2c3d4
fix/login-error/e5f6a7b8 -> ../.worktree/fix/login-error/e5f6a7b8
temp/20260525-101500-0197192a -> ../.worktree/temp/20260525-101500-0197192a
merge/release-check/c9d0e1f2 -> ../.worktree/merge/release-check/c9d0e1f2
```

Branch names containing `/` intentionally create nested directories below `../.worktree`. Do not flatten or rename path segments unless a project-specific tool requires it.

## General Worktree Rules

- Create work branches as Git worktrees instead of switching the main checkout whenever possible.
- Keep the main repository checkout clean and stable.
- Use `../.worktree/{BRANCH}` as the default path for every worktree branch.
- Do not create unrelated directories inside `../.worktree`.
- Do not reuse a worktree path for a different branch.
- Do not manually delete a worktree directory unless Git metadata has already been cleaned up.
- Before creating a branch from `main`, update `main` with a fast-forward-only pull.

Recommended preparation:

```bash
git fetch --all --prune
git switch main
git pull --ff-only
mkdir -p ../.worktree
```

## Creating a New Branch Worktree

For a new branch, create the branch and worktree together:

```bash
git worktree add -b "{BRANCH}" "../.worktree/{BRANCH}" "{BASE_BRANCH}"
```

Example:

```bash
git worktree add -b "feature/payment-api/a1b2c3d4" \
  "../.worktree/feature/payment-api/a1b2c3d4" \
  "main"
```

For an existing branch, attach it to a worktree without `-b`:

```bash
git worktree add "../.worktree/{BRANCH}" "{BRANCH}"
```

## Branch Naming

### `main`

`main`: The branch for the final production-ready state.

- Do not merge into this branch arbitrarily.
- Do not commit directly to this branch.
- Unless there is a specific reason otherwise, this branch is the base branch for other branches.
- Usually keep this branch in the main repository checkout, not as a separate worktree.

### `staging`

`staging`: The validation branch used for final testing before production release.

- Do not merge into this branch arbitrarily.
- Do not commit directly to this branch.
- Create a worktree for this branch only when release validation requires a separate checkout.

Worktree path:

```text
../.worktree/staging
```

### `dev`

`dev`: The baseline branch used for development-server testing.

- Do not merge into this branch arbitrarily.
- Direct commits are allowed only in limited cases.
- Use this branch for integration checks, development-server deployment, or shared test environments.

Worktree path:

```text
../.worktree/dev
```

Example:

```bash
git worktree add "../.worktree/dev" "dev"
```

### `feature`

`feature/{NAME}/{UUID}`: A branch for feature development.

- Branch from `main`.
- Add a short UUID to avoid duplicate feature branch names.
- Use this branch for isolated feature implementation.

Worktree path:

```text
../.worktree/feature/{NAME}/{UUID}
```

Example:

```bash
git worktree add -b "feature/payment-api/a1b2c3d4" \
  "../.worktree/feature/payment-api/a1b2c3d4" \
  "main"
```

### `fix`

`fix/{NAME}/{UUID}`: A branch for bug fixes or related corrections.

- Branch from the branch where the issue exists, as described in `git-branch-guideline`.
- Use this branch for defect fixes, small corrections, and production-related adjustments.

Worktree path:

```text
../.worktree/fix/{NAME}/{UUID}
```

Example:

```bash
git worktree add -b "fix/login-error/e5f6a7b8" \
  "../.worktree/fix/login-error/e5f6a7b8" \
  "main"
```

### `temp`

`temp/{DATETIME}-{UUID}`: A temporary branch.

- Usually branch from `main`.
- Create this branch temporarily before starting work when the final branch type is not yet clear.
- Before the final push, rename it to an appropriate `feature` or `fix` branch, or merge it into one.
- After renaming the branch, move the worktree so the path still matches `../.worktree/{BRANCH}`.

Worktree path:

```text
../.worktree/temp/{DATETIME}-{UUID}
```

Example:

```bash
git worktree add -b "temp/{DATETIME}-{UUID}" "../.worktree/temp/{DATETIME}-{UUID}" "main"
```

Rename and move example:

```bash
git -C "../.worktree/temp/{DATETIME}-{UUID}" branch -m "feature/{NAME}/{UUID}"
git worktree move "../.worktree/temp/{DATETIME}-{UUID}" "../.worktree/feature/{NAME}/{UUID}"
```

### `merge`

`merge/{NAME}/{UUID}`: A branch for merge testing or merge execution.

- Use this branch temporarily to test merges.
- Use this branch when pulling `main` and preparing or executing a merge into `main`.
- Delete this branch and remove its worktree after the merge task is complete.

Worktree path:

```text
../.worktree/merge/{NAME}/{UUID}
```

Example:

```bash
git worktree add -b "merge/release-check/c9d0e1f2" \
  "../.worktree/merge/release-check/c9d0e1f2" \
  "main"
```

## Common Commands

List worktrees:

```bash
git worktree list
```

Move into a worktree:

```bash
cd "../.worktree/{BRANCH}"
```

Check worktree status:

```bash
git -C "../.worktree/{BRANCH}" status
```

Push a branch for the first time:

```bash
git -C "../.worktree/{BRANCH}" push -u origin "{BRANCH}"
```

Remove a worktree after the branch is merged or no longer needed:

```bash
git worktree remove "../.worktree/{BRANCH}"
```

Delete the local branch after removing the worktree:

```bash
git branch -d "{BRANCH}"
```

Force-delete only when the branch is intentionally discarded:

```bash
git branch -D "{BRANCH}"
```

Clean stale worktree metadata:

```bash
git worktree prune
```

## Compatibility and Safety Notes

- Prefer lowercase branch names with hyphen-separated words for `{NAME}`.
- Avoid spaces, shell-special characters, and non-portable punctuation in branch names.
- Avoid `:` in branch names and worktree paths because it can be problematic on some filesystems and tools.
- Do not use deprecated or legacy Git workflows that rely on repeatedly switching a single working directory for parallel work when worktrees are available.
- Do not manually edit files under `.git/worktrees`.
- Do not remove a worktree directory with `rm -rf` unless `git worktree remove` cannot be used and the cleanup impact is understood.
- Keep generated files, dependency caches, and environment-specific files out of Git unless they are intentionally versioned.
