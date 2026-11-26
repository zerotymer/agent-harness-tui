import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { hasWrites, sha256 } from "./apply.js";
import { defaultConfig, HARNESS_STATE, STATE_FILE, toText } from "./contract.js";
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
  c.paths.artifacts = RelPath.parse("instructions/tmp");
  await assert.rejects(init.prepare(p.root, init.load(p.root), c, []), /nested/);
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

async function applyAll(root: ProjectRoot, c = defaultConfig()) {
  if (c.agents.length === 0) c.agents = [{ id: "claude-code", placement: "copy" }];
  const loaded = init.load(root);
  const [report] = init.applyPrepared(root, await init.prepare(root, loaded, c, []), loaded.state);
  assert.equal(report.failed, undefined);
  return report;
}

test("schema 1 harness.toml is read with output as artifacts and rewritten as 2", async () => {
  using p = project();
  await applyAll(p.root);
  const v1 = [
    "schema_version = 1",
    "[paths]",
    'references = "references"',
    'output = ".output"',
    "[backlog]",
    "enabled = false",
    'path = "instructions/agent.db"',
    "[[agents]]",
    'id = "claude-code"',
    'placement = "copy"',
    "",
  ].join("\n");
  fs.writeFileSync(path.join(p.dir, "harness.toml"), v1);
  // As if an older version wrote it: the recorded baseline is the v1 text.
  const state = init.load(p.root).state;
  for (const m of state.managed) if (m.path.toString() === "harness.toml") m.hash = sha256(v1);
  fs.writeFileSync(path.join(p.dir, STATE_FILE), toText(HARNESS_STATE, state));
  const loaded = init.load(p.root);
  assert.equal(loaded.config?.paths.artifacts?.toString(), ".output");
  assert.equal(loaded.config?.paths.instructions.toString(), "instructions");
  assert.equal(loaded.config?.paths.docs, undefined, "v1 had no docs path");
  const report = await applyAll(p.root, loaded.config);
  const toml = fs.readFileSync(path.join(p.dir, "harness.toml"), "utf8");
  assert.match(toml, /^schema_version = 2/m);
  assert.match(toml, /artifacts = "\.output"/);
  assert.ok(!toml.includes("output ="));
  const backup = path.join(p.dir, ".harness/backups", report.applyId!, "harness.toml");
  assert.equal(fs.readFileSync(backup, "utf8"), v1, "backed up before the rewrite");
  assert.ok(fs.statSync(path.join(p.dir, ".output")).isDirectory());
  assert.match(fs.readFileSync(path.join(p.dir, ".gitignore"), "utf8"), /^\/\.output\/$/m);
});

test(".gitkeep only in new tracked directories, never overwritten", async () => {
  using p = project();
  fs.mkdirSync(path.join(p.dir, "references"));
  fs.mkdirSync(path.join(p.dir, "docs"));
  fs.writeFileSync(path.join(p.dir, "docs/.gitkeep"), "mine");
  await applyAll(p.root);
  const has = (rel: string) => fs.existsSync(path.join(p.dir, rel));
  for (const d of ["instructions", "instructions/.completed"]) assert.ok(has(`${d}/.gitkeep`), d);
  assert.ok(!has("references/.gitkeep"), "existing directory untouched");
  assert.ok(!has(".artifacts/.gitkeep"), "ignored directory gets none");
  assert.equal(fs.readFileSync(path.join(p.dir, "docs/.gitkeep"), "utf8"), "mine");
  const state = init.load(p.root).state;
  assert.ok(!state.managed.some((m) => m.path.toString().endsWith(".gitkeep")), "not owned");
});

test("changing a path creates the new directory and keeps the old one", async () => {
  using p = project();
  await applyAll(p.root);
  fs.writeFileSync(path.join(p.dir, "references/a.md"), "x");
  const loaded = init.load(p.root);
  const c = {
    ...loaded.config!,
    paths: { ...loaded.config!.paths, references: RelPath.parse("refs") },
  };
  await applyAll(p.root, c);
  assert.ok(fs.existsSync(path.join(p.dir, "references/a.md")), "never moved or deleted");
  assert.ok(fs.existsSync(path.join(p.dir, "refs/.gitkeep")));
});
