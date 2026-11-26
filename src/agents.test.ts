import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { detect, detectAll, installRequest } from "./agents.js";
import { currentOs, parse } from "./catalog.js";

const win = process.platform === "win32";

/** A stub executable `name` printing `out` and exiting with `code`. */
function stub(dir: string, name: string, body: { out?: string; code?: number; sleep?: number }) {
  if (win) {
    const lines = ["@echo off"];
    if (body.out) lines.push(`echo ${body.out}`);
    lines.push(`exit /b ${body.code ?? 0}`);
    fs.writeFileSync(path.join(dir, `${name}.cmd`), lines.join("\r\n"));
    return;
  }
  const lines = ["#!/bin/sh"];
  if (body.sleep) lines.push(`exec sleep ${body.sleep}`);
  if (body.out) lines.push(`echo '${body.out}'`);
  lines.push(`exit ${body.code ?? 0}`);
  fs.writeFileSync(path.join(dir, name), `${lines.join("\n")}\n`, { mode: 0o755 });
}

function bin() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-agents-"));
  return { dir, [Symbol.dispose]: () => fs.rmSync(dir, { recursive: true, force: true }) };
}

test("detects installed and missing agents with stub executables", async () => {
  using b = bin();
  stub(b.dir, "good", { out: "good 1.2.3" });
  stub(b.dir, "bad", { code: 3 });
  assert.deepEqual(await detect(["good", "--version"], 5000, b.dir), {
    kind: "installed",
    version: "good 1.2.3",
  });
  assert.deepEqual(await detect(["bad"], 5000, b.dir), { kind: "missing", reason: "exit code 3" });
  const none = await detect(["absent"], 5000, b.dir);
  assert.equal(none.kind, "missing");
});

test("detection is killed at the timeout", { skip: win && "stub uses sleep" }, async () => {
  using b = bin();
  stub(b.dir, "slow", { sleep: 30 });
  const started = Date.now();
  const r = await detect(["slow"], 200, b.dir);
  assert.equal(r.kind, "missing");
  assert.match(r.kind === "missing" ? r.reason : "", /timed out/);
  assert.ok(Date.now() - started < 5000);
});

test("detectAll keeps list order", async () => {
  using b = bin();
  stub(b.dir, "one", { out: "1" });
  const c = parse(
    `{"schema_version":2,"agents":[${["absent", "one"]
      .map(
        (n) =>
          `{"id":"${n}","title":{"en":"${n}"},"detect":["${n}"],"guidance":"X.md","skills_dir":".x"}`,
      )
      .join(",")}]}`,
  );
  const r = await detectAll(c.agents, 5000, b.dir);
  assert.deepEqual(
    r.map((d) => d.kind),
    ["missing", "installed"],
  );
});

test("install request per OS, looked up on PATH", () => {
  using b = bin();
  fs.writeFileSync(path.join(b.dir, "npm"), "");
  const here = currentOs();
  if (!here) return; // not a supported platform
  const other = here === "linux" ? "darwin" : "linux";
  const [agent] = parse(
    `{"schema_version":2,"agents":[{"id":"a","title":{"en":"A"},"detect":["a"],"guidance":"A.md","skills_dir":".a","install":{"${here}":["npm","install","-g","a"]}}]}`,
  ).agents;
  assert.deepEqual(installRequest(agent!, b.dir), {
    program: path.join(b.dir, "npm"),
    args: ["install", "-g", "a"],
  });
  assert.deepEqual(installRequest(agent!, b.dir, other), {
    unavailable: "no install command for this OS",
  });
  assert.match((installRequest(agent!, "") as { unavailable: string }).unavailable, /not found/);
});
