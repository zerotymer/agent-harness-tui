import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  backupPath,
  CONFIG_FILE,
  ContractError,
  type ContractErrorKind,
  configLayout,
  defaultConfig,
  defaultState,
  fromText,
  HARNESS_CONFIG,
  HARNESS_LOCK,
  HARNESS_STATE,
  type HarnessLock,
  type HarnessState,
  isEnvName,
  isPageType,
  isSha256,
  isSlug,
  isUuid,
  load,
  SCHEMA_VERSION,
  toText,
} from "./contract.js";
import { completedLog, RelPath } from "./paths.js";

const HASH = "sha256:0e306d8fba8b7d173ab0e85ab2d40a93ec393cbc814b586025de5c6f6d5f9c1a";

const CONFIG = `
schema_version = 1

[paths]
references = "references"
output = ".output"

[backlog]
enabled = false
path = "agent.db"

[[agents]]
id = "claude-code"
placement = "copy"

[[agents]]
id = "codex"
placement = "reference"

[[sources]]
id = "wiki"
kind = "wiki-gateway"
token_env = "GATEWAY_TOKEN"

[[sources]]
id = "vendored"
kind = "local-dir"
path = "vendor/skills"

[[skills]]
id = "wiki-skill-import"
source = "wiki"
ref = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67"
type = "skill"

[[skills]]
id = "local-one"
source = "vendored"
ref = "local-one"
`;

const config = () => fromText(HARNESS_CONFIG, CONFIG);

function errorKind(fn: () => unknown): ContractErrorKind | undefined {
  try {
    fn();
  } catch (e) {
    if (e instanceof ContractError) return e.kind;
    throw e;
  }
  return undefined;
}

function invalid(text: string): string[] {
  try {
    fromText(HARNESS_CONFIG, text);
  } catch (e) {
    if (e instanceof ContractError && e.kind === "invalid") return e.problems;
    throw e;
  }
  assert.fail("expected an invalid config");
}

test("config round trips", () => {
  const c = config();
  assert.equal(c.skills[0]?.pageType, "skill");
  assert.deepEqual(fromText(HARNESS_CONFIG, toText(HARNESS_CONFIG, c)), c);
  const d = defaultConfig();
  assert.deepEqual(fromText(HARNESS_CONFIG, toText(HARNESS_CONFIG, d)), d);
});

test("instructions dir defaults when absent", () => {
  // CONFIG predates the field.
  assert.ok(!CONFIG.includes("instructions"));
  assert.equal(config().paths.instructions.toString(), "instructions");
  const custom = CONFIG.replace("[paths]", '[paths]\ninstructions = "docs/tasks"');
  const c = fromText(HARNESS_CONFIG, custom);
  assert.equal(completedLog(configLayout(c)).toString(), "docs/tasks/completed.log");
});

test("version is checked before fields", () => {
  // A newer file with unknown fields reports the version, not the fields.
  const newer = "schema_version = 3\nfuture_key = true\n";
  assert.equal(
    errorKind(() => fromText(HARNESS_CONFIG, newer)),
    "unsupported-version",
  );
  assert.equal(
    errorKind(() => fromText(HARNESS_CONFIG, "[paths]\n")),
    "parse",
  );
  assert.equal(
    errorKind(() => fromText(HARNESS_LOCK, '{"schema_version": 9}')),
    "unsupported-version",
  );
  assert.equal(
    errorKind(() => fromText(HARNESS_STATE, '{"schema_version": 0}')),
    "unsupported-version",
  );
});

test("rejects absolute and escaping paths", () => {
  for (const bad of ["/srv/refs", "C:\\\\work\\\\refs", "../refs", ""]) {
    const text = CONFIG.replace('references = "references"', `references = "${bad}"`);
    assert.equal(
      errorKind(() => fromText(HARNESS_CONFIG, text)),
      "parse",
      bad,
    );
  }
});

test("rejects secrets and unknown keys", () => {
  // No field can hold a token value: unknown keys are refused...
  let text = CONFIG.replace(
    'token_env = "GATEWAY_TOKEN"',
    'token_env = "GATEWAY_TOKEN"\ntoken = "gho_secret"',
  );
  assert.equal(
    errorKind(() => fromText(HARNESS_CONFIG, text)),
    "parse",
  );
  // ...and token_env only accepts a variable name.
  text = CONFIG.replace('token_env = "GATEWAY_TOKEN"', 'token_env = "gho_abc123"');
  assert.match(invalid(text)[0] ?? "", /token_env/);
  // Transient allocations such as ports have no field either.
  text = CONFIG.replace("[backlog]", "[service]\nport = 8080\n\n[backlog]");
  assert.equal(
    errorKind(() => fromText(HARNESS_CONFIG, text)),
    "parse",
  );
});

