import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import {
  APPLY_LOCK,
  type ApplyError,
  apply,
  classify,
  createOnly,
  type Desired,
  fileItem,
  formatUtc,
  plan,
  preview,
  sha256,
} from "./apply.js";
import { defaultState, HARNESS_STATE, type HarnessState, load, STATE_FILE } from "./contract.js";
import { ProjectRoot, RelPath } from "./paths.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function project() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-apply-"));
  temps.push(dir);
  return {
    dir,
    root: () => new ProjectRoot(dir),
    read: (rel: string) => fs.readFileSync(path.join(dir, rel), "utf8"),
    write: (rel: string, text: string) => {
      fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
      fs.writeFileSync(path.join(dir, rel), text);
    },
    exists: (rel: string) => fs.existsSync(path.join(dir, rel)),
  };
}
type Project = ReturnType<typeof project>;

const rel = (s: string) => RelPath.parse(s);

const desired = (): Desired[] => [
  fileItem(rel("harness.toml"), "schema_version = 1\n"),
  fileItem(rel(".skills/a/SKILL.md"), "# A\n"),
  createOnly(rel("instructions/completed.log"), ""),
];

const dirs = () => [rel("instructions/.completed")];

function run(p: Project, state: HarnessState, d: Desired[]) {
  const root = p.root();
  return apply(root, plan(root, state, dirs(), d), state);
}

const errorKind = (fn: () => unknown) => {
  try {
    fn();
  } catch (e) {
    return (e as ApplyError).kind;
  }
  return undefined;
};

test("fresh apply creates and records success last", () => {
  const p = project();
  const state = defaultState();
  const report = run(p, state, desired());
  assert.equal(report.failed, undefined);
  assert.equal(report.created.length, 3);
  assert.equal(p.read(".skills/a/SKILL.md"), "# A\n");
  assert.ok(fs.statSync(path.join(p.dir, "instructions/.completed")).isDirectory());
  // create-only files are never owned
  assert.equal(state.managed.length, 2);
  const saved = load(HARNESS_STATE, p.dir);
  assert.deepEqual(saved, state);
  assert.equal(saved?.lastSuccess?.id, report.applyId);
  assert.ok(!p.exists(APPLY_LOCK));
});

test("repeat apply is a no-op", () => {
  const p = project();
  const state = defaultState();
  run(p, state, desired());
  const before = p.read(STATE_FILE);
  const report = run(p, state, desired());
  assert.equal(report.applyId, undefined);
  assert.equal(report.unchanged.length, 2);
  assert.deepEqual(report.preserved, ["instructions/completed.log"]);
  assert.equal(p.read(STATE_FILE), before);
});

test("owned file is updated with backup", () => {
  const p = project();
  const state = defaultState();
  run(p, state, desired());
  const d = desired();
  d[1] = fileItem(rel(".skills/a/SKILL.md"), "# A v2\n");
  const report = run(p, state, d);
  assert.deepEqual(report.updated, [".skills/a/SKILL.md"]);
  assert.equal(p.read(".skills/a/SKILL.md"), "# A v2\n");
  assert.equal(p.read(`.harness/backups/${report.applyId}/.skills/a/SKILL.md`), "# A\n");
});

test("user edits and unowned files are conflicts", () => {
  const p = project();
  const state = defaultState();
  run(p, state, desired());
  p.write(".skills/a/SKILL.md", "# A edited by user\n");
  const d = desired();
  d[1] = fileItem(rel(".skills/a/SKILL.md"), "# A v2\n");
  const root = p.root();
  const pl = plan(root, state, dirs(), d);
  const a = pl.items[1]?.action;
  assert.ok(a?.kind === "conflict" && a.reason.includes("modified"));
  assert.equal(
    errorKind(() => apply(root, pl, state)),
    "conflicts",
  );
  assert.equal(p.read(".skills/a/SKILL.md"), "# A edited by user\n");
});

test("clone without local state does not overwrite", () => {
  const p = project();
  p.write("harness.toml", "schema_version = 1\n"); // identical: adopted
  p.write(".skills/a/SKILL.md", "# someone else's A\n"); // different: conflict
  const pl = plan(p.root(), defaultState(), dirs(), desired());
  assert.equal(pl.items[0]?.action.kind, "unchanged");
  const a = pl.items[1]?.action;
  assert.ok(a?.kind === "conflict" && a.reason.includes("ownership"));
});

