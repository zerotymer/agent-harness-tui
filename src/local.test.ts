import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { inspect } from "node:util";
import { defaultConfig } from "./contract.js";
import { ignoreBody } from "./gitignore.js";
import { LOCAL_FILE, loadToken, Secret, saveToken } from "./local.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function tempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-local-"));
  temps.push(dir);
  return dir;
}

const TOKEN = "tok-5c1e7a";

test("round trip in ignored harness dir", () => {
  const d = tempDir();
  assert.equal(loadToken(d), undefined);
  saveToken(d, Secret.from(TOKEN)!);
  assert.equal(loadToken(d)?.expose(), TOKEN);
  // `.harness/` is in the managed `.gitignore` block, so this file is never tracked.
  assert.ok(LOCAL_FILE.startsWith(".harness/"));
  assert.ok(ignoreBody(defaultConfig(), []).split("\n").includes("/.harness/"));
  if (process.platform !== "win32") {
    assert.equal(fs.statSync(path.join(d, LOCAL_FILE)).mode & 0o777, 0o600);
  }
});

test("errors and output never show the token", () => {
  const d = tempDir();
  fs.mkdirSync(path.join(d, ".harness"));
  for (const bad of [
    `schema_version = 1\n[gateway]\ntoken = "${TOKEN}"\nextra = 1\n`,
    `schema_version = 1\ntoken = "${TOKEN}"\n`,
    `schema_version = 1\n[gateway]\ntoken = ${TOKEN}\n`,
  ]) {
    fs.writeFileSync(path.join(d, LOCAL_FILE), bad);
    assert.throws(
      () => loadToken(d),
      (e: Error) => !e.message.includes(TOKEN),
    );
  }
  fs.writeFileSync(path.join(d, LOCAL_FILE), "schema_version = 2\n");
  assert.throws(() => loadToken(d), /schema_version/);
  const s = Secret.from(TOKEN)!;
  for (const shown of [inspect(s), `${s}`, JSON.stringify({ s })]) {
    assert.ok(!shown.includes(TOKEN), shown);
  }
  assert.equal(Secret.from("  "), undefined);
});
