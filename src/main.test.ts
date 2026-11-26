// Binary-level checks that need no terminal.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const main = fileURLToPath(new URL("./main.js", import.meta.url));
const run = (args: string[], cwd?: string) =>
  spawnSync(process.execPath, [main, ...args], { cwd, encoding: "utf8", stdio: "pipe" });

test("--version prints the package version", () => {
  const out = run(["--version"]);
  assert.equal(out.status, 0);
  assert.match(out.stdout, /^agent-harness-tui \d+\.\d+\.\d+/);
});

test("fails fast without a terminal and writes nothing", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-smoke-"));
  try {
    const out = run([], dir);
    assert.equal(out.status, 2);
    assert.match(out.stderr, /interactive terminal/);
    assert.equal(fs.readdirSync(dir).length, 0, "nothing written");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("unknown argument is an error", () => {
  assert.equal(run(["--bogus"]).status, 2);
});
