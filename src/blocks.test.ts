import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { apply, plan, sha256 } from "./apply.js";
import { type BlockError, desired, upsert } from "./blocks.js";
import { defaultState } from "./contract.js";
import { ProjectRoot, RelPath } from "./paths.js";

const BEGIN = "<!-- agent-harness:begin";

test("creates new file", () => {
  const e = upsert(undefined, "markdown", "agent-harness", "line 1\nline 2");
  assert.equal(
    e.content,
    "<!-- agent-harness:begin agent-harness -->\nline 1\nline 2\n<!-- agent-harness:end agent-harness -->\n",
  );
  assert.equal(e.currentHash, undefined);
});

test("appends to user file and keeps it", () => {
  const e = upsert("# My notes\n\nkeep me", "markdown", "h", "managed");
  assert.ok(e.content.startsWith(`# My notes\n\nkeep me\n\n${BEGIN} h -->`));
  // Second run replaces in place; no duplicate block.
  const again = upsert(e.content, "markdown", "h", "managed");
  assert.equal(again.content, e.content);
  assert.equal(again.currentHash, e.newHash);
});

test("replaces body only", () => {
  const text = "top\n<!-- agent-harness:begin h -->\nold\n<!-- agent-harness:end h -->\nbottom\n";
  const e = upsert(text, "markdown", "h", "new 1\nnew 2\n");
  assert.equal(
    e.content,
    "top\n<!-- agent-harness:begin h -->\nnew 1\nnew 2\n<!-- agent-harness:end h -->\nbottom\n",
  );
  assert.equal(e.currentHash, sha256("old\n"));
  assert.equal(e.newHash, sha256("new 1\nnew 2\n"));
});

test("preserves CRLF and missing final newline", () => {
  const text = "top\r\n<!-- agent-harness:begin h -->\r\nold\r\n<!-- agent-harness:end h -->";
  const e = upsert(text, "markdown", "h", "new");
  assert.equal(
    e.content,
    "top\r\n<!-- agent-harness:begin h -->\r\nnew\r\n<!-- agent-harness:end h -->",
  );
  // CRLF bodies hash like LF bodies, so line endings alone are no edit.
  assert.equal(e.currentHash, sha256("old\n"));
  assert.equal(
    upsert("a\r\nb", "markdown", "h", "x").content,
    "a\r\nb\r\n\r\n<!-- agent-harness:begin h -->\r\nx\r\n<!-- agent-harness:end h -->\r\n",
  );
});

test("keeps other blocks and BOM", () => {
  const text = "﻿<!-- agent-harness:begin other -->\nx\n<!-- agent-harness:end other -->\n";
  assert.ok(upsert(text, "markdown", "h", "y").content.startsWith(text));
});

test("hash syntax for gitignore", () => {
  assert.equal(
    upsert("target/\n", "hash", "ignore", ".output/\n.harness/").content,
    "target/\n\n# agent-harness:begin ignore\n.output/\n.harness/\n# agent-harness:end ignore\n",
  );
});

test("malformed markers are errors", () => {
  const cases: [string, string][] = [
    ["<!-- agent-harness:begin h -->\nx\n", "never closed"],
    ["<!-- agent-harness:end h -->\n", "without a begin"],
    ["<!-- agent-harness:begin h -->\n<!-- agent-harness:begin i -->\n", "inside block"],
    ["<!-- agent-harness:begin h -->\n<!-- agent-harness:end i -->\n", "closed by end"],
    [
      "<!-- agent-harness:begin h -->\n<!-- agent-harness:end h -->\n" +
        "<!-- agent-harness:begin h -->\n<!-- agent-harness:end h -->\n",
      "duplicate",
    ],
    ["<!-- agent-harness:begin -->\n", "unrecognized"],
    ["# agent-harness:begin h\n", "unrecognized"], // wrong syntax for Markdown
  ];
  for (const [text, expect] of cases) {
    assert.throws(
      () => upsert(text, "markdown", "h", "body"),
      (e: BlockError) => e.kind === "malformed" && e.message.includes(expect),
      text,
    );
  }
});

test("rejects bad id and marker in body", () => {
  assert.throws(() => upsert(undefined, "markdown", "../x", "b"), { kind: "invalid-id" });
  assert.throws(() => upsert(undefined, "markdown", "h", "<!-- agent-harness:end h -->"), {
    kind: "body-has-marker",
  });
});

test("works with apply engine", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-blocks-"));
  try {
    const root = new ProjectRoot(dir);
    const file = path.join(dir, "CLAUDE.md");
    const read = () => fs.readFileSync(file, "utf8");
    fs.writeFileSync(file, "# Project\n\nuser rules\n");
    const rel = RelPath.parse("CLAUDE.md");
    const state = defaultState();
    const step = (body: string) =>
      plan(root, state, [], [desired(root, rel, "markdown", "agent-harness", body)]);

    // Insert into a user file.
    let pl = step("policy v1");
    assert.equal(pl.items[0]?.action.kind, "update");
    apply(root, pl, state);
    assert.ok(read().startsWith("# Project\n\nuser rules\n\n"));

    // User edits outside the block: the block is still ours, nothing to do.
    fs.writeFileSync(file, read().replace("user rules", "user rules, edited"));
    assert.equal(step("policy v1").items[0]?.action.kind, "unchanged");

    // New body: updated in place, user edit kept.
    pl = step("policy v2");
    assert.equal(pl.items[0]?.action.kind, "update");
    apply(root, pl, state);
    assert.ok(read().includes("user rules, edited") && read().includes("policy v2"));

    // User edits inside the block: conflict.
    fs.writeFileSync(file, read().replace("policy v2", "policy hacked"));
    assert.equal(step("policy v3").items[0]?.action.kind, "conflict");

    // Broken marker: conflict, never a full rewrite.
    const broken = "<!-- agent-harness:begin agent-harness -->\n";
    fs.writeFileSync(file, broken);
    pl = step("policy v3");
    const a = pl.items[0]?.action;
    assert.ok(a?.kind === "conflict" && a.reason.includes("never closed"));
    assert.throws(() => apply(root, pl, state), { kind: "conflicts" });
    assert.equal(read(), broken);

    // Not UTF-8: conflict, not a crash.
    fs.writeFileSync(file, Buffer.from([0xff, 0xfe, 0x00, 0xd8]));
    const bad = step("policy v3").items[0]?.action;
    assert.ok(bad?.kind === "conflict" && bad.reason.includes("UTF-8"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
