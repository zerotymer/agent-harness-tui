---
name: git-worktree-guideline
description: Git worktree와 프로젝트 브랜치의 이름, 경로 매핑, 사용 규칙.
title: Git Worktree 가이드라인
version: 1.0.0
language: ko
source_language: en
source_file: SKILL.md
status: active
last_updated: 2026-05-25
---

# Git Worktree 가이드라인

## 목적

Git worktree를 사용하여 메인 작업 디렉터리에서 브랜치를 반복해서 전환하지 않고, 여러 브랜치를 동시에 체크아웃할 수 있게 한다.

작업을 위해 생성하는 각 브랜치는 상위 수준의 `.worktree` 디렉터리 아래에 대응되는 worktree 디렉터리를 가져야 한다.

## 사전 정의 변수

- `{DATE}`: 현재 날짜를 `YYYY-MM-DD` 형식으로 매핑한다.
- `{DATETIME}`: 현재 날짜와 시각을 `YYYYMMDD-HHmmss` 형식으로 매핑한다.
- `{UUID}`: UUIDv7 값의 앞 8자리를 사용한 짧은 UUID.
- `{UUIDv7}`: UUID v7 전체 값.
- `{NAME}`: 브랜치의 목적 또는 내용을 나타내는 설명형 명칭.
- `{BRANCH}`: 전체 브랜치 이름.
- `{WORKTREE_PATH}`: 브랜치 이름에서 매핑된 worktree 경로.

## 디렉터리 구조

표준 worktree 루트는 다음과 같다.

```bash
../.worktree
```

표준 매핑 규칙은 다음과 같다.

```text
{BRANCH} -> ../.worktree/{BRANCH}
```

예시:

```text
feature/payment-api/a1b2c3d4 -> ../.worktree/feature/payment-api/a1b2c3d4
fix/login-error/e5f6a7b8 -> ../.worktree/fix/login-error/e5f6a7b8
temp/20260525-101500-0197192a -> ../.worktree/temp/20260525-101500-0197192a
merge/release-check/c9d0e1f2 -> ../.worktree/merge/release-check/c9d0e1f2
```

`/`를 포함하는 브랜치 이름은 의도적으로 `../.worktree` 아래에 중첩 디렉터리를 만든다. 프로젝트별 도구가 요구하지 않는 한 경로 구간을 평탄화하거나 다른 이름으로 변경하지 않는다.

## 일반 Worktree 규칙

- 가능한 경우 메인 체크아웃에서 브랜치를 전환하지 말고 Git worktree로 작업 브랜치를 생성한다.
- 메인 저장소 체크아웃은 깨끗하고 안정적인 상태로 유지한다.
- 모든 worktree 브랜치의 기본 경로는 `../.worktree/{BRANCH}`를 사용한다.
- `../.worktree` 내부에 관련 없는 디렉터리를 생성하지 않는다.
- 서로 다른 브랜치가 같은 worktree 경로를 재사용하지 않는다.
- Git 메타데이터가 정리되지 않은 상태에서 worktree 디렉터리를 수동으로 삭제하지 않는다.
- `main`에서 브랜치를 생성하기 전에는 fast-forward-only 방식으로 `main`을 갱신한다.

권장 준비 절차:

```bash
git fetch --all --prune
git switch main
git pull --ff-only
mkdir -p ../.worktree
```

## 새 브랜치 Worktree 생성

새 브랜치는 브랜치와 worktree를 함께 생성한다.

```bash
git worktree add -b "{BRANCH}" "../.worktree/{BRANCH}" "{BASE_BRANCH}"
```

예시:

```bash
git worktree add -b "feature/payment-api/a1b2c3d4" \
  "../.worktree/feature/payment-api/a1b2c3d4" \
  "main"
```

이미 존재하는 브랜치에는 `-b` 없이 worktree를 연결한다.

```bash
git worktree add "../.worktree/{BRANCH}" "{BRANCH}"
```

## 브랜치 네이밍

### `main`

`main`: 최종 운영 가능 상태를 위한 브랜치.

- 임의로 병합하지 않는다.
- 직접 커밋하지 않는다.
- 특별한 사유가 없는 한 다른 브랜치의 기준 브랜치가 된다.
- 일반적으로 별도 worktree가 아니라 메인 저장소 체크아웃에 유지한다.

### `staging`

`staging`: 운영 반영 전 최종 테스트를 위한 검증 브랜치.

- 임의로 병합하지 않는다.
- 직접 커밋하지 않는다.
- 릴리스 검증을 위해 별도 체크아웃이 필요할 때만 worktree를 생성한다.

Worktree 경로:

```text
../.worktree/staging
```

### `dev`

`dev`: 개발 서버 테스트를 위한 기준 브랜치.

- 임의로 병합하지 않는다.
- 직접 커밋은 제한적으로 허용한다.
- 통합 확인, 개발 서버 배포, 공유 테스트 환경에 사용한다.

Worktree 경로:

