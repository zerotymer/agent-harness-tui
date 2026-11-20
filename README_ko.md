# Agent Harness TUI

## 목적

현재 프로젝트 루트에서 에이전트 하네스를 구성하는 Rust 독립 실행형 TUI를 만든다. 지시서·참조 문서·산출물·스킬 경로를 관리하고 프로젝트 전체에 적용할 에이전트 지침을 생성한다.

이 문서들은 이 저장소의 구현 지시서다. 애플리케이션은 이 저장소에서 단계 순서대로 구현한다. Git 초기화와 서비스 등록은 수행하지 않는다.

## 문서 구성

| 단계 | 영문 지시서 | 한국어 번역본 | 범위 |
| --- | --- | --- | --- |
| 1 | [STEP_1_INITIAL_CREATION.md](references/STEP_1_INITIAL_CREATION.md) | [STEP_1_INITIAL_CREATION_ko.md](references/STEP_1_INITIAL_CREATION_ko.md) | 독립 실행형 TUI, 최초 생성, 외부 스킬, 설정 및 기준 상태 저장 |
| 1 | [docs/USAGE.md](docs/USAGE.md), [docs/STAGE1_REPORT.md](docs/STAGE1_REPORT.md) | [docs/USAGE_ko.md](docs/USAGE_ko.md), [docs/STAGE1_REPORT_ko.md](docs/STAGE1_REPORT_ko.md) | 설치·사용, 검증 보고 |
| 2 | [STEP_2_CATALOG_INTEGRATION.md](references/STEP_2_CATALOG_INTEGRATION.md), [HARNESS_CATALOG_CONTRACT.md](references/HARNESS_CATALOG_CONTRACT.md) | [STEP_2_CATALOG_INTEGRATION_ko.md](references/STEP_2_CATALOG_INTEGRATION_ko.md), [HARNESS_CATALOG_CONTRACT_ko.md](references/HARNESS_CATALOG_CONTRACT_ko.md) | Wiki.js 페이지·공개 스킬·문서의 로컬 JSON 카탈로그 |
| 3 | [STEP_3_ADDITIONAL_FEATURES.md](references/STEP_3_ADDITIONAL_FEATURES.md) | [STEP_3_ADDITIONAL_FEATURES_ko.md](references/STEP_3_ADDITIONAL_FEATURES_ko.md) | 제한된 업데이트, override, 모드, 훅, Git worktree, Worktrunk·Docktree |

각 단계의 구현·검증을 완료한 뒤 다음 단계를 시작한다. Worktrunk는 Git worktree 관리 도구이므로 3단계에 포함하며, 최초 하네스 생성의 필수 조건으로 삼지 않는다.

## 결정 사항

- 구현 기술은 Rust, Ratatui, Crossterm이다. Linux 서버를 우선 대상으로 하고 Windows·macOS 네이티브 실행을 지원한다.
- 현재 실행 디렉터리를 프로젝트 루트로 사용한다. 1단계에서 상위 저장소나 주 worktree로 실행 기준을 자동 변경하지 않는다.
- 대상 프로젝트의 Git 저장소에서 진행 중·완료 지시서, 지시서 로그, 참조 문서, 스킬, 공유 하네스 설정을 관리한다.
- 산출물, 선택적 SQLite 백로그 데이터, 인증 정보, 환경별 로컬 상태는 Git에서 제외한다.
- 작업 지시서는 한국어로만 작성한다(`instructions/NNNN_name.md`). 에이전트가 참고하는 참조 문서·스킬·지침 문서는 영문과 동등한 `_ko.md` 번역본을 함께 작성한다. 외부에서 가져온 원문을 보존하며 번역을 임의로 만들어내지 않는다.
- 유지보수되는 안정 버전 의존성을 사용한다. deprecated API나 지원 종료 OS를 위한 호환 코드를 새로 도입하지 않는다.
- 외부 시스템은 선택적 연계 후보이며, 이 지시서에 따라 새로 구축할 서비스가 아니다.

## 출처 및 호환성 확인

다음 공식 자료를 2026-11-15 기획에 참고했다. 구현 시 지원 버전과 연동 규약을 다시 확인하며, 과거 이벤트명이나 설정 예제를 검증 없이 복사하지 않는다.

- [Ratatui 백엔드](https://ratatui.rs/concepts/backends/comparison/)
- [Crossterm](https://github.com/crossterm-rs/crossterm)
  - 2026-11-16 확인(Context7, docs.rs): Ratatui 0.30.x는 기본으로 `crossterm` 백엔드(crossterm 0.29.x, 기능 `crossterm_0_29`)를 쓰고 `ratatui::crossterm`으로 다시 내보낸다. 따라서 크레이트는 `ratatui`에만 의존한다. 툴체인: Rust 1.97.1, edition 2024.
- [프로젝트 AGENTS.md 지침](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
- [Claude Code 프로젝트 지침](https://code.claude.com/docs/en/memory)
- [Claude Code 훅](https://code.claude.com/docs/en/hooks)
- [Git ignore 규칙](https://git-scm.com/docs/gitignore)
- [Worktrunk 설정](https://worktrunk.dev/config/)
- [Docktree](https://docktree.dev/)
