# 하네스 카탈로그 규약

스키마 버전: **1**. [2단계](STEP_2_CATALOG_INTEGRATION_ko.md)에서 쓴다. 이 파일들은 배포 책임자가 작성하고 TUI는 읽기만 한다. TUI는 카탈로그를 내려받지 않는다.

| 파일 | `kind` | 용도 |
| --- | --- | --- |
| `wiki.json` | `wiki` | Wiki.js 페이지. 게이트웨이 API v2로 읽는다 |
| `skills.json` | `skills` | 공개 스킬. `npx skills add`로 설치한다 |
| `docs.json` | `docs` | 내려받을 문서 |

## 공통 틀

```json
{
  "schema_version": 1,
  "kind": "skills",
  "catalog_version": "2026.11.20",
  "entries": []
}
```

- `schema_version`(정수)을 가장 먼저 읽는다. 1이 아닌 버전은 미지원이다.
- `kind`는 TUI가 요청한 파일 종류와 같아야 한다.
- `catalog_version`(문자열, 선택)은 배포 책임자가 정하는 자유 형식 표시값이다. 값이 있으면 TUI가 화면에 보여 준다.
- 모든 단계에서 모르는 키는 거부한다.
- UTF-8, BOM 없음.

## 항목 공통 필드

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `id` | 문자열 | 예 | 파일 안에서 대소문자 구분 없이 중복 금지. 제어 문자 금지. |
| `title` | `{ "en"?, "ko"? }` | 예 | 언어 하나 이상. 없는 언어는 생략하고 만들어 넣지 않는다. |
| `summary` | `{ "en"?, "ko"? }` | 아니요 | `title`과 같은 규칙. |
| `version` | 문자열 | 아니요 | 표시용 버전 값(예: 커밋 SHA). |
| `updated` | 문자열 | 아니요 | RFC 3339 시각. |

## `wiki` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `uuid` | 문자열 | 예 | UUID. 소문자로 맞춰 비교한다. |
| `type` | 문자열 | 예 | Wiki.js 경로 type. 슬러그를 `/`로 이은 값(`skill`, `claude/hook`). |
| `target` | `"skill"` \| `"doc"` | 아니요 | `skill`이면 `.skills/<id>/`로 가져올 수 있다. 그 밖에는 목록에만 표시한다. |
| `status` | `"active"` \| `"archived"` | 아니요 | 기본값 `active`. 보관된 항목은 가져올 수 없다. |

`target`이 `skill`이면 `id`가 디렉터리 이름이 되므로 슬러그(`[A-Za-z0-9._-]+`, `.`이나 `-`로 시작하지 않음)여야 한다.

```json
{ "id": "wiki-skill-import", "uuid": "14cea96f-f1e2-461e-b9a7-96b5a6f63b67",
  "type": "skill", "target": "skill", "status": "active",
  "title": { "en": "wiki-skill-import", "ko": "wiki-skill-import" },
  "updated": "2026-06-28T12:25:41.928Z" }
```

## `skills` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `installer` | 문자열 | 예 | `skills-cli`는 TUI가 실행한다. 다른 값은 표시만 한다. |
| `source` | 문자열 | 예 | 저장소 URL 또는 `owner/repo`. 사용자에게 보여 준다. `skills-cli`에서는 CLI에 그대로 넘긴다. |
| `skill` | 문자열 | 아니요 | `skills-cli`만. `-s`에 넘기는 스킬 이름으로, 스킬 `SKILL.md` frontmatter의 `name`이다. 생략하거나 `*`이면 source의 모든 스킬을 설치하며 `-s`를 넘기지 않는다. |
| `install` | 문자열 | 다른 설치 도구만 | 사용자가 실행하도록 보여 주는 명령 문구. TUI는 실행하지 않는다. |

- `source`, `skill`은 `-`로 시작할 수 없고 공백·제어 문자를 포함할 수 없다. 셸 없이 각각 별도 인자로 넘긴다.
- `skills-cli` 항목에는 `install`을 둘 수 없다. 다른 설치 도구 항목에는 `skill`을 둘 수 없다.
- 설치용 버전 필드는 없다. 버전을 고정하려면 배포 책임자가 skills CLI 문법으로 `source`에 적는다(예: `owner/repo@<커밋 SHA>`, `owner/repo#<태그>`). TUI는 `source`를 해석하지 않고 그대로 넘긴다. `version`은 표시용으로만 쓴다.

```json
{ "id": "security-audit", "installer": "skills-cli",
  "source": "cloudflare/security-audit-skill@c1c8a8c1471069fb0e188eeaff69b8e8db6564a8", "skill": "security-audit",
  "title": { "en": "security-audit" },
  "summary": { "en": "Multi-phase security audits.", "ko": "다단계 보안 감사 스킬." },
  "version": "c1c8a8c1471069fb0e188eeaff69b8e8db6564a8" }

{ "id": "ponytail@ponytail", "installer": "claude-code-cli",
  "source": "https://github.com/DietrichGebert/ponytail",
  "install": "claude plugin marketplace add DietrichGebert/ponytail && claude plugin install ponytail@ponytail",
  "title": { "en": "ponytail" } }
```

## `docs` 항목

| 필드 | 타입 | 필수 | 규칙 |
| --- | --- | --- | --- |
| `url` | 문자열 | 예 | `https://` URL. 가능하면 커밋으로 고정한다. |
| `filename` | 문자열 | 예 | 경로 구분자가 없는 파일 이름. `references/<filename>`으로 저장한다(백로그). |
| `sha256` | 문자열 | 아니요 | 파일의 소문자 16진수 64자리. |

```json
{ "id": "anthropics-skills-readme",
  "url": "https://raw.githubusercontent.com/anthropics/skills/e4bb5d960d87126ff3b3a8af1a104b7db67560ba/README.md",
  "filename": "anthropics-skills-README.md",
  "title": { "en": "Anthropic skills README" },
  "version": "e4bb5d960d87126ff3b3a8af1a104b7db67560ba" }
```

## 로컬 토큰 파일

`.harness/local.toml`은 머신별 값을 담는다. `.harness/`가 Git에서 무시되므로 이 파일도 무시된다. 2단계는 키 하나만 정의한다.

```toml
schema_version = 1

[gateway]
token = "..."   # 선택 사항. 읽기 전용 게이트웨이 토큰
```

모르는 키는 거부한다. 3단계에서 머신·worktree override 키를 추가할 수 있다.
