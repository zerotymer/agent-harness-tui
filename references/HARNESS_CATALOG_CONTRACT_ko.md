# 하네스 프로젝트 구성 규약

스키마 버전: **2**. [2단계](STEP_2_SETUP_FLOW_ko.md)에서 쓴다. 버전 1(별도 파일 세 개 `wiki.json`, `skills.json`, `docs.json`)은 대체되었으며 더 읽지 않는다.

| 파일 | 위치 | 역할 |
| --- | --- | --- |
| `project.json` | 프로젝트 루트, Git으로 관리 | 프로젝트 구성. 프로젝트 소유자가 작성한다 |
| `default.json` | `catalog/default.json`으로 실행 파일에 내장 | `project.json`이 없을 때만 쓴다 |

두 파일은 병합하지 않는다. TUI는 읽기만 하며 내려받지 않는다.

## 최상위

```json
{
  "schema_version": 2,
  "catalog_version": "2026.11.24",
  "paths": { "instructions": "instructions", "artifacts": ".artifacts", "docs": "docs", "references": "references" },
  "service": { "url": "https://harness.example.internal", "project": "my-project", "token_env": "HARNESS_SERVICE_TOKEN" },
  "agents": [],
  "skills": [],
  "wiki_skills": [],
  "docs": [],
  "hooks": []
}
```

- `schema_version`(정수)을 가장 먼저 읽는다. 2가 아닌 버전은 미지원이다.
- `catalog_version`(문자열, 선택)은 자유 형식 표시값이다. 값이 있으면 TUI가 보여 준다.
- `paths`(선택)는 경로 화면의 초깃값이다. 키마다 선택 사항이다. 값이 `null`이면 그 선택 경로를 사용 안 함으로 시작한다. `instructions`는 `null`일 수 없다.
- `service`(선택)는 콘텐츠 서비스 설정의 초깃값이다. `token_env`는 `[A-Z_][A-Z0-9_]*` 형식의 환경변수 이름이며 토큰 값이 아니다.
- 모든 목록은 선택 사항이고 기본값은 빈 목록이다.
- 모든 단계에서 모르는 키는 거부한다.
- UTF-8, BOM 없음.

## 항목 공통 필드

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `id` | 문자열 | 예 | 목록 안에서 대소문자 구분 없이 중복 금지. 제어 문자 금지. |
| `title` | `{ "en"?, "ko"? }` | 예 | 언어 하나 이상. 없는 언어는 생략하고 번역을 지어내지 않는다. |
| `summary` | `{ "en"?, "ko"? }` | 아니요 | `title`과 같은 규칙. |
| `version` | 문자열 | 아니요 | 표시용 버전 값(예: 커밋 SHA). |
| `updated` | 문자열 | 아니요 | RFC 3339 시각. |

## 인자 배열

명령은 문자열 JSON 배열이며 셸 없이 실행한다.

- 요소가 하나 이상이다. 빈 요소나 제어 문자가 든 요소는 없다.
- 첫 요소는 실행할 프로그램이며 `-`로 시작할 수 없다.
- TUI는 요소를 문자열 하나로 합치지 않으며 셸에 넘기지 않는다. 예외는 Node.js가 `cmd.exe`로만 실행할 수 있는 Windows `.cmd`·`.bat` 프로그램 하나다. 이때 모든 요소에 공백과 `cmd` 특수문자(`& | < > ^ % ! " ( )`)가 없어야 하며, 있으면 명령을 거부한다(2단계 명세 §3).

## `agents` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `id` | 문자열 | 예 | 슬러그(`[A-Za-z0-9._-]+`, `.`이나 `-`로 시작하지 않음). `harness.toml`에 저장한다. |
| `detect` | 인자 배열 | 예 | 종료 코드 0이면 설치된 것이다. 출력 첫 줄을 버전으로 보여 준다. |
| `guidance` | 문자열 | 예 | 프로젝트 루트의 지침 파일 이름. 예: `CLAUDE.md`, `AGENTS.md`. 경로 구분자 없이 `.md`로 끝난다. 한국어 짝은 `<이름>_ko.md`다. |
| `skills_dir` | 문자열 | 예 | 에이전트가 스킬을 찾는 프로젝트 상대 디렉터리. |
| `install` | `{ "linux"?, "macos"?, "windows"? }` | 아니요 | OS별 인자 배열. 없는 OS에서는 설치를 사용할 수 없다. |
| `default` | 불리언 | 아니요 | 에이전트 화면에서 미리 선택한다. 기본값 `false`. |

