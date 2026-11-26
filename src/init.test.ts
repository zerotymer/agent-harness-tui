import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { hasWrites } from "./apply.js";
import { defaultConfig } from "./contract.js";
import * as init from "./init.js";
import { ProjectRoot, RelPath } from "./paths.js";

function project() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-init-"));
  return {
    dir,
    root: new ProjectRoot(dir),
    [Symbol.dispose]: () => fs.rmSync(dir, { recursive: true, force: true }),
  };
}

test("invalid layout is reported before planning", async () => {
  using p = project();
  const c = defaultConfig();
  c.paths.output = RelPath.parse("out");
  await assert.rejects(init.prepare(p.root, init.load(p.root), c, []), /dot-prefixed/);
});

test("unsupported state version blocks loading", () => {
  using p = project();
  fs.mkdirSync(path.join(p.dir, ".harness"));
  fs.writeFileSync(path.join(p.dir, ".harness/state.json"), '{"schema_version": 7}');
  assert.throws(() => init.load(p.root), /7/);
});

test("init without skills, then verify is unchanged", async () => {
  using p = project();
  const c = defaultConfig();
  c.agents = [{ id: "claude-code", placement: "copy" }];
  const loaded = init.load(p.root);
  assert.equal(loaded.config, undefined);
  const [report] = init.applyPrepared(
    p.root,
    await init.prepare(p.root, loaded, c, []),
    loaded.state,
  );
  assert.equal(report.failed, undefined);
  const again = init.load(p.root);
  assert.ok(!hasWrites((await init.prepare(p.root, again, again.config!, [])).plan));
});