test("injected failure restores files and records nothing", () => {
  const p = project();
  const state = defaultState();
  run(p, state, desired());
  const stateBefore = p.read(STATE_FILE);
  const d = desired();
  d[0] = fileItem(rel("harness.toml"), "schema_version = 1\n# v2\n");
  d[1] = fileItem(rel(".skills/a/SKILL.md"), "# A v2\n");
  d.push(fileItem(rel("new/dir/file.md"), "new\n"));
  const root = p.root();
  const pl = plan(root, state, dirs(), d);
  const snapshot = JSON.stringify(state);
  for (let failAt = 0; failAt < 4; failAt++) {
    const report = apply(root, pl, state, failAt);
    assert.ok(report.failed, `failAt ${failAt}`);
    assert.deepEqual(report.residual, []);
    assert.equal(JSON.stringify(state), snapshot, "in-memory state untouched");
    assert.equal(p.read("harness.toml"), "schema_version = 1\n");
    assert.equal(p.read(".skills/a/SKILL.md"), "# A\n");
    assert.ok(!p.exists("new"), "created dirs removed");
    assert.equal(p.read(STATE_FILE), stateBefore, "success not recorded");
    assert.ok(!p.exists(APPLY_LOCK));
  }
});

test("block ownership rules", () => {
  const state = defaultState();
  const block = (current?: string): Desired => ({
    path: rel("CLAUDE.md"),
    content: Buffer.from("user text\n<block v2>\n"),
    ownership: {
      kind: "block",
      id: "agent-harness",
      newHash: sha256("v2"),
      currentHash: current === undefined ? undefined : sha256(current),
    },
  });
  const userFile = Buffer.from("user text\n");
  // New block into a user file: allowed.
  assert.equal(classify(state, block(), userFile).kind, "update");
  // Block present but no baseline (fresh clone): cannot prove ownership.
  assert.equal(classify(state, block("v1"), userFile).kind, "conflict");
  // Block identical while the rest differs: nothing to write.
  assert.equal(classify(state, block("v2"), userFile).kind, "unchanged");

  const owned = defaultState();
  owned.managed.push({ path: rel("CLAUDE.md"), block: "agent-harness", hash: sha256("v1") });
  assert.equal(classify(owned, block("v1"), userFile).kind, "update");
  assert.equal(classify(owned, block("edited"), userFile).kind, "conflict");
  const removed = classify(owned, block(), userFile);
  assert.ok(removed.kind === "conflict" && removed.reason.includes("removed"));
});

test("stale lock blocks apply", () => {
  const p = project();
  p.write(APPLY_LOCK, "");
  const root = p.root();
  const state = defaultState();
  const pl = plan(root, state, dirs(), desired());
  assert.equal(
    errorKind(() => apply(root, pl, state)),
    "locked",
  );
  assert.ok(!p.exists("harness.toml"));
  assert.ok(p.exists(APPLY_LOCK), "a lock we did not take is left alone");
});

test("symlink escape is a conflict and permissions survive", {
  skip: process.platform === "win32",
}, () => {
  const p = project();
  const outside = project();
  fs.symlinkSync(outside.dir, path.join(p.dir, ".skills"));
  const pl = plan(p.root(), defaultState(), [], desired());
  assert.equal(pl.items[1]?.action.kind, "conflict");

  const q = project();
  const state = defaultState();
  run(q, state, [fileItem(rel("run.sh"), "v1")]);
  fs.chmodSync(path.join(q.dir, "run.sh"), 0o755);
  run(q, state, [fileItem(rel("run.sh"), "v2")]);
  assert.equal(fs.statSync(path.join(q.dir, "run.sh")).mode & 0o777, 0o755);
});

test("a directory at a file path is a conflict", () => {
  const p = project();
  fs.mkdirSync(path.join(p.dir, "harness.toml"));
  const a = plan(p.root(), defaultState(), [], desired()).items[0]?.action;
  assert.ok(a?.kind === "conflict" && a.reason.includes("directory"));
});

test("preview lists every item", () => {
  const p = project();
  const text = preview(plan(p.root(), defaultState(), dirs(), desired()));
  assert.ok(text.includes("mkdir     instructions/.completed/"));
  assert.ok(text.includes("create    .skills/a/SKILL.md"));
});

test("utc formatting", () => {
  assert.deepEqual(formatUtc(0), ["19700101-000000", "1970-01-01T00:00:00Z"]);
  assert.equal(formatUtc(1_794_805_200_000)[1], "2026-11-16T05:00:00Z");
  assert.equal(formatUtc(1_709_210_096_000)[0], "20240229-123456");
  assert.equal(sha256("").length, 71);
});
