# 0021 — 공개 스킬 설치 (`npx skills add`)

> Entity UUID: 01a1f814-b504-7198-9613-f289ac469b4d   (로컬 생성, UUIDv7)
> Status: TODO
> Branch: (작업 시작 시 기입, 예: `feature/skills-install/01a1f814`)
> 근거: `references/STEP_2_CATALOG_INTEGRATION*.md` §3, `references/HARNESS_CATALOG_CONTRACT*.md` `skills` 항목

## 목표
skills 카탈로그 항목을 사용자가 확인하면 공식 CLI `npx skills add`로 설치한다.

## 선행 조건
- 0020 (카탈로그 읽기와 카탈로그 화면)

## 범위
- 포함:
  - 인자 배열 생성: `npx skills add <source>[#<ref>] -s <skill> -a <agent>... -y [-g]`. 에이전트 목록은 `harness.toml`의 에이전트를 쓴다
  - 범위 선택: 기본 project, user를 고르면 `-g`
  - 확인 화면: 명령 전체를 보여 주고, 사용자가 확인한 경우에만 실행
  - 셸 없이 실행. 작업 디렉터리는 프로젝트 루트이고, Windows에서는 `npx.cmd`
  - 출력과 종료 코드 표시. 0이 아닌 종료 코드는 실패
  - `npx`가 없으면 사용 불가로 표시
  - 하네스가 이미 배포한 같은 이름의 스킬이 있으면 충돌로 표시하고 실행하지 않음
  - `skills-cli`가 아닌 설치 도구 항목은 `install` 문구만 표시
  - `#<ref>` 버전 고정이 실제 CLI에서 동작하는지 수동으로 확인하고 결과를 문서에 기록
  - 영/한 사용 문서
- 제외: CLI가 설치한 파일의 소유권 관리, 제거·갱신(3단계), 다른 패키지 실행 도구

## 테스트
- project/user 범위별 인자 배열
- 확인 없이는 실행하지 않음
- 대체 실행 파일로 성공·실패 종료 코드 보고
- `npx`가 없을 때 사용 불가로 표시
- 이름 충돌 시 실행 차단
- 옵션처럼 보이는 값(`-`로 시작)이 카탈로그 검증 단계에서 거부되는지

## 완료 기준 (DoD)
- [ ] 테스트 포함 (fmt·clippy·test 통과)
- [ ] 영/한 문서 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
허용된 실행 파일(`npx`)만, 사용자 확인 후, 셸 없이 구조화된 인자로 실행한다. 카탈로그의 명령 문자열은 실행하지 않는다.
