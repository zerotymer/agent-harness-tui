import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import {
  applyHiddenAttribute,
  completedDir,
  completedLog,
  defaultLayout,
  type Layout,
  type PathError,
  type PathErrorKind,
  ProjectRoot,
  RelPath,
  validateLayout,
} from "./paths.js";

const windows = process.platform === "win32";
const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

/** Temporary target project, removed after the tests. */
function tempProject(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-paths-"));
  temps.push(dir);
  return dir;
}

function layout(references: string, artifacts: string): Layout {
  return {
    ...defaultLayout(),
    references: RelPath.parse(references),
    artifacts: RelPath.parse(artifacts),
  };
}

function throwsKind(fn: () => unknown, kind: PathErrorKind): void {
  assert.throws(fn, (e: PathError) => e.kind === kind);
}

function has(errors: PathError[], kind: PathErrorKind, ...paths: string[]): boolean {
  return errors.some((e) => e.kind === kind && paths.every((p, i) => e.paths[i] === p));
}

test("resolves relative and normalizes", () => {
  const root = new ProjectRoot(tempProject());
  assert.equal(root.resolve("references").toString(), "references");
  assert.equal(root.resolve("./a/./b/../c").toString(), "a/c");
  assert.equal(root.resolve("docs\\ref").toString(), "docs/ref");
  assert.equal(root.resolve("  .output  ").toString(), ".output");
});

test("supports spaces and Korean", () => {
  const dir = tempProject();
  const root = new ProjectRoot(dir);
  assert.equal(root.resolve("참조 문서/설계").toString(), "참조 문서/설계");
  fs.mkdirSync(path.join(dir, "참조 문서"));
  assert.equal(root.resolve("참조 문서").toString(), "참조 문서");
});

test("rejects empty and root", () => {
  const root = new ProjectRoot(tempProject());
  throwsKind(() => root.resolve("  "), "empty");
  throwsKind(() => root.resolve("."), "is-root");
  throwsKind(() => root.resolve("a/.."), "is-root");
});

test("rejects parent escape", () => {
  const root = new ProjectRoot(tempProject());
  throwsKind(() => root.resolve(".."), "outside-root");
  throwsKind(() => root.resolve("a/../../x"), "outside-root");
  throwsKind(() => root.resolve("..\\x"), "outside-root");
});

test("absolute inside root becomes relative", () => {
  const root = new ProjectRoot(tempProject());
  assert.equal(root.resolve(path.join(root.path, "refs", "a")).toString(), "refs/a");
  throwsKind(() => root.resolve(path.join(path.dirname(root.path), "other")), "outside-root");
});

test("absolute through symlinked alias becomes relative", { skip: windows }, () => {
  const dir = tempProject();
  const alias = `${dir}.alias`;
  fs.symlinkSync(dir, alias);
  temps.push(alias);
  const got = new ProjectRoot(dir).resolve(path.join(alias, "refs", "a"));
  assert.equal(got.toString(), "refs/a");
});

/** Plain `C:\..` input in another letter case, as users type it. */
test("plain drive path in other case becomes relative", { skip: !windows }, () => {
  const root = new ProjectRoot(tempProject());
  const plain = root.path;
  assert.equal(root.resolve(`${plain.toLowerCase()}\\refs\\a`).toString(), "refs/a");
  assert.equal(root.resolve(`${plain.replaceAll("\\", "/")}/refs/a`).toString(), "refs/a");
});

test("Windows drive path is outside on Unix", { skip: windows }, () => {
  const root = new ProjectRoot(tempProject());
  for (const input of ["C:\\project\\refs", "d:/data", "C:"]) {
    throwsKind(() => root.resolve(input), "outside-root");
  }
});

test("rejects symlink escape", { skip: windows }, () => {
  const dir = tempProject();
  const outside = tempProject();
  fs.symlinkSync(outside, path.join(dir, "link"));
  fs.symlinkSync(path.join(dir, "missing"), path.join(dir, "dangling"));
  fs.mkdirSync(path.join(dir, "real"));
  fs.symlinkSync(path.join(dir, "real"), path.join(dir, "inner-link"));
  const root = new ProjectRoot(dir);
  throwsKind(() => root.resolve("link"), "symlink-escape");
  throwsKind(() => root.resolve("link/sub"), "symlink-escape");
  throwsKind(() => root.resolve("dangling"), "symlink-escape");
  assert.equal(root.resolve("inner-link/x").toString(), "inner-link/x");
});

test("default layout is valid and derives fixed paths", () => {
  const root = new ProjectRoot(tempProject());
  const l = defaultLayout();
  assert.deepEqual(validateLayout(l, root), []);
  assert.equal(completedDir(l).toString(), "instructions/.completed");
  assert.equal(completedLog(l).toString(), "instructions/completed.log");
  assert.equal(root.abs(completedLog(l)), path.join(root.path, "instructions", "completed.log"));
});

test("any name is accepted and disabled paths are not checked", () => {
  const root = new ProjectRoot(tempProject());
  assert.deepEqual(validateLayout(layout("references", "build/out"), root), []);
  // A disabled path cannot collide or nest.
  const l: Layout = { instructions: RelPath.parse("instructions"), backlogEnabled: false };
  fs.writeFileSync(path.join(root.path, "references"), "a file");
  assert.deepEqual(validateLayout(l, root), []);
  assert.ok(has(validateLayout(layout("references", ".a"), root), "collision", "references"));
});

test("rejects duplicates case-insensitively", () => {
  const root = new ProjectRoot(tempProject());
  const errs = validateLayout(layout(".Skills", ".output"), root);
  assert.equal(errs[0]?.kind, "duplicate");
});

test("rejects nesting", () => {
  const root = new ProjectRoot(tempProject());
  let errs = validateLayout(layout("instructions/refs", ".output"), root);
  assert.ok(has(errs, "nested", "instructions", "instructions/refs"));
  errs = validateLayout(layout("docs", "docs/.out"), root);
  assert.ok(has(errs, "nested", "docs", "docs/.out"));
  // Sibling with a shared name prefix is not nesting.
  assert.deepEqual(validateLayout(layout("refs", "refs-x/.out"), root), []);
});

test("rejects collisions with existing files", () => {
  const dir = tempProject();
  fs.writeFileSync(path.join(dir, "references"), "file, not dir");
  fs.mkdirSync(path.join(dir, "instructions/agent.db"), { recursive: true });
  fs.writeFileSync(path.join(dir, "parent"), "");
  const root = new ProjectRoot(dir);
  const errs = validateLayout(defaultLayout(), root);
  assert.ok(has(errs, "collision", "references"));
  assert.ok(has(errs, "collision", "instructions/agent.db"));
  assert.ok(
    has(validateLayout(layout("parent/child", ".output"), root), "collision", "parent/child"),
  );
});

test("existing directories are preserved, not collisions", () => {
  const dir = tempProject();
  fs.mkdirSync(path.join(dir, "instructions/.completed"), { recursive: true });
  fs.mkdirSync(path.join(dir, "references"));
  assert.deepEqual(validateLayout(defaultLayout(), new ProjectRoot(dir)), []);
});

test("hidden attribute is not applicable off Windows", { skip: windows }, () => {
  assert.deepEqual(applyHiddenAttribute(".output"), { kind: "not-applicable" });
});
