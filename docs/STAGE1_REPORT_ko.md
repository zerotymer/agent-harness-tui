# 1단계 검증 보고

> 2026-11-26: 구현을 Node.js(TypeScript + Ink, 지시서 0025)로 옮기고 Rust 크레이트를 지웠다. 이 보고는 인수된 Rust 빌드의 기록으로 남긴다. 같은 인수 테스트 11개는 이제 `src/stage1.test.ts`에서 돈다.

일자: 2026-11-16. 범위: 지시서 0001–0011. `main`의 `bdafb75`에 0011 브랜치를 더한 상태로 빌드했다. 명세: [`references/STEP_1_INITIAL_CREATION_ko.md`](../references/STEP_1_INITIAL_CREATION_ko.md) §8.

## 환경

| 항목 | 값 |
| --- | --- |
| OS | Rocky Linux 9.8 (커널 5.14), x86_64 |
| 툴체인 | Rust 1.97.1, edition 2024 |
| 주요 의존성 | ratatui 0.30.2 (crossterm 0.29.0), serde, toml 1.2, serde_json, sha2 0.11, ureq 3 |
| 사용자 | 일반 사용자 (관리자 권한 없음) |
| Git | 2.52.0 |

## 테스트 결과

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`가 모두 통과한다. 테스트는 101개다. 단위 테스트 87개, 실행 파일 테스트 3개, 1단계 인수 테스트 11개(`tests/stage1.rs`)로 이루어진다. 모든 테스트는 임시 디렉터리에 대상 프로젝트를 만들어 검증한다. 자동 테스트는 네트워크를 쓰지 않으며, 게이트웨이 클라이언트는 로컬 목 HTTP 서버로 테스트한다.

## 완료 기준 (명세 §8)

| 기준 | 결과 | 근거 |
| --- | --- | --- |
| 로컬 소스로 관리자 권한·네트워크 없이 임시 프로젝트 초기화 | 통과 | `stage1::initializes_with_local_source_and_no_network` (일반 사용자, 로컬 소스만) |
| 재실행 후 설정·경로 해석 유지 | 통과 | `stage1::rerun_keeps_config_and_path_resolution`: 공백이 든 한글 경로, `harness.toml`에 절대 경로 없음, 재실행 시 쓰기 계획 없음 |
| 두 언어 지침의 진행 중·완료 지시서 정책 일치 | 통과 | `stage1::english_and_korean_guidance_state_the_same_policy`, `guidance::tests::english_and_korean_share_every_path_and_value` |
| 기존 지침·로그 보존, 반복 초기화 시 중복 없음 | 통과 | `stage1::existing_guidance_and_log_survive_and_repeats_add_nothing`: 사용자 내용·CRLF·기존 로그·기존 `.gitignore` 규칙 보존, 파일마다 블록 하나 |
| 경로 이탈·충돌 거부 | 통과 | `stage1::rejects_path_escapes_and_collisions`, `paths::tests::*` (`..`, 심볼릭 링크, Windows 드라이브 경로, 중복, 중첩, 파일 충돌) |
| 생성 지침 경계 | 통과 | `stage1::guidance_edits_inside_the_block_are_conflicts`, `blocks::tests::*` (손상·중복·중첩 마커는 충돌, 파일 전체를 다시 쓰지 않음) |
| 패키지 경로 이탈 거부 | 통과 | `stage1::rejects_package_paths_that_escape`, `sources::tests::local_rejects_escaping_symlinks` |
| Git ignore 동작 | 통과 | `stage1::git_tracks_shared_files_and_ignores_local_state`와 `gitignore::tests::*`를 실제 Git으로 확인: 공유 파일은 보이고 로컬 상태는 제외됨, 스테이징·커밋 없음, 상위 규칙·추적 중 경로 경고 |
| 적용 중단 후 복구 | 통과 | `stage1::interrupted_apply_restores_files_and_records_no_success` (적용 도중 실제 권한 오류), `apply::tests::injected_failure_restores_files_and_records_nothing` (모든 쓰기 지점에서 실패 주입) |
| 에이전트 연결부를 최신 공식 규약과 대조, 미지원 방식 기록 | 통과 | "에이전트 연결부" 참고 |
| Linux·Windows·macOS에서 루트·경로·터미널 동작 검증 | Linux만 | "플랫폼" 참고 |
| worktree·서비스 관리, 실행형 훅, 임의 SQLite 스키마 미포함 | 통과 | 해당 코드가 없다. `agent.db`는 옵션과 ignore 규칙뿐이며, 인수 테스트에서 DB 파일이 생기지 않음을 확인한다 |

추가로 확인한 것:
- 로컬 상태 없이 clone된 프로젝트에서 수정된 파일을 덮어쓰지 않는다 (`stage1::clone_without_local_state_does_not_overwrite`).
- 2단계에 필요한 상태를 기록한다 (`stage1::state_keeps_what_stage2_needs`).

## 플랫폼

| 플랫폼 | 결과 | 확인한 내용 |
| --- | --- | --- |
| Linux (Rocky 9.8) | 검증함 | Unix 전용 심볼릭 링크 테스트를 포함한 전체 테스트. 실제 실행 파일을 가상 터미널(`script`, 120×40)에서 실행: Enter만으로 초기화, 파일 생성, 종료 코드 0, 대체 화면 해제(`ESC[?1049l`)와 커서 복원(`ESC[?25h`). TTY 없이 실행하면 종료 코드 2로 끝나고 아무것도 쓰지 않음 |
| Windows | CI 자동 테스트 통과 (실행 43979645645) | `windows-latest`에서 clippy·테스트·release 빌드. `\\?\C:\` 형태의 정규 루트와 비교되는 절대 경로 입력·대소문자 처리는 0013에서 수정함. 직접 확인할 것: 산출물 폴더의 `attrib +h`, 콘솔 렌더링과 한글 입력. 심볼릭 링크 테스트는 Windows에서 건너뜀 |
| macOS | CI 자동 테스트 통과 (실행 43979645645) | `macos-latest`에서 clippy·테스트·release 빌드. 직접 확인할 것: 터미널 동작, 대소문자를 구분하지 않는 파일 시스템에서의 중복 판정, `/private/var` 임시 경로에서의 심볼릭 링크 처리 |

## 에이전트 연결부 (2026-11-16 확인, 어댑터 버전 1)

| 에이전트 | 지침 파일 | 스킬 폴더 | 복사 | 참조 | 심볼릭 링크 |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `CLAUDE.md` (+ `CLAUDE_ko.md`) | `.claude/skills/<name>/SKILL.md` | 지원 | 지침 안내만 | 선택 불가 |
| Codex | `AGENTS.md` (+ `AGENTS_ko.md`) | `.agents/skills/<name>/SKILL.md` | 지원 | 지침 안내만 | 선택 불가 |

- **Claude Code** (Claude Code CHANGELOG·스킬 문서): `SKILL.md`에 `name`·`description` 프런트매터가 필요하다.
- **Codex** (learn.chatgpt.com "Build skills", "Customization overview"): 현재 폴더부터 저장소 루트까지 `.agents/skills`를 탐색하며, `name`·`description`이 필요하다. Codex 소스에는 사용자 문서에 없는 `<repo>/.codex/skills` 루트도 있어서 사용하지 않는다.
- **참조 배치:** 두 에이전트 모두 지침에 `.skills/`를 적는 것만으로는 스킬을 찾지 않는다. 그래서 참조 배치는 지침에 정책을 적는 것으로 끝난다.
- **심볼릭 링크 배치:** Windows에서 추가 권한이 필요하고 복사로 모든 환경에서 동작하므로 제공하지 않는다.
- **프런트매터:** 해석하지 않는다 (사용자가 추후 결정). 복사 배치에서 `SKILL.md`가 `---`로 시작하지 않으면 경고만 한다.

## 위키 게이트웨이 (2026-11-16 실제 확인)

`wiki-skill-import`(`14cea96f-f1e2-461e-b9a7-96b5a6f63b67`, `type = "skill"`)를 사용자 환경변수의 토큰으로 가져왔다. 환경변수는 이름으로만 지정했고, 값은 출력하지 않았다.
- 두 언어를 모두 받았고, 빠진 언어나 경고는 없었다.
- 언어별 `contentHash`가 이 저장소 `.skills/wiki-skill-import/source.json`의 값과 같다.
- 받은 바이트의 SHA-256이 이 저장소의 `SKILL.md`, `SKILL_ko.md`와 같아서, 내용을 가공 없이 저장함을 확인했다.

## 미확정 전제조건

| 항목 | 현재 동작 | 필요한 것 |
| --- | --- | --- |
| 게이트웨이 목록·검색 | "더미"로 표시한 고정 항목, 네트워크 미사용 | 목록·검색 REST 규약 (사용자 제공 예정) |
| 프런트매터 활용 | 내용은 그대로 저장, 스킬 이름은 사용자가 입력 | 활용 방식 결정 (사용자 결정 예정) |
| Windows·macOS 검증 | CI 자동 테스트 통과, 터미널 직접 확인은 남음 | 별도 실행 (사용자 결정) |
| Codex `.codex/skills` | 사용 안 함 | 공식 문서에 등장할 때 |

## 알려진 제한

- 스킬 조회는 동기 방식이라, 응답이 오거나 15초 시간 초과가 날 때까지 화면이 갱신되지 않는다.
- 로컬 폴더와 토큰 환경변수 이름은 한 세션에 하나씩이다. 해당 소스에서 첫 스킬을 추가하면 고정된다.
- 로컬 스킬 폴더는 프로젝트 안만 받는다.
- 스킬을 가져올 때 실행 권한 같은 파일 모드는 옮기지 않는다.
- 관리 대상 파일에서 정확한 마커가 아닌 `agent-harness:` 문자열은 충돌로 처리한다. 의도적으로 보수적으로 판정한다.
- 적용 ID와 시각은 UTC로 기록한다.
- 이미 초기화된 프로젝트는 검증만 제공한다. 스킬 추가·갱신은 2단계다.

## 2단계를 위한 상태 규약

[`references/HARNESS_STATE_CONTRACT_ko.md`](../references/HARNESS_STATE_CONTRACT_ko.md)가 스키마 버전 1을 정의한다. 2단계 명세 §8이 요구하는 상태 항목을 모두 1단계 필드에 대응시켜 두었다: 안정 ID, 소스 리비전, 관리 파일·블록, 기준 해시, 복사·참조 매핑, 어댑터 버전, 마지막 성공 적용. 1단계는 프로젝트 로컬 상태만 쓰므로 저장소·worktree 식별자는 의도적으로 기록하지 않는다.
