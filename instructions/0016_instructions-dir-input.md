# 0016 — 지시서 경로 사용자 입력 (기본 `instructions`)

> Entity UUID: 01a1e6cf-6ba8-7c9e-ab81-70e95764ab08   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/instructions-dir/01a1e6cf`
> 근거: 사용자 요청 (2026-11-17) "instructions 경로는 고정값이 아니야. 사용자 입력을 받아서 적용 (기본: instructions)"

## 목표
대상 프로젝트의 지시서 루트를 사용자가 TUI에서 입력해 정할 수 있게 한다. 기본값은 `instructions`다. `.completed/`와 `completed.log`는 입력한 지시서 루트 아래에 둔다.

## 선행 조건
- 없음. 0017(agent.db)이 이 지시서에 의존한다.

## 명세 변경 (먼저 수행, 영/한 동시)
- `references/STEP_1_INITIAL_CREATION.md`·`_ko.md` §1 표: 진행 중 지시서 항목의 "1단계에서 고정"을 "사용자 입력, 기본 `instructions`"로 바꾼다.
- 같은 문서의 본문 예시 경로(`instructions/…`)를 "지시서 루트(기본 `instructions`)" 표현으로 정리한다.

## 범위
- 포함:
  - `paths.rs`: `INSTRUCTIONS_DIR` 상수는 기본값으로만 쓴다. `Layout`은 설정값에서 경로를 얻는다(`completed_log()` 등).
  - `contract.rs`: 설정 파일에 지시서 경로 필드를 추가하고 기본값을 `instructions`로 둔다. 기존 경로 검증을 그대로 적용한다(루트 밖·중첩·중복·충돌 금지, 대소문자 무시 비교).
  - TUI 경로 입력 단계(`Step::Paths`)에 지시서 경로 입력란을 추가한다.
  - 경로를 따라가야 하는 곳: `init.rs`, `guidance.rs`(지침 문서 내 경로 표기), `gitignore.rs`(완료 로그·`.completed/`를 추적 대상으로 검사), 미리보기 화면
  - 이미 초기화된 프로젝트는 기록된 경로를 쓴다. 기록이 없으면 기본값을 쓴다.
- 제외: 초기화 이후 경로 변경·이동(2단계 updater 범위)

## 작업 항목
1. 명세 수정(영/한)
2. 설정·Layout 변경 및 하드코딩된 `"instructions/…"` 제거 (`grep -rn '"instructions' src`)
3. TUI 입력란·검증 메시지
4. 테스트: 기본값 동작, 사용자 지정 경로(예: `docs/tasks`)로 init 시 파일 위치, 잘못된 경로 거부, 기존 설정 파일(필드 없음) 호환

## 완료 기준 (DoD)
- [ ] 테스트 포함 (fmt·clippy·test 통과)
- [ ] 명세 영/한 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다
