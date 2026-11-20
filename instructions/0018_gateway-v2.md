# 0018 — 위키 게이트웨이 v2 대응 (1단계 버그 수정)

> Entity UUID: 01a1f80c-e91d-7389-8857-2a2178453de7   (로컬 생성, UUIDv7)
> Status: IN_PROGRESS
> Branch: `fix/gateway-v2/01a1f80c`
> 근거: 사용자 확인 (2026-11-20) "wikijs가 2.0으로 업데이트", "토큰 입력하되 없어도 진행"

## 목표
게이트웨이 `wikijs-agent-gateway`가 2.0.0으로 바뀌면서 1단계의 위키 스킬 가져오기가 동작하지 않는다. 지금 코드는 v1 경로 `GET /pages/by-uuid`를 호출하는데, 이 경로는 404를 반환한다. TUI는 이 404를 "페이지 없음"으로 보고하므로, 사용자는 원인이 API 변경이라는 것을 알 수 없다. 이 지시서는 v2 계약에 맞게 고친다.

## 확인된 v2 계약 (2026-11-20, `GET /api/v2/openapi.json` 기준)
- `GET /api/v2/documents/{uuid}[?type=<type>]`
  - 응답 형태는 v1과 같다: `pages.en`/`pages.ko` 아래에 `content`와 `contentHash`가 있다.
  - UUID는 대소문자를 구분하지 않는다.
  - `type`을 생략하면 게이트웨이가 실제 경로를 찾아서 응답의 `type`에 넣어 준다.
  - 오류: 404 `page-not-found`, 409 (대소문자만 다른 중복 페이지), 422, 502. 오류 응답은 `application/problem+json`이다.
- 읽기에는 인증이 필요 없다. 쓰기 엔드포인트만 Bearer 토큰을 요구한다.
- v1 경로(`/pages/by-uuid` 등)는 모두 404를 반환한다.

## 선행 조건
- 없음. 1단계 인수 후 발생한 버그 수정이다.
- `.skills/wiki-skill-import` 사본 갱신은 위키 원본 페이지(`14cea96f`)가 v2 안내로 바뀐 뒤에 한다. 2026-11-20 기준 위키에는 아직 v1 안내가 남아 있다.

## 범위
- 포함:
  1. `src/sources.rs` 게이트웨이 클라이언트
     - 경로를 `/api/v2/documents/{uuid}`로 바꾼다.
     - 토큰을 선택 사항으로 바꾼다. `token_env`가 가리키는 환경변수가 설정되어 있으면 `Authorization` 헤더를 보내고, 없으면 헤더 없이 요청한다(`MissingToken` 오류 삭제).
     - 401/403을 받으면 토큰이 필요하다고 안내한다.
  2. `type`이 `claude/hook`처럼 여러 단계 경로도 받도록 한다. `/`로 나눈 각 단계가 slug여야 하고, `..`이나 빈 단계는 거부한다. 클라이언트와 `harness.toml` 검증(`contract.rs`)을 함께 고친다.
  3. 404 응답 본문이 v2 problem 형식이 아니면 "게이트웨이 API 불일치"로 따로 보고한다. 경로가 바뀐 것을 "페이지 없음"으로 오해하지 않게 하기 위함이다.
  4. 문서 영/한 동시 수정:
     - `references/STEP_1_INITIAL_CREATION*.md` §4: 엔드포인트, 토큰 선택 사항
     - `references/HARNESS_STATE_CONTRACT*.md`: `type` 규칙
     - `docs/USAGE*.md`
  5. 위키 페이지가 v2 안내로 갱신된 경우에만 `.skills/wiki-skill-import`를 다시 가져오고 `source.json`을 갱신한다.
- 제외:
  - 카탈로그 연동 (0019 이후 2단계)
  - 토큰 입력 UI와 `.harness/local.toml` 저장 (2단계)

## 작업 항목
1. 클라이언트 수정 (경로, 선택적 토큰, 다단계 `type`, API 불일치 보고)
2. `contract.rs`의 `type` 검증 수정
3. 테스트 (로컬 목 서버 사용, 네트워크 없음):
   - v2 경로로 요청하는지
   - 토큰이 없으면 `Authorization` 헤더 없이 요청하는지
   - 토큰이 있으면 헤더를 보내는지, 그리고 토큰 값이 오류 메시지에 나오지 않는지
   - `type=claude/hook`을 허용하고 `../x`, 빈 단계는 거부하는지
   - problem 형식이 아닌 404는 API 불일치로 보고하는지
4. 문서 영/한 수정
5. 실제 게이트웨이로 수동 확인: `wiki-skill-import` 가져오기를 해 보고 `contentHash`가 일치하는지 확인

## 산출물
- `src/sources.rs`, `src/contract.rs`와 각 테스트
- 위 문서들의 영/한 수정본

## 완료 기준 (DoD)
- [ ] 테스트 포함 (fmt·clippy·test 통과)
- [ ] 명세 영/한 동시 수정
- [ ] 가드레일 준수 확인
- [ ] 브랜치에서 작업 후 PR 준비 (main 직접 push 금지)
- [ ] 완료는 사용자가 명시적으로 선언한다

## 가드레일 확인
게이트웨이는 읽기만 한다. Wiki.js GraphQL이나 HTML에는 접근하지 않는다. 토큰 값은 파일, 로그, 화면에 남기지 않는다.
