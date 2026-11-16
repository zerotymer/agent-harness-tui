# 0012 — 빌드 액션 (GitHub Actions CI·빌드 결과물)

> Entity UUID: 01a1e370-a94a-7cb5-b953-5a74223cbf50   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/build-action/01a1e370`
> 근거: 사용자 추가 지시 (2026-11-16). 1단계 명세 §8 플랫폼 검증 보조

## 목표
PR과 `main` push마다 Linux·Windows·macOS에서 fmt·clippy·test를 자동으로 돌리고, release 실행 파일을 워크플로 결과물로 올린다. 1단계에서 검증하지 않은 Windows·macOS에 대해 자동 테스트 수준의 결과를 얻는다.

## 규칙 구분
- 이 지시서는 이 저장소의 개발 인프라(CI)를 다룬다. TUI가 대상 프로젝트에 적용하는 제품 동작이 아니다.
- CI 통과는 Windows·macOS의 터미널 조작·한글 입력 같은 실사용 검증을 대신하지 않는다. 그런 검증은 계속 별도로 진행한다 (`.output/stage1-manual-test.html` Q 항목).

## 선행 조건
- 0001 ~ 0011 완료
- 사용자 결정 (2026-11-16): 범위는 "CI + 빌드 결과물", OS는 Linux·Windows·macOS

## 범위
- 포함:
  - `.github/workflows/ci.yml`: `pull_request`, `main` push, 수동 실행(`workflow_dispatch`)
  - OS 매트릭스: `ubuntu-latest`, `windows-latest`, `macos-latest` (한 OS가 실패해도 나머지는 계속)
  - 단계: `cargo fmt --check`(Linux), `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`
  - release 실행 파일을 OS별 결과물로 업로드
  - 툴체인 고정: `rust-toolchain.toml` (로컬과 같은 Rust 1.97.1, rustfmt·clippy)
  - 공급망 방어: 공식 `actions/*` 액션만 사용하고 커밋 SHA로 고정 (태그는 주석으로 표기), 최소 권한(`contents: read`), checkout 자격 증명 미보존
  - Cargo 캐시 (`actions/cache`)
  - Windows·macOS에서 드러나는 테스트 실패 처리: 테스트 환경 문제는 이 지시서에서 수정, 제품 결함이면 보고 후 별도 지시서로 분리
- 제외:
  - 태그 기반 GitHub Release 생성·배포
  - 서드파티 액션 (Rust 설정은 러너에 설치된 `rustup` 사용)
  - 코드 서명, 크로스 컴파일 타깃 추가

## 작업 항목
1. 사용할 공식 액션의 최신 릴리스와 커밋 SHA, 입력값 확인
2. `rust-toolchain.toml`, `.github/workflows/ci.yml` 작성
3. PR에서 세 OS 실행 결과 확인, 실패 원인 분류·수정
4. 문서 반영: `CLAUDE.md`(영/한) 명령 안내, `docs/STAGE1_REPORT*.md` 플랫폼 표에 CI 결과

## 산출물
- `.github/workflows/ci.yml`, `rust-toolchain.toml`
- 세 OS의 CI 실행 결과와 실행 파일 결과물
- 갱신된 문서

## 완료 기준 (DoD)
- [ ] 세 OS에서 CI 통과 (또는 실패 원인과 후속 지시서 명시)
- [ ] 실행 파일 결과물이 OS별로 올라감
- [ ] 액션이 커밋 SHA로 고정되고 권한이 최소화됨
- [ ] 테스트 제외 사유: CI 설정 자체는 실행 결과로 검증
- [ ] 가드레일 확인 (아래)
- [ ] 브랜치에서 작업하고 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
- 비밀 값을 워크플로에 넣지 않는다. 워크플로는 저장소 읽기 권한만 쓴다.
- 서드파티 액션·스크립트를 원격에서 받아 실행하지 않는다 (`curl | sh` 금지).
- 1단계 금지 범위(worktree·서비스 관리, 실행형 훅 등)를 건드리지 않는다.
