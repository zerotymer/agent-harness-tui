---
name: wiki-skill-import
description: Use when reading Wiki.js content by UUID through the fixed read-only gateway server or its MCP read tool; retrieves English and Korean Markdown without direct Wiki.js API access, curl, or scraping.
---

# Wiki Skill Import

Read Wiki.js content by UUID through the fixed gateway server. Agents must not
connect directly to the Wiki.js GraphQL API. The gateway is the only normal data
access path.

This skill supports two read interfaces:

- **MCP read interface**: use the gateway MCP server when the client supports it.
- **REST read interface**: use the gateway HTTP endpoint when MCP is unavailable.

Both interfaces are read-only for this skill.

## Fixed Server

```text
GATEWAY_URL=https://md-gw.zerotymer.net
GATEWAY_MCP_URL=https://md-gw.zerotymer.net/mcp
GATEWAY_TOKEN=<token>
```

The gateway is read-only for agent use. This skill only describes content reads.

## Access Rules

- Use only the fixed gateway server for structured content reads.
- Prefer the MCP read tool when the client supports MCP.
- Use the REST read endpoint when MCP is unavailable.
- Use `Authorization: Bearer <GATEWAY_TOKEN>` for REST gateway requests.
- Do not access Wiki.js GraphQL directly.
- Do not use `curl`.
- Do not fetch Wiki.js HTML as the primary data source.
- Do not scrape, parse, or extract Markdown from rendered HTML.
- Do not modify Wiki.js content from this skill.

`https://md.zerotymer.net/` is the Wiki.js web UI. Use it only as a secondary
human-facing reference when gateway access fails and the user needs help checking
whether the page exists. It is not a data source for automated import.

## MCP Read

When MCP is available, connect to:

```text
https://md-gw.zerotymer.net/mcp
```

Use the read tool corresponding to `get_page_by_uuid`.

Input:

```json
{
  "uuid": "550e8400-e29b-41d4-a716-446655440000",
  "type": "skill"
}
```

The `type` field is optional. Include it only when the caller provides a path
prefix such as `skill`, `guide`, or `agent`. If no type is provided, omit it.

MCP read tools are the preferred interface because they avoid shell examples and
keep the agent inside a structured tool contract.

## REST Read

When MCP is unavailable, retrieve English and Korean content together through the
gateway REST read endpoint:

```text
GET /pages/by-uuid?uuid=<uuid>[&type=<type>]
Authorization: Bearer <GATEWAY_TOKEN>
```

The `type` parameter follows the same rule as the MCP input: pass it only when
the caller provides a path prefix.

## Response Shape

Both MCP and REST reads return the same logical shape:

```json
{
  "uuid": "550e8400-e29b-41d4-a716-446655440000",
  "type": "skill",
  "pages": {
    "en": {
      "id": 10,
      "path": "skill/550e8400-...",
      "locale": "en",
      "title": "My Skill",
      "description": "...",
      "content": "---\nname: my-skill\n---\n# My Skill\n...",
      "tags": ["skill"],
      "contentHash": "sha256..."
    },
    "ko": {
      "id": 11,
      "path": "skill/550e8400-...",
      "locale": "ko",
      "title": "내 스킬",
      "description": "...",
      "content": "---\nname: my-skill\n---\n# 내 스킬\n...",
      "tags": ["skill"],
      "contentHash": "sha256..."
    }
  }
}
```

If a locale is missing, that locale is `null`. If both locales are missing,
report `PAGE_NOT_FOUND`.

## Modes

Dynamic reference mode:

1. Read the latest content through the gateway at request time.
2. Prefer MCP; use REST only when MCP is unavailable.
3. Use the returned Markdown as task-local context.
4. Do not write project files.

Import mode:

1. Read the latest content through the gateway at request time.
2. Prefer MCP; use REST only when MCP is unavailable.
3. Write local files only when the user explicitly asks to import, install,
   refresh, save, or apply the content permanently.
4. Preserve English and Korean as paired files.

## Import Layout

When importing a skill into a project, write:

```text
skills/<skill-name>/
  SKILL.md
  SKILL_ko.md
```

When exporting a revised Wiki.js document inside this repository, do not edit
`md/`. Write paired copies under:

```text
.output/<uuid>/
  en.md
  ko.md
  meta.json
```

## Content Rules

- Use `pages.en.content` as the English Markdown source.
- Use `pages.ko.content` as the Korean Markdown source.
- Keep both locales semantically equivalent.
- Keep frontmatter aligned unless a field is language-specific.
- For skill files, the final `SKILL.md` must start with skill frontmatter
  containing `name` and `description`; remove Wiki.js page frontmatter if it
  would appear before the skill frontmatter.

## Error Handling

| Situation | Action |
| --- | --- |
| MCP unavailable | Use the REST read endpoint if `GATEWAY_TOKEN` is available |
| Missing gateway token for REST | Report that `GATEWAY_TOKEN` is unavailable; stop |
| Gateway unreachable | Report the fixed gateway URL and UUID; stop |
| 401 / 403 | Report authentication or permission failure; stop |
| Both locales missing | Report `PAGE_NOT_FOUND`; stop unless best-effort is requested |
| One locale missing | Use the available locale only if the user accepts a partial import |
| Empty `content` | Warn and do not write an empty file |
| Missing skill `name` | Derive a slug from `title`, warn, and continue only in import mode |

