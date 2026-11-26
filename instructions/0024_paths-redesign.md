# 0024 — 표준 경로 재정의: 경로 자유 지정, `.artifacts/`, `.gitkeep`, 지시서 Git 규칙

> Entity UUID: 01a20b6e-8b1a-74c2-babb-d819de45ac39   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/paths-redesign/01a20b6e`
> 근거: 상위 지시서 `0022_flow-redesign.md` 2·3번, 이슈 #36, 2026-11-24 사용자 결정
> 선행: 0025 (Node 이식), 0023 (카탈로그 v2 로더의 `paths` 초깃값을 쓴다). Node로 구현한다

## 목표
경로 화면에서 지시서·임시 산출물·산출물·참조 문서 경로를 사용자가 자유롭게 정한다. TUI는 경로를 만들기만 한다.

## 사용자 결정 (2026-11-24)
1. 경로 이름 형식은 강제하지 않는다. 점 접두사 규칙(`OutputNotDotPrefixed`)을 없앤다.
2. 임시 산출물 키는 `artifacts`, 기본값 `.artifacts/`. 스키마 1 `paths.output`은 값은 그대로인 채 `paths.artifacts`로 읽는다.
3. TUI는 경로 생성 프로그램이다. 경로를 바꿔도 기존 디렉터리를 옮기거나 이름을 바꾸거나 지우지 않는다.
4. `.gitkeep`은 TUI가 새로 만든 Git 추적 디렉터리에만 만든다. 이미 있던 디렉터리는 무관하다. 기존 `.gitkeep`은 덮어쓰지 않고, 소유 파일로 기록하지 않는다.
5. 진행 중 지시서 제외: `/<instructions>/*.md`, `!/<instructions>/README.md`. 하위(`.completed/`)는 패턴이 닿지 않아 추적된다.
6. 디렉터리 숨김은 OS에 따른 선택 기능이다. Windows에서 새 임시 산출물 디렉터리 이름이 `.`으로 시작하면 시도하고, 실패는 정보로만 보고한다(적용을 막지 않음).
7. 선택 경로는 텍스트를 비우면 사용 안 함이다. `instructions`는 비울 수 없다.

## 범위
- 포함:
  - `harness.toml`·`harness.lock.json`·`state.json` 스키마 2(상수 하나). 버전 1·2를 읽고 2로 쓴다
  - `PathsConfig`: `instructions` 필수, `artifacts`·`docs`·`references`는 `Option`. 스키마 1 변환(`output` → `artifacts`, 없는 `references`·`instructions`는 기본값)
  - 경로 화면: 네 경로 편집, 빈 값은 사용 안 함, `project.json`의 `paths`를 초깃값으로
  - 사용 안 하는 경로는 만들지 않고 `.gitignore`·지침에 적지 않는다
  - `.gitignore` 관리 블록: 임시 산출물, `.harness/`, `agent.db*`, 지시서 `*.md`(README 제외)
  - `.gitkeep` 생성(새 추적 디렉터리: 지시서, `.completed/`, 산출물, 참조 문서)
  - 1단계 안전 검사 유지(루트 이탈, 심볼릭 링크, 중복·중첩, 종류 충돌)
  - 사용 문서 영/한 수정
- 제외: 디렉터리 이동·이름 변경·삭제, lock의 `docs` 필드와 동적 스킬 필드(해당 지시서에서 추가), 지침 블록 내용 재구성(지침 지시서)

## 테스트
- 임의 이름 경로 허용(점 없는 임시 산출물 포함), 이탈·중복·중첩은 거부
- 스키마 1 `harness.toml`(`output = ".output"`)을 읽으면 `artifacts = ".output"`, 적용 후 스키마 2로 쓰고 백업
- 사용 안 하는 경로는 디렉터리·`.gitignore` 줄이 없음
- `.gitkeep`: 새 디렉터리에만 생성, 기존 디렉터리·기존 `.gitkeep`은 그대로
- `git check-ignore`: `instructions/0001_x.md` 제외, `instructions/README.md`·`.completed/<uuid>.md`·`completed.log`·`.gitkeep` 추적
- 경로를 바꿔 다시 적용해도 기존 디렉터리가 남음
- Windows 숨김 실패가 적용을 막지 않음

## 완료 기준 (DoD)
- [ ] 테스트 포함 (build·lint·test 통과)
- [ ] 영/한 문서 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
현재 디렉터리가 루트이고, 기존 파일은 관리 블록 안에서만 바꾸며, 디렉터리를 옮기거나 지우지 않는다. 모든 쓰기가 성공한 뒤에만 성공을 기록한다.
