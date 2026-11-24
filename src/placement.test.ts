import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { apply, type Desired, fileItem, plan, sha256 } from "./apply.js";
import {
  type AgentId,
  defaultConfig,
  defaultState,
  HARNESS_STATE,
  type HarnessConfig,
  type HarnessLock,
  type HarnessState,
  load,
  type Placement,
} from "./contract.js";
import { ProjectRoot, RelPath } from "./paths.js";
import { ignorePaths, methods, planPlacement } from "./placement.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

const SKILL = "---\nname: review\ndescription: Review code\n---\n# Review\n";
const rel = (s: string) => RelPath.parse(s);

function project(withSkill = true) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-placement-"));
  temps.push(dir);
  const write = (r: string, text: string) => {
    fs.mkdirSync(path.dirname(path.join(dir, r)), { recursive: true });
    fs.writeFileSync(path.join(dir, r), text);
  };
  if (withSkill) {
    write(".skills/review/SKILL.md", SKILL);
    write(".skills/review/scripts/check.sh", "echo hi\n");
  }
  return {
    dir,
    write,
    root: () => new ProjectRoot(dir),
    read: (r: string) => fs.readFileSync(path.join(dir, r), "utf8"),
  };
}
type Project = ReturnType<typeof project>;

const lock = (): HarnessLock => ({
  schemaVersion: 1,
  skills: [
    {
      id: "review",
      source: "local",
      reference: "review",
      files: [
        { path: rel("SKILL.md"), hash: sha256(SKILL) },
        { path: rel("scripts/check.sh"), hash: sha256("echo hi\n") },
      ],
    },
  ],
});

function config(...agents: [AgentId, Placement][]): HarnessConfig {
  return { ...defaultConfig(), agents: agents.map(([id, placement]) => ({ id, placement })) };
}

function run(p: Project, c: HarnessConfig, state: HarnessState) {
  const root = p.root();
  const pp = planPlacement(root, c, lock(), state, []);
  const pl = plan(root, state, [], pp.desired);
  pl.deployments = pp.deployments;
  pl.adapters = pp.adapters;
  return apply(root, pl, state);
}

test("copies into each agent's discovery dir and records mapping", () => {
  const p = project();
  const c = config(["claude-code", "copy"], ["codex", "copy"]);
  const state = defaultState();
  assert.equal(run(p, c, state).failed, undefined);
  assert.equal(p.read(".claude/skills/review/SKILL.md"), SKILL);
  assert.equal(p.read(".agents/skills/review/scripts/check.sh"), "echo hi\n");
  assert.equal(state.deployments.length, 4);
  const d = state.deployments.find((d) => `${d.target}` === ".claude/skills/review/SKILL.md");
  assert.equal(d?.hash, sha256(SKILL));
  assert.equal(state.adapters.length, 2);
  // Persisted, and a second run changes nothing.
  assert.deepEqual(load(HARNESS_STATE, p.dir), state);
  assert.equal(run(p, c, state).applyId, undefined);
});

test("reference placement writes no files", () => {
  const p = project();
  const c = config(["claude-code", "reference"]);
  const pp = planPlacement(p.root(), c, lock(), defaultState(), []);
  assert.deepEqual([pp.desired, pp.deployments], [[], []]);
  assert.equal(pp.adapters.length, 1);
  assert.deepEqual(ignorePaths(c, lock()), []);
});

test("unrelated existing skills are not overwritten", () => {
  const p = project();
  p.write(".claude/skills/review/SKILL.md", "user's own review skill\n");
  p.write(".claude/skills/other/SKILL.md", "unrelated\n");
  const root = p.root();
  const state = defaultState();
  const pp = planPlacement(root, config(["claude-code", "copy"]), lock(), state, []);
  const a = plan(root, state, [], pp.desired).items[0]?.action;
  assert.ok(a?.kind === "conflict" && a.reason.includes("not deployed"));
  assert.equal(p.read(".claude/skills/review/SKILL.md"), "user's own review skill\n");
  assert.equal(p.read(".claude/skills/other/SKILL.md"), "unrelated\n");
});

test("edited copy is a conflict", () => {
  const p = project();
  const c = config(["codex", "copy"]);
  const state = defaultState();
  run(p, c, state);
  p.write(".agents/skills/review/SKILL.md", "edited copy\n");
  p.write(".skills/review/SKILL.md", "---\nname: review\n---\nv2\n");
  const root = p.root();
  const pp = planPlacement(root, c, lock(), state, []);
  assert.equal(plan(root, state, [], pp.desired).items[0]?.action.kind, "conflict");
});

test("uses pending import bytes and warns without frontmatter", () => {
  const p = project(false);
  const pending: Desired[] = [
    fileItem(rel(".skills/review/SKILL.md"), "# no frontmatter\n"),
    fileItem(rel(".skills/review/scripts/check.sh"), "echo hi\n"),
  ];
  const pp = planPlacement(
    p.root(),
    config(["claude-code", "copy"]),
    lock(),
    defaultState(),
    pending,
  );
  assert.equal(pp.desired[0]?.content.toString(), "# no frontmatter\n");
  assert.equal(pp.warnings.length, 1);
  assert.ok(pp.warnings[0]?.includes("frontmatter"));
});

test("missing source is a conflict, not an error", () => {
  const p = project(false);
  const pp = planPlacement(p.root(), config(["claude-code", "copy"]), lock(), defaultState(), []);
  assert.ok(pp.desired.length > 0);
  assert.ok(
    pp.desired.every(
      (d) => d.ownership.kind === "unresolvable" && d.ownership.reason.includes("missing"),
    ),
  );
});

test("symlink is unavailable with reason and ignores are per skill", () => {
  const m = methods("claude-code");
  assert.ok(m.some(([k, why]) => k === "symlink" && why));
  assert.equal(m.filter(([, why]) => !why).length, 2);
  const c = config(["claude-code", "copy"], ["codex", "reference"]);
  assert.deepEqual(ignorePaths(c, lock()).map(String), [".claude/skills/review"]);
});
