# 0019 — 2단계 명세 재작성: 카탈로그 연동 (나머지는 3단계로 이관)

> Entity UUID: 01a1f80c-e91d-7346-8e86-a6cc73d252b3   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `feature/stage2-catalog-spec/01a1f80c`
> 근거: 사용자와 2단계 범위 논의 및 결정 (2026-11-17 ~ 2026-11-20)

## 목표
2단계 범위를 **외부 카탈로그 연동**(위키, 공개 스킬, 문서 목록)으로 다시 정한다. 현재 `STEP_2_ADDITIONAL_FEATURES`에 있는 나머지 기능은 3단계 명세로 옮긴다. 이 지시서는 명세와 규격 문서만 다룬다. 구현은 이후 지시서에서 한다.

## 결정 사항 (사용자 확인)
### 공통
1. 카탈로그는 세 파일로 나눈다: `wiki.json`, `skills.json`, `docs.json`. 세 파일은 같은 공통 틀을 쓴다.
   - 파일 수준: `schema_version`, `kind`(`wiki`|`skills`|`docs`), `catalog_version`, `entries`
   - 항목 공통: `id`(파일 안에서 중복 금지), `title{en,ko}`, `summary{en,ko}`, `version`, `updated`. 없는 언어는 키를 생략한다.
2. **TUI는 카탈로그를 내려받지 않는다.** 카탈로그 JSON은 배포 책임자가 관리하고 배포한다. 받는 방법(스크립트, 수동 복사 등)은 TUI 밖의 일이다.
3. TUI가 카탈로그를 찾는 순서: 환경변수에 지정한 로컬 경로 → 실행 파일 옆 `catalog/` → 실행 파일에 내장된 샘플(`include_str!`). 배포물은 실행 파일 하나이고, JSON은 선택 리소스다.
4. 모르는 키, 맞지 않는 `kind`, 지원하지 않는 `schema_version`은 거부한다. 거부할 때 다른 카탈로그로 몰래 대체하지 않는다. 화면에는 지금 쓰는 카탈로그의 출처와 `catalog_version`을 표시한다.
5. 저장소의 `catalog/*.json`은 내장 샘플이다. CI에서 TUI와 같은 Rust 타입으로 파싱해 규격을 검사한다.

### skills (공개 스킬)
6. 설치는 공식 방법인 `npx skills add`(vercel-labs/skills)로 한다. `npx`만 사용한다.
7. **사용자가 확인하면 TUI가 실행한다.** 기본 설치 범위는 project이고, 사용자가 user를 고르면 `-g`를 붙인다.
   - 카탈로그에는 셸 명령 문자열 대신 구조화된 필드를 둔다: `source`, `skill`, `ref`
   - TUI는 `npx skills add <source>[#ref] -s <skill> -a <agents> -y [-g]`를 인자 배열로 만들어 셸 없이 실행한다. Windows에서는 `npx.cmd`를 쓴다.
   - 실행 전에 명령 전체를 보여 주고, 실행 후 출력과 종료 코드를 표시한다.
   - `npx`가 없으면 사용 불가로 표시한다.
8. 설치 위치와 링크 처리는 skills CLI에 맡긴다. 하네스는 그 파일들의 소유권을 갖지 않는다. 다만 하네스가 이미 배포한 같은 이름의 스킬이 있으면 충돌로 표시한다.
9. skills CLI로 설치하지 않는 항목(`claude plugin` 등)은 `install` 문구를 보여 주기만 한다.
10. `#ref` 버전 고정이 CLI에서 실제로 동작하는지는 구현 단계에서 실행해 확인한다. 동작하지 않으면 `version`은 정보로만 표시한다.

### docs
11. 2단계에서는 목록만 표시한다.
12. (백로그) TUI가 문서 본문을 받아 `references/<filename>`에 저장하는 기능. `sha256`이 없으면 경고 후 진행하고, 값이 있는데 일치하지 않으면 그 파일은 저장하지 않고 경고한다.

### wiki
13. 항목 필드: `uuid`, `type`(Wiki.js 경로 type을 JSON에 직접 지정, 예: `skill`, `claude/hook`), `target`, `status`
    - `target: "skill"`이면 게이트웨이 v2로 받아 `.skills/`에 가져온다(현행 방식).
    - `target: "doc"`이거나 값이 없으면 목록에만 표시한다.
14. 토큰은 선택 사항이다. TUI에서 입력받아 `.harness/local.toml`(Git 무시 경로)에 저장할 수 있고, 입력하지 않아도 진행한다. 토큰 값은 화면, 로그, 미리보기에 표시하지 않는다.
15. UUID는 소문자로 맞춰 비교한다.
16. (백로그) dynamic(사용할 때 받기) / static(영구 저장) 방식.

### 3단계로 이관
17. 범위 제한 업데이터, 설정 우선순위(override) 계층, 모드, 훅, Git worktree, Worktrunk, Docktree. 현재 `STEP_2` 본문을 거의 그대로 `STEP_3`로 옮긴다.

## 선행 조건
- 1단계 인수 (2026-11-20 사용자 확인)
- 0018 (게이트웨이 v2 대응). 명세의 엔드포인트 표기를 맞추기 위함이다.

## 범위
- 포함 (모두 영/한 동시 작성):
  - `references/STEP_2_ADDITIONAL_FEATURES*.md`를 카탈로그 연동 명세로 재작성
  - `references/STEP_3_*.md` 신규 작성 (기존 2단계 내용 이관)
  - `references/HARNESS_CATALOG_CONTRACT*.md` 신규 작성 (세 파일의 JSON 규격과 예시)
  - `CLAUDE*.md`, `README*.md`의 명세 목록과 단계 설명 갱신
- 제외: 구현, `catalog/*.json` 샘플 파일 작성(구현 지시서에서 함께 작성)

## 작업 항목
1. 3단계 문서로 이관
2. 2단계 명세 재작성 (위 결정 사항 1~16)
3. 카탈로그 규격 문서 작성
4. 안내 문서 갱신
5. 2단계 구현 지시서 초안 분할 (0020 이후)

## 완료 기준 (DoD)
- [ ] 테스트 예외 사유: 문서만 변경
- [ ] 영/한 문서가 같은 내용인지 확인
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
외부 서비스를 새로 만들거나 배포하지 않는다. TUI는 카탈로그를 네트워크로 받지 않는다. 공개 스킬 설치는 사용자가 확인한 경우에만 공식 CLI로 셸 없이 실행한다.
