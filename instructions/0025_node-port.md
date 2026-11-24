# 0025 — 구현 플랫폼 전환: Rust → Node.js(TypeScript + Ink)

> Entity UUID: 01a20b75-cbb5-73b9-9f00-8377607348bd   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/node-port/01a20b75`
> 근거: 2026-11-24 사용자 결정, 상위 지시서 `0022_flow-redesign.md`
> 순서: 0023·0024보다 먼저 한다. 0023·0024는 이 지시서가 끝난 뒤 Node로 구현한다.

## 목표
에이전트와 스킬을 npm으로 설치하므로 환경 전체를 Node.js로 통일한다. 인수된 1단계 기능을 같은 동작으로 Node에 옮기고, 같은 PR에서 Rust 크레이트를 지운다.

## 사용자 결정 (2026-11-24)
1. 언어·화면: TypeScript + Ink 8 (Node.js `>=22`, React `>=19.3.0`, 2026-11-24 npm 레지스트리 확인)
2. 전환: 이식 먼저, 1단계와 동등해지면 같은 PR에서 Rust 삭제
3. 도구: pnpm, 테스트는 내장 `node:test`(화면은 `ink-testing-library`)
4. 배포: GitHub Release에 `npm pack` 타르볼과 `SHA256SUMS`. 설치는 `npm install -g <URL>`. npm 레지스트리 공개는 나중에 정한다
5. Node 최소 버전 22 (`engines`)

## 범위
- 포함:
  - `package.json`(`engines.node >=22`, `bin: agent-harness-tui`), `tsconfig.json`, `pnpm-lock.yaml`. 스크립트 `build`·`test`·`lint`
  - 1단계 기능 이식: 경로 검사(`RelPath`, 이탈·심볼릭 링크·중복·중첩), 관리 블록, `.gitignore` 블록, 지침 생성, 계약 파일(`harness.toml`·`harness.lock.json`·`state.json`, 모르는 키 거부), 적용 엔진(백업, `apply.lock`, 모든 쓰기 성공 후 기록), 백로그 스크립트, 위키 게이트웨이 가져오기, 스킬 배치(copy/reference), 0021 skills 설치
  - 화면: Ink. 터미널 넘기기는 `useApp().suspendTerminal()`
  - 인수 테스트 이식: `tests/stage1.rs`의 11개를 `node:test`로. 단위 테스트는 동작 단위로 옮긴다(개수 맞추기는 목표가 아님)
  - CI(`ci.yml`)와 릴리스(`release.yml`)를 Node로: 세 OS, SHA 고정 공식 `actions/*`(`actions/setup-node` 포함), `v*` 태그는 `package.json` 버전과 같아야 함
  - 의존성 선택 시 공식 문서 확인: TOML 파서, HTTP는 내장 `fetch`, 해시는 `node:crypto`
  - 같은 PR에서 삭제: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/*.rs`, `tests/*.rs`. `.gitignore`의 `target/`을 `node_modules/`, `dist/`로 바꾼다
  - 문서 영/한: `CLAUDE*.md` 명령 절(“현재/목표” 구분 제거), `docs/USAGE*.md`. `docs/STAGE1_REPORT*.md`는 이력으로 두고 맨 위에 전환 안내 한 줄
- 제외: 0023·0024의 새 기능(에이전트 화면, 카탈로그 v2, 경로 재정의). 동작은 1단계 그대로 옮긴다

## 추가 결정 (2026-11-24)
- 단위: U1 뼈대 → U2 경로·계약 → U3 적용 엔진(`apply`·`blocks`·`backlog`) → U4 생성물(`placement`·`guidance`·`gitignore`·`local`) → U5 가져오기·실행(`sources`·`catalog`·`install`) → U6 화면·통합(`init`·`tui`·`main`, 인수 테스트) → U7 Rust 삭제·CI·릴리스·문서. PR 하나에 단위별 커밋
- SQLite: 내장 `node:sqlite`(U3에서 공식 문서로 확인)
- 0020 카탈로그 v1은 항목 검증만 옮긴다. 환경변수·실행 파일 옆 위치와 위키 카탈로그 가져오기는 옮기지 않는다(0023에서 삭제 예정)
- 린트·포맷: Biome + `tsc --noEmit`
- pnpm은 `packageManager` 필드와 Corepack으로 고정한다(CI는 공식 `actions/*`만 쓰므로)
- 미비한 부분은 추후 작업으로 남긴다

## 확인 필요 (구현 전)
- ~~Windows의 `.cmd` 실행~~ → 결정(2026-11-24): Node는 `.cmd`·`.bat`을 `cmd.exe` 없이 실행하지 못하고 `shell: true`는 폐기 예정(DEP0190)이다. 찾은 프로그램이 `.cmd`·`.bat`일 때만 `cmd.exe /d /s /c <파일> <인자>`로 실행하고, 모든 인자에 공백과 `& | < > ^ % ! " ( )`가 없는지 먼저 검사해 걸리면 거부한다. `shell` 옵션은 쓰지 않는다. 이 검사의 단위 테스트(모든 OS에서 실행)를 둔다

## 테스트
- 1단계 인수 테스트 11개가 Node에서 통과
- 세 OS CI 통과(Linux·Windows·macOS)
- 타르볼을 `npm install -g`로 설치한 뒤 `agent-harness-tui`가 실행됨(CI 스모크)

## 완료 기준 (DoD)
- [ ] 테스트 포함 (build·lint·test 통과)
- [ ] 영/한 문서 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
1단계 규칙(현재 디렉터리가 루트, 관리 블록 밖 보존, 백업, 모든 쓰기 성공 후 기록, 셸 문자열 실행 금지)은 플랫폼과 무관하게 그대로다. 동작 변경 없이 옮긴다.