```text
../.worktree/dev
```

예시:

```bash
git worktree add "../.worktree/dev" "dev"
```

### `feature`

`feature/{NAME}/{UUID}`: 기능 개발을 위한 브랜치.

- `main`에서 분기한다.
- 기능 브랜치명이 중복되지 않도록 짧은 UUID를 추가한다.
- 독립적인 기능 구현에 사용한다.

Worktree 경로:

```text
../.worktree/feature/{NAME}/{UUID}
```

예시:

```bash
git worktree add -b "feature/payment-api/a1b2c3d4" \
  "../.worktree/feature/payment-api/a1b2c3d4" \
  "main"
```

### `fix`

`fix/{NAME}/{UUID}`: 버그 수정 또는 관련 보정을 위한 브랜치.

- `git-branch-guideline`에 따라 문제가 발생한 브랜치에서 분기한다.
- 결함 수정, 작은 보정, 운영 관련 조정에 사용한다.

Worktree 경로:

```text
../.worktree/fix/{NAME}/{UUID}
```

예시:

```bash
git worktree add -b "fix/login-error/e5f6a7b8" \
  "../.worktree/fix/login-error/e5f6a7b8" \
  "main"
```

### `temp`

`temp/{DATETIME}-{UUID}`: 임시 브랜치.

- 주로 `main`에서 분기한다.
- 최종 브랜치 유형이 명확하지 않을 때 작업 전에 임시로 생성한다.
- 최종 push 전에 적절한 `feature` 또는 `fix` 브랜치로 이름을 변경하거나 해당 브랜치에 병합한다.
- 브랜치 이름을 변경한 뒤에는 worktree 경로도 `../.worktree/{BRANCH}` 규칙에 맞게 이동한다.

Worktree 경로:

```text
../.worktree/temp/{DATETIME}-{UUID}
```

예시:

```bash
git worktree add -b "temp/{DATETIME}-{UUID}" "../.worktree/temp/{DATETIME}-{UUID}" "main"
```

브랜치 이름 변경 및 worktree 이동 예시:

```bash
git -C "../.worktree/temp/{DATETIME}-{UUID}" branch -m "feature/{NAME}/{UUID}"
git worktree move "../.worktree/temp/{DATETIME}-{UUID}" "../.worktree/feature/{NAME}/{UUID}"
```

### `merge`

`merge/{NAME}/{UUID}`: 병합 테스트 또는 병합 실행을 위한 브랜치.

- 임시 병합 테스트에 사용한다.
- `main`을 당겨와 병합을 준비하거나 `main`으로 병합을 실행할 때 사용한다.
- 병합 작업이 완료되면 이 브랜치를 삭제하고 worktree를 제거한다.

Worktree 경로:

```text
../.worktree/merge/{NAME}/{UUID}
```

예시:

```bash
git worktree add -b "merge/release-check/c9d0e1f2" \
  "../.worktree/merge/release-check/c9d0e1f2" \
  "main"
```

## 자주 사용하는 명령

worktree 목록 확인:

```bash
git worktree list
```

worktree로 이동:

```bash
cd "../.worktree/{BRANCH}"
```

worktree 상태 확인:

```bash
git -C "../.worktree/{BRANCH}" status
```

브랜치 최초 push:

```bash
git -C "../.worktree/{BRANCH}" push -u origin "{BRANCH}"
```

브랜치가 병합되었거나 더 이상 필요하지 않을 때 worktree 제거:

```bash
git worktree remove "../.worktree/{BRANCH}"
```

worktree 제거 후 로컬 브랜치 삭제:

```bash
git branch -d "{BRANCH}"
```

브랜치를 의도적으로 폐기할 때만 강제 삭제:

```bash
git branch -D "{BRANCH}"
```

오래된 worktree 메타데이터 정리:

```bash
git worktree prune
```

## 호환성 및 안전 참고 사항

- `{NAME}`에는 소문자와 하이픈으로 구분한 단어를 사용하는 것을 권장한다.
- 브랜치 이름에는 공백, 셸 특수 문자, 이식성이 낮은 문장 부호를 피한다.
- `:` 문자는 일부 파일시스템 및 도구에서 문제가 될 수 있으므로 브랜치 이름과 worktree 경로에 사용하지 않는다.
- worktree를 사용할 수 있는 환경에서는 병렬 작업을 위해 단일 작업 디렉터리에서 브랜치를 반복 전환하는 deprecated 또는 legacy 방식의 Git 워크플로를 사용하지 않는 것을 권장한다.
- `.git/worktrees` 아래의 파일을 수동으로 편집하지 않는다.
- `git worktree remove`를 사용할 수 없고 정리 영향을 이해한 경우가 아니라면 `rm -rf`로 worktree 디렉터리를 제거하지 않는다.
- 생성 파일, 의존성 캐시, 환경별 파일은 의도적으로 버전 관리하는 경우가 아니라면 Git에 포함하지 않는다.
