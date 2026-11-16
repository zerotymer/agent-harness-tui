# CLAUDE.md (한국어 번역)

이 파일은 이 리포지토리에서 작업하는 Claude Code(claude.ai/code)를 위한 안내입니다. 원본은 `CLAUDE.md`이며, 두 파일은 함께 수정합니다.

## 현재 상태

1단계 구현을 마쳤고 사용자 인수를 기다리는 중입니다(`docs/STAGE1_REPORT_ko.md` 참고). 이 리포지토리에는 에이전트 하네스를 구성하는 독립형 Rust TUI(Ratatui + Crossterm)의 명세(SDD)와, 이를 구현하는 Rust 크레이트 `agent-harness-tui`가 들어 있습니다. 리포지토리 자체도 하네스가 관리하는 구조와 비슷한 구조를 따릅니다.

- 빌드: `cargo build`
- 테스트: `cargo test` (테스트는 임시 디렉터리에 대상 프로젝트를 만들어 검증합니다)
- 린트: `cargo clippy --all-targets -- -D warnings`
- 포맷: `cargo fmt --check`

- `README.md` / `README_ko.md`: 목적, 설계 결정, 구현 전에 다시 확인해야 할 공식 출처.
- `references/STEP_1_INITIAL_CREATION*.md`: 1단계, 초기 하네스 생성.
- `docs/USAGE*.md`: 설치와 사용. `docs/STAGE1_REPORT*.md`: 1단계 검증 보고.
- `references/STEP_2_ADDITIONAL_FEATURES*.md`: 2단계, 범위가 정해진 업데이터, 오버라이드 계층, 모드, 훅, Git worktree/Worktrunk/Docktree 지원.

명세가 기준입니다. 가져온 스킬이 명세나 다른 스킬과 충돌하면, 필요한 경우 사용자에게 확인한 뒤 명세(두 언어 모두)나 `.skills/`의 로컬 스킬 사본을 고칩니다. 로컬 스킬을 고쳤다면 그 스킬의 `source.json`에 있는 `localChanges`에 기록합니다. 위키 원문은 절대 수정하지 않습니다. 원문이 바뀌면 사용자가 알려주며, 다시 가져올 때는 로컬 수정을 다시 적용해야 합니다.

## 이 리포지토리의 작업 규칙

- **단계 순서:** 1단계가 인수 조건을 통과하기 전에는 2단계 작업을 시작하지 않습니다. 1단계에는 worktree/서비스 관리, 런타임 훅 실행, 범용 업데이터, SQLite 스키마가 들어가면 안 됩니다. 다만 2단계 업데이터가 쓸 상태는 1단계에서 저장해 두어야 합니다.
- **언어:**
  - 작업 지시서는 한국어로만 작성합니다.
  - 에이전트가 참고하는 문서(참조 문서, 스킬, 이 파일 같은 지침)는 영어 `{name}.md`와 같은 내용의 한국어 `{name}_ko.md`로 짝을 이루며, 둘을 함께 수정합니다.
  - 외부에서 가져온 원본 문서의 번역은 지어내지 않습니다.
- **외부 계약을 지어내지 않음:** `md.zerotymer.net`, 에이전트 훅 계약, Worktrunk(`.config/wt.toml`), Docktree의 엔드포인트나 이벤트 이름을 추측하지 않습니다. 먼저 최신 공식 문서로 확인합니다. 지원하지 않는 기능은 사용 불가로 표시하고, 호환용 shim은 절대 추가하지 않습니다. 예외는 1단계 명세 §4가 허용하는 게이트웨이 목록·검색 더미뿐입니다.
- **Git 정책:**
  - `instructions/`(`.completed/`와 `completed.log` 포함), `references/`, `.skills/`, 루트 안내 문서는 추적합니다.
  - `.output/`, `.harness/`, `agent.db*`, 에이전트 세션 파일(`.claude/session*`, `.codex/session*`), 자격 증명은 무시합니다. `.output/`은 단방향 산출물이며 다시 입력으로 읽지 않습니다.
  - 새로운 로컬 상태나 산출물 경로가 생기면 `.gitignore`도 함께 갱신합니다.
  - 사용자 대신 commit이나 push를 하지 않습니다.
- **지시서 생명주기** (`instruction-operating-rules` 스킬을 따름):
  - 지시서 하나당 파일 하나(`instructions/NNNN_짧은-이름.md`)를 씁니다. 머리말에는 UUID와 상태(`TODO` → `IN_PROGRESS` → `DONE`)를 적습니다.
  - 지시서는 사용자가 명시적으로 완료라고 할 때만 완료입니다. 세션 종료나 에이전트의 응답은 완료로 보지 않습니다.
  - 완료되면 파일을 `instructions/.completed/<UUID>.md`로 옮기고, `instructions/completed.log`에 `UUID | 내용 | 일시(+0900)` 한 줄을 추가합니다.
  - `completed.log`는 추가만 가능합니다. 잘라내거나, 고쳐 쓰거나, 순서를 바꾸지 않습니다.
