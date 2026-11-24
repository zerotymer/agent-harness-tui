import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { apply, plan } from "./apply.js";
import {
  defaultConfig,
  defaultState,
  HARNESS_CONFIG,
  type HarnessConfig,
  type HarnessState,
} from "./contract.js";
import { guidanceFiles, planItems, render } from "./guidance.js";
import { ProjectRoot, RelPath } from "./paths.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function project() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-guidance-"));
  temps.push(dir);
  return {
    dir,
    root: () => new ProjectRoot(dir),
    read: (r: string) => fs.readFileSync(path.join(dir, r), "utf8"),
  };
}
type Project = ReturnType<typeof project>;

function config(): HarnessConfig {
  const c = defaultConfig();
  c.paths.references = RelPath.parse("docs/참조 문서");
  c.paths.output = RelPath.parse(".out");
  c.agents = [
    { id: "claude-code", placement: "reference" },
    { id: "codex", placement: "reference" },
  ];
  c.sources = [{ id: "local", kind: "local-dir", path: RelPath.parse("vendor") }];
  c.skills = [{ id: "review", source: "local", reference: "review" }];
  assert.deepEqual(HARNESS_CONFIG.problems(c), []);
  return c;
}

const codeSpans = (text: string) =>
  [...new Set(text.split("`").filter((_, i) => i % 2 === 1))].sort();

async function run(p: Project, state: HarnessState, c: HarnessConfig) {
  const root = p.root();
  const [dirs, desired] = await planItems(root, c);
  return apply(root, plan(root, state, dirs, desired), state);
}

test("English and Korean share every path and value", () => {
  for (const enabled of [false, true]) {
    const c = config();
    c.backlog.enabled = enabled;
    const en = render(c, "claude-code", "en");
    const ko = render(c, "claude-code", "ko");
    assert.deepEqual(codeSpans(en), codeSpans(ko));
    assert.equal(en.split("\n").length, ko.split("\n").length, "same structure");
    for (const must of [
      "docs/참조 문서/",
      ".out/",
      "instructions/.completed/<UUID>.md",
      "instructions/agent.db-wal",
    ]) {
      assert.ok(codeSpans(en).includes(must), must);
    }
    assert.ok(!en.includes("agent-harness:"), "no marker text in the body");
  }
});

test("placement line matches agent and language", () => {
  const c = config();
  c.agents[1] = { id: "codex", placement: "copy" }; // codex copies, claude references
  const codexEn = render(c, "codex", "en");
  assert.ok(codexEn.includes("`.agents/skills/<id>/`"));
  assert.deepEqual(codeSpans(codexEn), codeSpans(render(c, "codex", "ko")));
  const claude = render(c, "claude-code", "en");
  assert.ok(claude.includes("referenced, not copied"));
  assert.ok(!claude.includes(".claude/skills"));
});

test("skills are listed, not inlined", () => {
  assert.ok(render(config(), "claude-code", "en").includes("| review | `.skills/review/` |"));
  const c = config();
  c.skills = [];
  assert.ok(render(c, "claude-code", "ko").includes("아직 선택한 스킬이 없다"));
});

test("agents get plural AGENTS.md and CLAUDE.md", () => {
  const names = (["codex", "claude-code"] as const).flatMap((a) =>
    guidanceFiles(a).map(([, p]) => `${p}`),
  );
  assert.deepEqual(names, ["AGENTS.md", "AGENTS_ko.md", "CLAUDE.md", "CLAUDE_ko.md"]);
});

test("initializes project and is idempotent", async () => {
  const p = project();
  const state = defaultState();
  assert.equal((await run(p, state, config())).failed, undefined);
  assert.ok(fs.statSync(path.join(p.dir, "instructions/.completed")).isDirectory());
  assert.equal(p.read("instructions/completed.log"), "");
  for (const f of ["AGENTS.md", "AGENTS_ko.md", "CLAUDE.md", "CLAUDE_ko.md"]) {
    assert.ok(p.read(f).includes("<!-- agent-harness:begin agent-harness -->"), f);
  }
  assert.equal((await run(p, state, config())).applyId, undefined, "repeat run changes nothing");
  assert.equal(p.read("CLAUDE.md").split("agent-harness:begin").length - 1, 1);
});

test("preserves existing guidance and log", async () => {
  const p = project();
  fs.mkdirSync(path.join(p.dir, "instructions"));
  const old = "u1 | old | 2026-01-01 00:00:00 +0900\n";
  fs.writeFileSync(path.join(p.dir, "instructions/completed.log"), old);
  fs.writeFileSync(path.join(p.dir, "CLAUDE.md"), "# Team rules\n\nBe kind.\n");
  const c = config();
  c.agents = c.agents.slice(0, 1);
  const report = await run(p, defaultState(), c);
  assert.deepEqual(report.preserved, ["instructions/completed.log"]);
  assert.equal(p.read("instructions/completed.log"), old);
  assert.ok(p.read("CLAUDE.md").startsWith("# Team rules\n\nBe kind.\n\n<!-- agent-harness:begin"));
  assert.ok(!fs.existsSync(path.join(p.dir, "AGENTS.md")), "only selected agents");
});

test("config change updates block only", async () => {
  const p = project();
  const state = defaultState();
  await run(p, state, config());
  const c = config();
  c.backlog.enabled = true;
  const root = p.root();
  const [dirs, desired] = await planItems(root, c);
  const updated = plan(root, state, dirs, desired)
    .items.filter((i) => i.action.kind === "update")
    .map((i) => `${i.desired.path}`);
  assert.deepEqual(updated, ["CLAUDE.md", "CLAUDE_ko.md", "AGENTS.md", "AGENTS_ko.md"]);
});
