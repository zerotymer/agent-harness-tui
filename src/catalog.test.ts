import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { argvProblem, builtinText, currentOs, display, isRfc3339, load, parse } from "./catalog.js";

const UUID = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";
const doc = (body: string) => `{"schema_version":2${body ? `,${body}` : ""}}`;
const agent = (extra = "") =>
  `{"id":"a","title":{"en":"A"},"detect":["a","--version"],"guidance":"A.md","skills_dir":".a/skills"${extra}}`;

function fails(input: string, why: string): void {
  assert.throws(
    () => parse(input),
    (e: Error) => e.message.includes(why),
    `expected \`${why}\``,
  );
}

test("built-in default parses with Claude Code and Codex", () => {
  const c = parse(builtinText());
  assert.ok(c.catalogVersion);
  assert.deepEqual(
    c.agents.map((a) => [a.id, a.guidance, a.skillsDir.toString(), a.default]),
    [
      ["claude-code", "CLAUDE.md", ".claude/skills", true],
      ["codex", "AGENTS.md", ".agents/skills", false],
    ],
  );
  assert.deepEqual(c.agents[0]!.detect, ["claude", "--version"]);
  assert.deepEqual(c.agents[1]!.install.windows, ["npm", "install", "-g", "@openai/codex"]);
  assert.ok(c.skills.length > 0 && c.wikiSkills.length > 0 && c.docs.length > 0);
  assert.equal(c.hooks.length, 1);
});

test("lists are optional; paths and service are read", () => {
  const c = parse(
    doc(
      `"paths":{"instructions":"tasks","artifacts":null},"service":{"url":"https://s","token_env":"S_TOKEN"}`,
    ),
  );
  assert.deepEqual(c.agents, []);
  assert.deepEqual(c.paths, { instructions: "tasks", artifacts: null });
  assert.deepEqual(c.service, { url: "https://s", tokenEnv: "S_TOKEN" });
  fails(doc(`"paths":{"instructions":null}`), "instructions");
  fails(doc(`"service":{"token_env":"lower"}`), "token_env");
});

test("rejects bad envelopes", () => {
  fails("﻿{}", "byte order mark");
  fails("{", "invalid JSON");
  fails("{}", "schema_version");
  fails('{"schema_version":1}', "unsupported schema_version 1");
  fails(doc(`"extra":1`), "unknown field `extra`");
  fails(doc(`"agents":{}`), "must be an array");
});

test("agent entry rules", () => {
  assert.equal(parse(doc(`"agents":[${agent()}]`)).agents[0]!.default, false);
  fails(doc(`"agents":[${agent()},${agent()}]`), "duplicate `id` `a`");
  fails(doc(`"agents":[${agent(',"x":1')}]`), "unknown field `x`");
  fails(doc(`"agents":[${agent().replace('"a","--version"', "")}]`), "`detect` is empty");
  fails(doc(`"agents":[${agent().replace('["a",', '["-a",')}]`), "starts with an option");
  fails(doc(`"agents":[${agent(',"install":{"linux":[]}')}]`), "`install.linux` is empty");
  fails(doc(`"agents":[${agent(',"install":{"beos":["x"]}')}]`), "unknown field `beos`");
  fails(doc(`"agents":[${agent().replace("A.md", "dir/A.md")}]`), "`guidance`");
  fails(doc(`"agents":[${agent().replace(".a/skills", "../x")}]`), "skills_dir");
  fails(doc(`"agents":[${agent(',"default":"yes"')}]`), "`default` must be a boolean");
});

test("skills, wiki skills, docs and hooks entry rules", () => {
  const skill = (extra: string) =>
    doc(`"skills":[{"id":"s","title":{"en":"S"},"installer":"skills-cli"${extra}}]`);
  assert.equal(parse(skill(',"source":"o/r","skill":"x"')).skills[0]!.skill, "x");
  fails(skill(',"source":"-o"'), "`source` must not");
  fails(skill(',"source":"o/r","install":"x"'), "must not have `install`");
  const wiki = (extra: string) =>
    doc(`"wiki_skills":[{"id":"w","title":{"en":"W"},"uuid":"${UUID}","type":"skill"${extra}}]`);
  assert.equal(parse(wiki("")).wikiSkills[0]!.status, "active");
  fails(wiki(',"status":"gone"'), "unknown variant");
  fails(wiki(',"target":"skill"'), "unknown field `target`");
  fails(
    doc(`"docs":[{"id":"d","title":{"en":"D"},"url":"http://x","filename":"a.md"}]`),
    "`https://`",
  );
  fails(doc(`"hooks":[{"id":"h","title":{"en":"H"},"url":"ftp://x"}]`), "`https://`");
  fails(doc(`"hooks":[{"id":"h","title":{"ko":" "}}]`), "`title`");
});

test("project.json wins, is never merged and never falls back", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-catalog-"));
  try {
    assert.equal(load(dir).source, "builtin");
    fs.writeFileSync(path.join(dir, "project.json"), doc(`"agents":[${agent()}]`));
    const l = load(dir);
    assert.equal(l.source, "project");
    assert.deepEqual(
      l.config.agents.map((a) => a.id),
      ["a"],
    );
    assert.deepEqual(l.config.skills, [], "nothing from the default");
    fs.writeFileSync(path.join(dir, "project.json"), '{"schema_version":2,"x":1}');
    assert.throws(() => load(dir), /^Error: project\.json: .*unknown field `x`/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("helpers", () => {
  assert.equal(argvProblem(["npm", "-g"]), undefined);
  assert.equal(currentOs("darwin"), "macos");
  assert.equal(currentOs("aix"), undefined);
  assert.equal(display({ en: "E", ko: "K" }), "K");
  assert.ok(isRfc3339("2026-06-28T12:25:36.744Z"));
  assert.ok(!isRfc3339("2026-06-28"));
});
