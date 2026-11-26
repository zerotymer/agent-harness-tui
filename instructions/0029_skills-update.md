# 0029 — 스킬 갱신: skills CLI로 설치한 프로젝트 스킬

> Entity UUID: 01a21a29-409f-7d10-9846-497f4c34ddf9   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/skills-update/01a21a29`
> 근거: 상위 지시서 `0022_flow-redesign.md` 5번(skills 갱신), `STEP_2` §6.1
> 선행: 0028

## 목표
skills CLI로 설치한 프로젝트 스킬을 사용자 확인 후 갱신한다.

## 공식 문서 확인 (2026-11-27, skills CLI 1.9.0, vercel-labs/skills `docs/commands.md`)
- `npx skills update [skills...]`, `-g`(전역만), `-p`(프로젝트만), `-y`(범위 질문 생략)
- `npx skills check`: 갱신·로컬 변경 확인만, 있으면 종료 코드 1

## 범위
- 포함:
  - 초기화된 프로젝트의 검증 화면에 [스킬 갱신]. 프로젝트 루트에 `skills-lock.json`이 있을 때만 보인다. 검증 충돌과 무관하다
  - 확인 화면에 `npx skills update -p` 전체 표시. `-y` 없음(CLI 질문은 사용자에게)
  - Enter → 터미널 넘기기 실행 → Enter로 복귀 → 종료 코드 표시. `npx`가 없으면 실행하지 않음
  - 하네스는 `skills-lock.json`을 읽거나 쓰지 않는다. 읽기 전용 미리보기로 `npx skills check`를 안내
- 제외: 자동·예약 갱신, 스킬 삭제, 전역(`-g`) 갱신

## 테스트
- `skills-lock.json` 없으면 버튼 없음, 있으면 표시. `npx` 없으면 실행 안 함. 명령 인자 `skills update -p`. 결과 표시 후 Esc로 검증 화면 복귀. 잠금 파일 무변경

## 완료 기준 (DoD)
- [ ] 테스트 포함 (build·lint·test 통과)
- [ ] 영/한 문서 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
셸 없이 인자 배열만, 사용자 확인 후에만 실행한다. 하네스 소유 파일은 바꾸지 않는다.