- **브랜치:** `git-branch-guideline`을 따릅니다. 예를 들어 `feature/{NAME}/{UUID}` 형식에 지시서 UUID를 씁니다. worktree는 `../.worktree/{BRANCH}`에 둡니다(`git-worktree-guideline` 참고). `main`과 `staging`에는 직접 커밋하지 않습니다.

## 제품 요구사항 (TUI가 대상 프로젝트에 적용하는 규칙)

`references/`의 명세는 TUI가 실행되는 대상 프로젝트에 생성·적용하는 규칙을 설명합니다. 비슷해 보여도 이 리포지토리의 작업 규칙이 아닙니다. 제품 동작으로 구현하고 임시 대상 프로젝트에서 테스트하며, 이 리포지토리의 `instructions/`, `.gitignore`, 지침 파일을 테스트 픽스처로 쓰지 않습니다. 예:

- **프로젝트 루트:** 현재 작업 디렉터리가 프로젝트 루트입니다. 상위 디렉터리를 탐색하거나, 루트를 바꾸거나, `git init`을 자동으로 실행하지 않습니다.
- **관리 블록 편집:** 기존 사용자 파일은 경계가 정해진 관리 블록 안에서만 수정합니다. 마커가 잘못되었으면 충돌로 처리하고, 파일 전체를 덮어쓰지 않습니다. 모든 쓰기가 성공한 뒤에만 성공을 기록합니다.

## 스킬

정식 스킬은 `.skills/<name>/`에 `SKILL.md`와 `SKILL_ko.md` 한 쌍으로 둡니다. 모두 `wiki-skill-import`로 `md.zerotymer.net`에서 UUID로 가져왔고, 출처 UUID·수정 시각·해시는 각 스킬의 `source.json`에 있습니다. Claude Code는 `.skills/`를 자동으로 찾지 못하므로, 관련 주제가 나오면 해당 `SKILL.md`를 직접 읽습니다.

스킬을 가져오면 이 표와 `CLAUDE.md`의 표에 행을 추가하거나 갱신합니다. 이미 있는 UUID라면 새 행을 만들지 않고 기존 행을 갱신합니다.

| UUID | 이름 | 용도 |
| --- | --- | --- |
| 14cea96f-f1e2-461e-b9a7-96b5a6f63b67 | wiki-skill-import | 읽기 전용 `md-gw` MCP gateway를 통해 Wiki.js의 스킬과 문서(영어·한국어)를 UUID로 읽는다. curl이나 HTML scraping은 절대 쓰지 않는다. |
| e03f48fb-3e00-41d7-b99d-c32854567d67 | git-branch-guideline | 브랜치 이름과 사용 규칙. `main`/`staging`/`dev`는 보호 브랜치이고, `feature`/`fix`/`hotfix`/`merge`는 `{NAME}/{UUID}`, `temp`는 `{DATETIME}-{UUID}` 형식을 쓴다. 태그 규칙도 포함한다. |
| 5c502e34-1daf-44d7-b740-1baa03e583e5 | git-worktree-guideline | worktree 배치 규칙. 브랜치마다 `../.worktree/{BRANCH}`에 체크아웃하며, worktree 생성·이름 변경·제거 명령을 담고 있다. |
| b3230cd0-6081-4570-9bda-183543f585e9 | project-session | 세션 시작 시 프로젝트 로컬 `.claude/session`과 `.claude/session.log`(`<session-id> - <topic>`)에 세션을 기록한다. 로컬 실행 상태이므로 절대 커밋하지 않는다. |
| 366d5d35-7b46-4d5c-b5dd-3e53d48d7492 | instruction-operating-rules | `instructions/NNNN_name.md` 지시서의 생명주기. 템플릿, 상태, 완료 보관과 로그, UUID 레지스트리, 브랜치 연결, 범위 가드레일을 다룬다. |

위키 원문과 달라진 로컬 사본은 다음과 같습니다(자세한 내용은 각 `source.json`에 있음).
- **`git-worktree-guideline`:** 브랜치 이름, `{DATETIME}` 형식, `dev`/`fix` 규칙을 `git-branch-guideline` v1.1.0에 맞췄습니다. worktree 경로 매핑 `../.worktree/{BRANCH}`는 그대로입니다.
- **`instruction-operating-rules`:** UUID 레지스트리를 없앴으므로 UUID는 로컬에서 생성만 합니다. `git-branch-guideline`의 UUID를 바로잡고 브랜치 형식을 맞췄습니다. 세션 파일은 `.claude/session`, 명세는 `references/`, 스킬 경로는 `.skills/`를 가리킵니다.
