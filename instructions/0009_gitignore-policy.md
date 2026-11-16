# 0009 — 1단계: Git ignore 처리

> Entity UUID: 01a1e24a-c393-79e1-a94c-d6fd10582729   (로컬 생성, UUIDv7)
> Status: TODO
> Branch: (작업 시작 시 기입 — `feature/gitignore-policy/01a1e24a`)
> 명세: `references/STEP_1_INITIAL_CREATION.md` / `_ko.md` — §6

## 목표
대상 프로젝트 `.gitignore`에 제한된 관리 블록을 생성하고, Git이 있으면 ignore 영향과 추적 상태를 보고한다.

## 규칙 구분
- 아래 요구사항은 TUI가 **대상 프로젝트에 적용하는 제품 동작**이다. 이 저장소의 작업 규칙(`CLAUDE.md`, `.skills/`)과 혼동하지 않는다.
- 테스트는 임시 디렉터리에 대상 프로젝트를 만들어 검증한다. 이 저장소의 `instructions/`, `.gitignore`, 지침 파일을 픽스처로 쓰지 않는다.

## 선행 조건
- 0002, 0005

## 범위
- 포함:
  - 관리 블록 내용: 설정된 산출물 경로, `.harness/`, `agent.db`, `agent.db-wal`, `agent.db-shm`, `agent.db-journal`, 재생성 가능 배포 복사본(0008)
  - DB 미사용이어도 백로그 경로 제외
  - 무관한 기존 ignore 규칙 보존
  - Git 사용 가능 시 (`git` CLI): 상위·하위·전역 ignore 때문에 공유 경로가 제외되면 보고
  - ignore 목적지가 이미 추적 중이면 `.gitignore`가 추적을 해제하지 않음을 안내, 인덱스 자동 변경 금지
  - 빈 디렉터리가 공유된다고 안내하지 않음
  - Git 없음: 검사를 건너뛰고 그 사실을 표시
  - 커밋·푸시 없음
- 제외:
  - Git 인덱스·브랜치 변경

## 작업 항목
1. ignore 블록 생성
2. Git 검사 (`check-ignore`, `ls-files`)
3. 테스트: 블록 생성·멱등, 기존 규칙 보존, 상위 규칙에 의한 제외 보고, 추적 중 경로 안내 (Git 없는 환경은 건너뜀 사유 명시)

## 산출물
- ignore 모듈 + 테스트

## 완료 기준 (DoD)
- [ ] Git ignore 동작 테스트 포함 (명세 §8)
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` 통과
- [ ] 의미 있는 테스트 포함 (테스트 제외 시 사유 명시)
- [ ] 가드레일 확인 (아래)
- [ ] 브랜치에서 작업하고 PR 준비 (main 직접 push 금지, 커밋·푸시는 사용자)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
- 1단계 금지 범위(worktree·서비스 관리, 실행형 훅, 범용 업데이트, SQLite 스키마)를 건드리지 않는다.
- 외부 규약은 확인된 자료(게이트웨이: `wiki-skill-import` 스킬, 에이전트 규약: 공식 문서)에 없는 내용을 추측 구현하지 않는다.
- 인증 정보·토큰·환경별 절대 경로를 공유 파일·로그·미리보기에 기록하지 않는다.
