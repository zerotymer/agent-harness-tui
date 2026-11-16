---
name: git-branch-guideline
description: Naming and usage rules for project Git branches.
title: Git Branch Guideline
version: 1.1.0
language: en
status: active
last_updated: 2026-06-14
---

# Git Branch Guideline

## Predefined Variables

* `{DATE}`: Maps to the current date in `YYYY-MM-DD` format.
* `{DATETIME}`: Maps to the current date and time in `YYYYMMDD-HHmmss` format.
* `{UUID}`: A short UUID, using the first 8 characters of a UUIDv7 value.
* `{UUIDv7}`: A full UUID v7 value.
* `{NAME}`: A descriptive name that represents the purpose or content of the branch.

## Branch Name Rules

* Branch names may include Korean text when it is required for project readability or business context.
* Prefer simple and consistent names whenever possible.
* Avoid characters that commonly cause shell, URL, CI/CD, or filesystem issues.
* Do not use spaces. Use `-` or `_` instead.
* Avoid `:`, `?`, `*`, `[`, `]`, `\`, and consecutive slashes.
* Keep branch names descriptive enough to understand the purpose of the work.
* Add `{UUID}` to work branches when name collisions are possible.

## Branch Principles

* `main`, `staging`, and `dev` are protected baseline branches.
* `main`, `staging`, and `dev` are not regular work branches.
* These branches may be connected to deployment automation for specific environments.
* Do not commit directly to `main` or `staging`.
* Direct commits to `dev` are allowed only in limited cases for development-server validation.
* Use `feature`, `fix`, `hotfix`, `temp`, or `merge` branches for actual work.
* Use pull requests, merge requests, or an equivalent explicit review process when merging into protected baseline branches.
* Apply version tags to `main` and `staging` when a release, deployment, or validation milestone is created.

## Branch Naming

### `main`

`main`: The branch for the final production-ready state.

* Do not merge into this branch arbitrarily.
* Do not commit directly to this branch.
* Unless there is a specific reason otherwise, this branch is the base branch for production releases.
* Apply a version tag when a production release or production-ready milestone is created.

Example tags:

```text
v1.0.0
v1.0.1
release-20260614
```

### `staging`

`staging`: The validation branch used for final testing before production release.

* Do not merge into this branch arbitrarily.
* Do not commit directly to this branch.
* Use this branch for staging-server deployment, final QA, release validation, or pre-production checks.
* Apply a version tag when a staging deployment or validation milestone is created.

Example tags:

```text
staging-v1.0.0
staging-20260614
rc-1.0.0
```

### `dev`

`dev`: The baseline branch used for development-server testing.

* Do not merge into this branch arbitrarily.
* Direct commits are allowed only in limited cases.
* Use this branch for development-server deployment, integration testing, or early validation.
* This branch may be connected to development environment deployment automation.
* Do not use `dev` as a personal work branch.

### `feature`

`feature/{NAME}/{UUID}`: A branch for feature development.

* Usually branch from `main`.
* If the feature must be tested against the development environment first, branch from `dev` may be allowed.
* Add a short UUID to avoid duplicate feature branch names.
* Merge into `dev`, `staging`, or `main` according to the project release flow.

Examples:

```text
feature/user-login/018fc2a1
feature/결제-세션-개선/018fc2a1
```

### `fix`

`fix/{NAME}/{UUID}`: A branch for bug fixes or related corrections.

* Usually branch from the branch where the issue exists.
* For production bugs, branch from `main` unless a different base is explicitly required.
* For staging-only issues, branch from `staging`.
* For development-server issues, branch from `dev`.

Examples:

```text
fix/payment-callback/018fc2a1
fix/삼성브라우저-세션/018fc2a1
```

### `hotfix`

`hotfix/{NAME}/{UUID}`: A branch for urgent production fixes.

* Branch from `main`.
* Use only for urgent issues that must be fixed directly against the production-ready branch.
* After merging into `main`, backport or merge the change into `staging` and `dev` if needed.
* Apply a version tag to `main` after the hotfix is released.
* Apply a version tag to `staging` if the hotfix is also validated through staging.

Examples:

```text
hotfix/login-session-expiry/018fc2a1
hotfix/운영-결제오류/018fc2a1
```

### `temp`

`temp/{DATETIME}-{UUID}`: A temporary branch.

* Usually branch from `main`, unless a different target branch is required.
* Create this branch temporarily before the final work category is clear.
* Do not treat this branch as a long-lived branch.
* Before the final push or review, rename it to an appropriate `feature`, `fix`, or `hotfix` branch, or merge it into one.
* Delete this branch after the work is reorganized or completed.

Example:

```text
temp/20260614-213045-018fc2a1
```

### `merge`

`merge/{NAME}/{UUID}`: A branch for merge validation.

* Use this branch temporarily to test merges.
* Use this branch to check conflicts, run integration tests, and verify combined changes before the actual merge.
* Do not use this branch as a permanent integration branch.
* Do not use this branch to bypass review or protected branch rules.
* Actual merges into `main`, `staging`, or `dev` must follow the project review and approval process.

Examples:

```text
merge/release-1.0.0/018fc2a1
merge/staging-validation/018fc2a1
```

## Recommended Flow

### Normal Feature Flow

```text
main
  └─ feature/{NAME}/{UUID}
       └─ dev
            └─ staging
                 └─ main
```

### Bug Fix Flow

```text
target branch
  └─ fix/{NAME}/{UUID}
       └─ target branch
```

Where `target branch` may be `main`, `staging`, or `dev`.

### Hotfix Flow

```text
main
  └─ hotfix/{NAME}/{UUID}
       └─ main
       └─ staging
       └─ dev
```

### Temporary Work Flow

```text
main
  └─ temp/{DATETIME}-{UUID}
       └─ feature/{NAME}/{UUID}
```

Or:

```text
main
  └─ temp/{DATETIME}-{UUID}
       └─ fix/{NAME}/{UUID}
```

## Tagging Rules

* Apply version tags to `main` for production releases or production-ready milestones.
* Apply version tags to `staging` for staging deployments, release candidates, or validation milestones.
* Tags should be immutable once published.
* If a tag was created incorrectly, create a corrected tag instead of silently moving the old tag, unless the project explicitly allows tag rewriting.

Recommended tag examples:

```text
v1.0.0
v1.0.1
rc-1.0.0
staging-v1.0.0
release-20260614
```
