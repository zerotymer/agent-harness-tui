import assert from "node:assert/strict";
import { test } from "node:test";
import {
  builtinText,
  DOCS,
  display,
  isRfc3339,
  loadBuiltin,
  parse,
  SKILLS,
  SKILLS_CLI,
  WIKI,
} from "./catalog.js";

const UUID = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";

const wiki = (entries: string) => `{"schema_version":1,"kind":"wiki","entries":[${entries}]}`;
const wikiEntry = (extra = "") =>
  `{"id":"a","uuid":"${UUID}","type":"skill","title":{"en":"A"}${extra}}`;
const skills = (entry: string) =>
  parse(SKILLS, `{"schema_version":1,"kind":"skills","entries":[${entry}]}`);
const docs = (entry: string) =>
  parse(DOCS, `{"schema_version":1,"kind":"docs","entries":[${entry}]}`);

function fails(fn: () => unknown, why: string, label: string): void {
  assert.throws(fn, (e: Error) => e.message.includes(why), `${label}: expected \`${why}\``);
}

test("built-in samples parse", () => {
  // CI check: the shipped samples follow the contract.
  const w = loadBuiltin(WIKI);
  assert.ok(w.entries.some((e) => e.target === "skill"));
  assert.ok(w.catalogVersion);
  const s = loadBuiltin(SKILLS);
  assert.ok(s.entries.some((e) => e.installer === SKILLS_CLI));
  assert.ok(s.entries.some((e) => e.installer !== SKILLS_CLI));
  loadBuiltin(DOCS);
  assert.ok(builtinText("docs").includes('"kind"'));
});

test("optional fields and defaults", () => {
  let p = parse(WIKI, wiki(wikiEntry()));
  assert.equal(p.catalogVersion, undefined, "catalog_version is optional");
  assert.equal(p.entries[0]?.status, "active");
  assert.equal(p.entries[0]?.target, undefined);
  assert.equal(display(p.entries[0]?.title ?? {}), "A");
  p = parse(
    WIKI,
    wiki(
      wikiEntry(
        ',"target":"skill","status":"archived","updated":"2026-06-28T12:25:41.928+09:00","summary":{"ko":"요약"}',
      ),
    ),
  );
  assert.equal(p.entries[0]?.status, "archived");
});

test("rejects bad envelopes", () => {
  for (const [text, why] of [
    ['{"schema_version":2,"kind":"wiki","entries":[]}', "unsupported"],
    ['{"schema_version":"1","kind":"wiki","entries":[]}', "schema_version"],
    ['{"kind":"wiki","entries":[]}', "schema_version"],
    ['{"schema_version":1,"kind":"docs","entries":[]}', "kind"],
    ['{"schema_version":1,"kind":"wiki","entries":[],"extra":1}', "extra"],
    ["﻿{}", "byte order mark"],
    ["{", "invalid JSON"],
  ]) {
    fails(() => parse(WIKI, text ?? ""), why ?? "", text ?? "");
  }
});

test("rejects bad wiki entries", () => {
  for (const [text, why] of [
    [wiki(wikiEntry(',"colour":"red"')), "colour"],
    [wiki(`${wikiEntry()},${wikiEntry().replace('"a"', '"A"')}`), "duplicate"],
    [wiki(wikiEntry(',"target":"skill"').replace('"a"', '"a b"')), "slug"],
    [wiki(wikiEntry().replace(UUID, "nope")), "uuid"],
    [wiki(wikiEntry().replace('"skill"', '"../x"')), "type"],
    [wiki(wikiEntry().replace('{"en":"A"}', "{}")), "title"],
    [wiki(wikiEntry().replace('{"en":"A"}', '{"ja":"A"}')), "ja"],
    [wiki(wikiEntry(',"updated":"yesterday"')), "RFC 3339"],
    [wiki(wikiEntry(',"status":"gone"')), "gone"],
    [wiki(wikiEntry().replace('"a"', '"a\\u0007"')), "id"],
  ] as const) {
    fails(() => parse(WIKI, text), why, text);
  }
});

test("skills entry rules", () => {
  const cli = (extra = "") =>
    `{"id":"x","installer":"skills-cli","source":"owner/repo","title":{"en":"x"}${extra}}`;
  skills(cli()); // skill is optional
  skills(cli(',"skill":"*"')); // `*` installs every skill
  skills(cli(',"skill":"pdf"'));
  for (const [entry, why] of [
    [cli(',"skill":"-g"'), "`skill`"],
    [cli(',"skill":"a b"'), "`skill`"],
    [cli().replace("owner/repo", "--yes"), "`source`"],
    [cli().replace("owner/repo", "owner/repo x"), "`source`"],
    [cli(',"install":"npx x"'), "must not have `install`"],
    [cli(',"ref":"main"'), "ref"],
    [
      '{"id":"p","installer":"claude-code-cli","source":"o/r","title":{"en":"p"}}',
      "needs `install`",
    ],
    [
      '{"id":"p","installer":"claude-code-cli","source":"o/r","skill":"p","install":"x","title":{"en":"p"}}',
      "only `skills-cli`",
    ],
  ] as const) {
    fails(() => skills(entry), why, entry);
  }
});

test("docs entry rules", () => {
  const doc = (url: string, file: string, extra = "") =>
    `{"id":"d","url":"${url}","filename":"${file}","title":{"en":"d"}${extra}}`;
  const hash = "0e306d8fba8b7d173ab0e85ab2d40a93ec393cbc814b586025de5c6f6d5f9c1a";
  docs(doc("https://x.test/a.md", "a.md", `,"sha256":"${hash}"`));
  for (const [entry, why] of [
    [doc("http://x.test/a.md", "a.md"), "https"],
    [doc("https://", "a.md"), "https"],
    [doc("https://x.test/a.md", "dir/a.md"), "filename"],
    [doc("https://x.test/a.md", ".."), "filename"],
    [doc("https://x.test/a.md", "a.md", `,"sha256":"${hash.toUpperCase()}"`), "sha256"],
  ] as const) {
    fails(() => docs(entry), why, entry);
  }
});

test("RFC 3339 shapes", () => {
  for (const ok of [
    "2026-06-28T12:25:41Z",
    "2026-06-28T12:25:41.928Z",
    "2026-06-28t12:25:41+09:00",
  ]) {
    assert.ok(isRfc3339(ok), ok);
  }
  for (const bad of [
    "2026-06-28",
    "2026-06-28 12:25:41Z",
    "2026-06-28T12:25:41",
    "2026-06-28T12:25:41.Z",
    "2026-06-28T12:25:41+0900",
  ]) {
    assert.ok(!isRfc3339(bad), bad);
  }
});
