# 0015 — TUI 종료 기능 보강

> Entity UUID: 01a1e6cf-6ba6-79dc-a8b9-f78e6f74cdf9   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `fix/quit-key/01a1e6cf`
> 근거: GitHub 이슈 #20 "종료기능부재", 사용자 요청 (2026-11-17)

## 목표
사용자가 어느 단계에서든 쉽게 종료할 수 있게 한다. 지금은 Ctrl+C로만 종료되고, 완료 화면에서는 Enter로도 종료된다. 첫 화면(`Step::Root`)에서는 Esc를 눌러도 반응이 없다(`src/tui.rs` `back()`).

## 선행 조건
- 없음

## 범위
- 포함:
  - 첫 화면(`Step::Root`)에서 Esc를 누르면 종료한다.
  - `q` 키로 종료한다. 단, 텍스트 입력 중(경로 입력 등)에는 `q`를 글자로 처리한다.
  - 적용 전 종료는 지금의 Ctrl+C와 같다. 아무것도 쓰지 않는 취소로 처리한다.
  - 하단 키 안내 문구에 종료 키를 표시한다.
  - 사용 문서 `docs/USAGE.md`와 `docs/USAGE_ko.md`의 키 안내를 함께 갱신한다.
- 제외: 종료 확인 대화상자, 적용(apply) 도중 중단

## 작업 항목
1. `handle_key`: Root 단계의 Esc와 비입력 상태의 `q`를 `quit = true`로 처리
2. 하단 안내 문구 갱신
3. 단위 테스트: Root+Esc 종료, 입력 중 `q`는 글자 입력, 종료 시 파일 미생성

## 완료 기준 (DoD)
- [ ] 테스트 포함 (`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` 통과)
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지), PR에 `Closes #20`
- [ ] 완료는 사용자가 명시적으로 선언한다