```json
{ "id": "claude-code", "title": { "en": "Claude Code" },
  "detect": ["claude", "--version"],
  "guidance": "CLAUDE.md", "skills_dir": ".claude/skills",
  "install": {
    "linux":   ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"],
    "macos":   ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"],
    "windows": ["npm", "install", "-g", "@anthropic-ai/claude-code@latest"] },
  "default": true }
```

내장 에이전트는 npm으로 설치한다(제약 사항은 2단계 명세 §3). Windows에서는 `PATHEXT` 확장자를 붙여 프로그램을 찾으므로 `npm`이 `npm.cmd`를 찾는다. 인자 배열은 모든 OS에서 같다.

## `skills` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `installer` | 문자열 | 예 | `skills-cli`는 TUI가 실행한다. 다른 값은 표시만 한다. |
| `source` | 문자열 | 예 | 저장소 URL 또는 `owner/repo`. 사용자에게 보여 준다. `skills-cli`에서는 CLI에 그대로 넘긴다. |
| `skill` | 문자열 | 아니요 | `skills-cli`만. `-s`에 넘기는 스킬 이름으로, 스킬 `SKILL.md` frontmatter의 `name`이다. 생략하거나 `*`이면 source의 모든 스킬을 설치하며 `-s`를 넘기지 않는다. |
| `install` | 문자열 | 다른 설치 도구만 | 사용자가 실행하도록 보여 주는 명령 문구. TUI는 실행하지 않는다. |

- `source`, `skill`은 `-`로 시작할 수 없고 공백·제어 문자를 포함할 수 없다. 셸 없이 각각 별도 인자로 넘긴다.
- `skills-cli` 항목에는 `install`을 둘 수 없다. 다른 설치 도구 항목에는 `skill`을 둘 수 없다.
- 설치용 버전 필드는 없다. 버전을 고정하려면 skills CLI 문법으로 `source`에 적는다(예: `owner/repo@<커밋 SHA>`, `owner/repo#<태그>`). TUI는 `source`를 그대로 넘긴다. `version`은 표시용이다.

```json
{ "id": "security-audit", "installer": "skills-cli",
  "source": "cloudflare/security-audit-skill@c1c8a8c1471069fb0e188eeaff69b8e8db6564a8", "skill": "security-audit",
  "title": { "en": "security-audit" },
  "summary": { "en": "Multi-phase security audits.", "ko": "다단계 보안 감사 스킬." } }
```

## `wiki_skills` 항목

Wiki.js 스킬 페이지. `.skills/<id>/`에 static 사본으로 가져온다.

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `id` | 문자열 | 예 | 슬러그(`[A-Za-z0-9._-]+`, `.`이나 `-`로 시작하지 않음). 디렉터리 이름이 된다. |
| `uuid` | 문자열 | 예 | UUID. 소문자로 맞춰 비교한다. |
| `type` | 문자열 | 예 | Wiki.js 경로 type. 슬러그를 `/`로 이은 값(`skill`, `claude/hook`). |
| `status` | `"active"` \| `"archived"` | 아니요 | 기본값 `active`. 보관된 항목은 가져올 수 없다. |

```json
{ "id": "wiki-skill-import", "uuid": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67", "type": "skill",
  "title": { "en": "wiki-skill-import" }, "updated": "2026-06-28T12:25:41.928Z" }
```

## `docs` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `url` | 문자열 | 예 | `https://` URL. 가능하면 커밋으로 고정한다. |
| `filename` | 문자열 | 예 | 경로 구분자가 없는 파일 이름. `<references>/<filename>`으로 저장한다. |
| `sha256` | 문자열 | 아니요 | 파일의 소문자 16진수 64자리. |

```json
{ "id": "anthropics-skills-readme",
  "url": "https://raw.githubusercontent.com/anthropics/skills/e4bb5d960d87126ff3b3a8af1a104b7db67560ba/README.md",
  "filename": "anthropics-skills-README.md",
  "title": { "en": "Anthropic skills README" } }
```

## `hooks` 항목

참고 항목일 뿐이다. 아무것도 설치하거나 실행하지 않는다.

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `url` | 문자열 | 아니요 | 참고 문서의 `https://` 링크. |

## 로컬 토큰 파일

`.harness/local.toml`은 머신별 값을 담는다. `.harness/`가 Git에서 제외되므로 이 파일도 제외된다.

```toml
schema_version = 1

[gateway]
token = "..."   # 선택 사항. 읽기 전용 위키 게이트웨이 토큰

[service]
token = "..."   # 선택 사항. 콘텐츠 서비스 토큰
```

- `token_env`가 가리키는 환경변수의 토큰이 저장된 토큰보다 우선한다.
- 모르는 키는 거부한다. 3단계에서 머신·worktree override 키를 추가할 수 있다.
