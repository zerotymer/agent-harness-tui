# 0013 — Windows 절대 경로 입력 수정

> Entity UUID: 01a1e6a0-7b4b-76d2-a4fd-85d67d7a494b   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `fix/windows-absolute-path/01a1e6a0`
> 근거: 0012 CI 실행 43831487125 (Windows 1개 실패), 사용자 결정 (2026-11-17): 별도 지시서로 수정

## 목표
Windows에서 프로젝트 안을 가리키는 절대 경로 입력이 `OutsideRoot`로 거부되는 제품 결함을 고친다.

## 원인
- `ProjectRoot::resolve`가 `\`를 `/`로 바꾼 뒤 경로를 해석해 `\\?\C:\` 형태가 깨진다.
- 정규화된 루트는 `\\?\C:\...`인데 사용자는 보통 `C:\...`로 입력하고, 대소문자도 다를 수 있다.

## 범위
- 포함:
  - Windows에서는 절대 경로 입력을 원래 구분자 그대로 해석
  - 입력 중 실제로 있는 부분을 정규화(canonicalize)한 뒤 루트와 비교 (`\\?\` 접두어, 대소문자, 심볼릭 링크 별칭 처리)
  - 테스트: Windows의 `C:\`·소문자·`/` 입력, Unix의 심볼릭 링크 별칭 입력
  - 0012 머지에서 빠진 Windows 전용 미사용 import 수정(`3d08cb7`) 반영
  - `docs/STAGE1_REPORT*.md` 플랫폼 표에 CI 결과 반영
- 제외: Windows 터미널 조작·한글 입력 실사용 검증 (별도 진행)

## 완료 기준 (DoD)
- [ ] 세 OS CI 통과
- [ ] 테스트 추가 (위 범위)
- [ ] 브랜치에서 작업하고 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다
