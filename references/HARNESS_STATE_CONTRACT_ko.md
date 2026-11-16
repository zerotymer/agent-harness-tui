# 하네스 설정·잠금·상태 규약

스키마 버전: **1**. 구현 위치는 `src/contract.rs`(지시서 0003)다. 이 규약은 1단계 명세 §3에 따라 TUI가 **대상 프로젝트**에 쓰는 파일을 다룬다. 이 저장소 자체의 설정이 아니다.

| 파일 | 역할 | Git |
| --- | --- | --- |
| `harness.toml` | 공유 의도: 경로, 에이전트, 소스, 선택 스킬, 배치 방식, 백로그 옵션 | 포함 |
| `harness.lock.json` | 확정된 가져오기: 소스·ref, 파일 목록, 해시 | 포함 |
| `.harness/state.json` | 마지막 성공 적용, 소유권, 기준 해시, 배치, 어댑터 | 제외 |
| `.harness/backups/<apply-id>/<path>` | 도구가 변경한 각 파일의 적용 전 사본 | 제외 |
| `.harness/apply.lock` | 적용 중에만 존재한다. 남아 있으면 사용자가 지울 때까지 적용을 막는다 | 제외 |

## 공통 규칙

- 모든 파일에는 정수 `schema_version`이 있다. 버전은 다른 필드보다 **먼저** 읽는다. 버전이 없으면 파싱 오류다. 1이 아닌 버전은 미지원으로 보고하고 파일은 변경하지 않는다. 1단계에는 마이그레이션이 없으며, 백업을 동반한 마이그레이션은 2단계에서 추가한다.
- 모든 테이블에서 모르는 키를 거부한다. 이 방식으로 일시적 정보(포트, 서비스 할당)와 섞여 들어온 비밀 값이 공유 파일에 들어가지 않게 한다.
- 모든 경로는 프로젝트 상대 `RelPath`다. `/`로 구분하고 정규화하며, 절대 경로나 루트 이탈 경로는 허용하지 않는다. Windows 드라이브 경로와 `..` 이탈은 파일을 파싱할 때 거부한다. 심볼릭 링크는 경로를 해석할 때 실제 파일 시스템 기준으로 확인한다(지시서 0002).
- ID(`source`, `skill`, 적용 `id`)는 슬러그다. 형식은 `[A-Za-z0-9._-]+`이며 `.`이나 `-`로 시작하지 않는다. 중복 여부는 대소문자를 무시하고 비교한다.
- 해시는 `sha256:` 뒤에 소문자 16진수 64자리가 오는 형식이며, 실제로 쓴 바이트를 기준으로 계산한다.
- 규약 모듈은 파싱·검증·직렬화만 한다. 쓰기는 적용 엔진(지시서 0004)을 거친다. 적용 엔진은 각 파일을 먼저 읽으므로 미지원 버전 파일은 덮어쓰지 않는다.

## `harness.toml`

```toml
schema_version = 1

[paths]
references = "references"   # 사용자 변경 가능
output = ".output"          # 사용자 변경 가능, 마지막 이름은 "."으로 시작

[backlog]
enabled = false
path = "agent.db"           # 1단계 고정

[[agents]]
id = "claude-code"          # claude-code | codex
placement = "copy"          # copy | reference

[[sources]]
id = "wiki"
kind = "wiki-gateway"       # https://md-gw.zerotymer.net (고정, 저장하지 않음)
token_env = "GATEWAY_TOKEN" # 변수 이름만, 값은 절대 저장하지 않음

[[sources]]
id = "vendored"
kind = "local-dir"
path = "vendor/skills"      # 프로젝트 상대 경로

[[skills]]
id = "wiki-skill-import"    # .skills/<id>/ 디렉터리 이름이기도 함
source = "wiki"
ref = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67"  # wiki-gateway는 페이지 UUID
type = "skill"              # 선택, wiki-gateway 전용

[[skills]]
id = "local-one"
source = "vendored"
ref = "local-one"           # 로컬 소스 기준 상대 디렉터리
```

`instructions/`와 `.skills/`는 1단계에서 고정이므로 저장하지 않는다. 검증 규칙은 다음과 같다.

- `backlog.path`는 `agent.db`여야 한다.
- 에이전트·소스·스킬 ID는 고유해야 한다.
- `local-dir`에는 `path`가 필요하고 `token_env`는 쓸 수 없다.
- `wiki-gateway`에는 `token_env`(형식 `[A-Z_][A-Z0-9_]*`)가 필요하고 `path`는 쓸 수 없다.
- 각 스킬은 존재하는 소스를 참조해야 한다. `wiki-gateway` 스킬의 `ref`는 UUID여야 하고, `type`은 슬러그여야 한다.

공유 파일에는 절대 경로를 저장하지 않으므로, 프로젝트 밖의 로컬 디렉터리는 여기에 저장할 수 없다. 그런 디렉터리는 한 번만 가져오는 데 쓸 수 있다. 이때 잠금 파일에는 소스 ID와 해시가 남는다.

## `harness.lock.json`