test("validates references between entries", () => {
  let text = CONFIG.replace('source = "vendored"', 'source = "missing"');
  assert.match(invalid(text)[0] ?? "", /unknown source/);
  text = CONFIG.replace("14cea96f-f1e2-461e-b9a7-96b5a6f63b67", "not-a-uuid");
  assert.match(invalid(text)[0] ?? "", /UUID/);
  text = CONFIG.replace('id = "local-one"', 'id = "Wiki-Skill-Import"');
  assert.match(invalid(text)[0] ?? "", /duplicate skill/);
  text = CONFIG.replace('path = "agent.db"', 'path = "data/agent.db"');
  assert.match(invalid(text)[0] ?? "", /backlog\.path/);
  text = CONFIG.replace('path = "vendor/skills"', 'token_env = "X"');
  const problems = invalid(text);
  assert.ok(
    problems.some((p) => p.includes("needs `path`")),
    problems.join("; "),
  );
});

function lock(): HarnessLock {
  return {
    schemaVersion: SCHEMA_VERSION,
    skills: [
      {
        id: "wiki-skill-import",
        source: "wiki",
        reference: "14cea96f-f1e2-461e-b9a7-96b5a6f63b67",
        pageType: "skill",
        files: [
          {
            path: RelPath.parse("SKILL.md"),
            hash: HASH,
            locale: "en",
            revision: "sha256:upstream",
          },
        ],
      },
    ],
  };
}

test("lock round trips and checks hashes", () => {
  const l = lock();
  assert.deepEqual(fromText(HARNESS_LOCK, toText(HARNESS_LOCK, l)), l);
  const bad = lock();
  bad.skills[0]!.files[0]!.hash = "md5:abc";
  assert.equal(
    errorKind(() => toText(HARNESS_LOCK, bad)),
    "invalid",
  );
  const dup = lock();
  dup.skills[0]!.files.push(dup.skills[0]!.files[0]!);
  assert.match(HARNESS_LOCK.problems(dup)[0] ?? "", /duplicate file/);
});

function state(): HarnessState {
  return {
    schemaVersion: SCHEMA_VERSION,
    lastSuccess: { id: "20261116-140000", at: "2026-11-16T14:00:00+09:00", toolVersion: "0.1.1" },
    managed: [
      { path: RelPath.parse("CLAUDE.md"), block: "agent-harness", hash: HASH },
      { path: RelPath.parse("instructions/completed.log"), hash: HASH },
    ],
    deployments: [
      {
        skill: "wiki-skill-import",
        agent: "claude-code",
        placement: "copy",
        target: RelPath.parse(".claude/skills/wiki-skill-import/SKILL.md"),
        hash: HASH,
      },
    ],
    adapters: [{ agent: "claude-code", version: "1" }],
  };
}

test("state round trips", () => {
  const s = state();
  assert.deepEqual(fromText(HARNESS_STATE, toText(HARNESS_STATE, s)), s);
  const d = defaultState();
  assert.deepEqual(fromText(HARNESS_STATE, toText(HARNESS_STATE, d)), d);
});

test("ownership is file or block, never both", () => {
  let s = state();
  s.managed.push({ path: RelPath.parse("CLAUDE.md"), hash: HASH });
  assert.ok(HARNESS_STATE.problems(s).some((p) => p.includes("both as a whole file")));
  s = state();
  s.deployments[0]!.hash = undefined;
  assert.match(HARNESS_STATE.problems(s)[0] ?? "", /copy needs/);
});

test("backup paths stay under the backups dir", () => {
  const rel = RelPath.parse("docs/CLAUDE.md");
  assert.equal(
    backupPath("20261116-140000", rel)?.toString(),
    ".harness/backups/20261116-140000/docs/CLAUDE.md",
  );
  assert.equal(backupPath("../x", rel), undefined);
  assert.equal(backupPath("", rel), undefined);
});

test("load of a missing file is undefined", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-contract-"));
  try {
    assert.equal(load(HARNESS_CONFIG, dir), undefined);
    fs.writeFileSync(path.join(dir, CONFIG_FILE), CONFIG);
    assert.deepEqual(load(HARNESS_CONFIG, dir), config());
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("format helpers", () => {
  assert.ok(isUuid("01a1e24a-c2fa-72f4-914c-d9af2cc792e2"));
  assert.ok(!isUuid("01a1e24a-c2fa-72f4-914c"));
  assert.ok(isEnvName("GATEWAY_TOKEN") && !isEnvName("gateway") && !isEnvName("1X"));
  assert.ok(isSlug("wiki-skill-import") && !isSlug(".hidden") && !isSlug("a/b"));
  assert.ok(isSha256(HASH) && !isSha256(HASH.toUpperCase()));
});

test("page type allows slug segments", () => {
  for (const ok of ["skill", "claude/hook", "a.b/c_d-e"]) assert.ok(isPageType(ok), ok);
  for (const bad of ["", "/skill", "skill/", "claude//hook", "../x", "a b", "-x/y"]) {
    assert.ok(!isPageType(bad), bad);
  }
});