```json
{
  "schema_version": 1,
  "skills": [
    {
      "id": "wiki-skill-import",
      "source": "wiki",
      "ref": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67",
      "type": "skill",
      "files": [
        { "path": "SKILL.md", "hash": "sha256:…", "locale": "en", "revision": "sha256:…" },
        { "path": "SKILL_ko.md", "hash": "sha256:…", "locale": "ko", "revision": "sha256:…" }
      ]
    }
  ]
}
```

- `files[].path`는 `.skills/<id>/` 기준 상대 경로다.
- `hash`는 로컬에 쓴 바이트의 해시다.
- `revision`은 소스가 알려 준 리비전(게이트웨이 `contentHash`)을 그대로 기록한 값이다.
- `locale`은 선택 항목이다.
- 스킬마다 파일이 하나 이상 있어야 하고, 파일 경로는 중복되지 않아야 한다.

## `.harness/state.json`

```json
{
  "schema_version": 1,
  "last_success": { "id": "20261116-140000", "at": "2026-11-16T14:00:00+09:00", "tool_version": "0.1.0" },
  "managed": [
    { "path": "CLAUDE.md", "block": "agent-harness", "hash": "sha256:…" },
    { "path": ".skills/wiki-skill-import/SKILL.md", "hash": "sha256:…" }
  ],
  "deployments": [
    { "skill": "wiki-skill-import", "agent": "claude-code", "placement": "copy",
      "target": ".claude/skills/wiki-skill-import/SKILL.md", "hash": "sha256:…" }
  ],
  "adapters": [ { "agent": "claude-code", "version": "1" } ]
}
```

- **`last_success`**: 계획한 쓰기가 모두 성공한 뒤에만 기록한다. `id`는 백업 디렉터리 이름으로도 쓴다. `at`은 UTC RFC 3339 시각(`…Z`)이다. `id`는 같은 시각을 `YYYYMMDD-HHMMSS`로 쓴 값이며, 같은 이름의 백업 디렉터리가 있으면 `-2`, `-3`, …을 붙인다.
- **`managed`**: 소유권을 기록한다. `block`이 없는 항목은 도구가 파일 전체를 소유한다는 뜻이다. `block`이 있는 항목은 도구가 해당 관리 블록만 소유한다는 뜻이고, 파일의 나머지는 사용자 소유다. 한 경로를 두 방식으로 동시에 소유하지 않는다. `hash`는 소유한 내용의 기준 해시다. 빈 `instructions/completed.log`처럼 없을 때만 만드는 파일은 소유하지 않으며 기록하지도 않는다.
- **`deployments`**: 에이전트별 스킬 배치다. `copy` 배치에는 해시가 있어야 하고, `reference` 배치에는 해시가 없다. 배치 대상은 중복되지 않는다.
- **`adapters`**: 도구가 에이전트별로 구현한 어댑터 규약 버전이다.

## 관리 블록 마커

관리 블록은 ID가 같은 시작 마커와 끝 마커 사이의 줄이다. ID는 슬러그다. 마커는 한 줄을 단독으로 차지하며, 앞뒤 공백은 무시한다.

| 파일 종류 | 시작 | 끝 |
| --- | --- | --- |
| Markdown (`AGENTS.md`, `CLAUDE.md` 등) | `<!-- agent-harness:begin ID -->` | `<!-- agent-harness:end ID -->` |
| `#` 주석 (`.gitignore`) | `# agent-harness:begin ID` | `# agent-harness:end ID` |

- 마커 밖의 내용은 사용자 소유이며 바이트 단위로 보존한다. CRLF 줄바꿈, 마지막 줄바꿈이 없는 상태, BOM도 그대로 둔다.
- 새 블록은 사용자 내용 뒤에 빈 줄 하나를 두고 덧붙인다. 이후 실행에서는 그 자리에서 교체하므로 중복되지 않는다.
- `managed[].hash`의 블록 해시는 마커 사이의 줄을 LF로 정규화해 계산한다. 따라서 줄바꿈 형식만 바뀐 것은 수정으로 보지 않는다.
- `agent-harness:`가 들어 있지만 정확한 마커가 아닌 줄은 충돌이다. 중복 ID, 중첩 블록, 끝 마커 누락, 시작 없는 끝 마커, ID가 맞지 않는 끝 마커도 충돌이다. 마커를 고치려고 파일 전체를 다시 쓰지 않는다.
- 블록 본문에는 `agent-harness:` 문자열을 넣을 수 없다.

## 2단계 상태 항목 대응

2단계 명세 §8이 요구하는 상태 항목을 1단계에서는 다음과 같이 기록한다.

| 2단계 항목 | 1단계 필드 |
| --- | --- |
| 안정 ID | 소스·스킬 `id` |
| 소스 리비전 | 잠금 파일 `ref`, `files[].revision`, `files[].hash` |
| 활성화 여부 | `harness.toml`에 있는지 여부 (명시적 비활성화는 2단계) |
| 관리 파일·블록 | 상태 `managed` |
| 기준 해시 | 상태 `managed[].hash`, `deployments[].hash` |
| 복사·참조 매핑 | 상태 `deployments` |
| 지원 어댑터 버전 | 상태 `adapters` |
| 적용 결과 | 상태 `last_success` (실패는 보고만 하고 저장하지 않음) |
| 저장소·worktree 식별자 | 기록하지 않음: 1단계는 프로젝트 로컬 상태만 사용 |
