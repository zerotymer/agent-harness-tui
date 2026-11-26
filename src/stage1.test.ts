// Stage 1 acceptance checks (spec §8, instruction 0011), run against
// temporary target projects through the public API only. Local sources
// only: no network and no administrator rights are needed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { conflicts, hasWrites, preview } from "./apply.js";
import {
  defaultConfig,
  fromText,
  HARNESS_CONFIG,
  HARNESS_LOCK,
  HARNESS_STATE,
  type HarnessConfig,
  load as loadContract,
} from "./contract.js";
import * as init from "./init.js";
import { PathError, ProjectRoot, RelPath } from "./paths.js";
import { LocalDir, SourceError } from "./sources.js";

const unix = process.platform !== "win32";

class Project {
  readonly dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-stage1-"));
  constructor() {
    this.write("vendor/review/SKILL.md", "---\nname: review\ndescription: Review\n---\n# Review\n");
    this.write("vendor/review/scripts/run.sh", "#!/bin/sh\ntouch PWNED\n");
  }
  root = () => new ProjectRoot(this.dir);
  abs = (rel: string) => path.join(this.dir, rel);
  write(rel: string, text: string) {
    fs.mkdirSync(path.dirname(this.abs(rel)), { recursive: true });
    fs.writeFileSync(this.abs(rel), text);
  }
  read = (rel: string) => fs.readFileSync(this.abs(rel), "utf8");
  [Symbol.dispose]() {
    fs.rmSync(this.dir, { recursive: true, force: true });
  }
}

function config(): HarnessConfig {
  const c = defaultConfig();
  c.agents = [
    { id: "claude-code", placement: "copy" },
    { id: "codex", placement: "reference" },
  ];
  c.sources = [{ id: "local", kind: "local-dir", path: RelPath.parse("vendor") }];
  c.skills = [{ id: "review", source: "local", reference: "review" }];
  // Spaces and Korean in configurable paths.
  c.paths.references = RelPath.parse("docs/참조 문서");
  c.paths.artifacts = RelPath.parse(".산출물");
  delete c.paths.docs; // `docs/참조 문서` would nest inside it
  return c;
}

const imports = (p: Project): init.PendingImport[] => [
  {
    skillId: "review",
    sourceId: "local",
    reference: "review",
    acceptPartial: false,
    package: new LocalDir(p.abs("vendor")).fetch("review"),
  },
];

/** Prepares and applies; fails on conflicts or failure. */
async function initProject(p: Project, pending: init.PendingImport[]) {
  const root = p.root();
  const loaded = init.load(root);
  const prepared = await init.prepare(root, loaded, loaded.config ?? config(), pending);
  assert.equal(conflicts(prepared.plan).length, 0, preview(prepared.plan));
  const [report] = init.applyPrepared(root, prepared, loaded.state);
  assert.equal(report.failed, undefined);
  return loaded.state;
}

/** No-change verification of an initialized project. */
async function verifyUnchanged(p: Project) {
  const root = p.root();
  const loaded = init.load(root);
  assert.ok(loaded.config, "initialized");
  const prepared = await init.prepare(root, loaded, loaded.config, []);
  assert.ok(!hasWrites(prepared.plan), preview(prepared.plan));
  assert.equal(conflicts(prepared.plan).length, 0);
}

const block = (text: string) =>
  text.slice(text.indexOf("agent-harness:begin"), text.indexOf("agent-harness:end"));

const codeSpans = (text: string) => new Set(text.split("`").filter((_, i) => i % 2 === 1));

test("initializes with a local source and no network", async () => {
  using p = new Project();
  await initProject(p, imports(p));
  for (const f of [
    "harness.toml",
    "harness.lock.json",
    ".harness/state.json",
    ".gitignore",
    "AGENTS.md",
    "AGENTS_ko.md",
    "CLAUDE.md",
    "CLAUDE_ko.md",
    "instructions/completed.log",
    ".skills/review/SKILL.md",
    ".skills/review/scripts/run.sh",
    ".claude/skills/review/SKILL.md",
  ]) {
    assert.ok(fs.statSync(p.abs(f)).isFile(), f);
  }
  for (const d of ["instructions/.completed", "docs/참조 문서", ".산출물"]) {
    assert.ok(fs.statSync(p.abs(d)).isDirectory(), d);
  }
  // Bundled scripts are copied, never run; reference placement writes nothing.
  assert.ok(!fs.existsSync(p.abs("PWNED")) && !fs.existsSync(p.abs("vendor/review/PWNED")));
  assert.ok(!fs.existsSync(p.abs(".agents")));
  // No SQLite file or schema is created for the backlog.
  assert.ok(!fs.existsSync(p.abs("agent.db")));
});

test("re-run keeps config and path resolution", async () => {
  using p = new Project();
  await initProject(p, imports(p));
  const loaded = init.load(p.root());
  assert.deepEqual(loaded.config, config());
  assert.equal(
    p.root().abs(loaded.config!.paths.references!),
    path.join(p.root().path, "docs", "참조 문서"),
  );
  const toml = p.read("harness.toml");
  assert.ok(toml.includes('references = "docs/참조 문서"'), "portable relative path");
  assert.ok(!toml.includes(p.dir), "no absolute paths in shared config");
  await verifyUnchanged(p);
});

test("existing guidance and log survive and repeats add nothing", async () => {
  using p = new Project();
  p.write("CLAUDE.md", "# Team rules\n\nKeep tests green.\n");
  p.write("AGENTS.md", "# Agents\r\nUse CRLF.\r\n");
  p.write("instructions/completed.log", "u-1 | earlier work | 2026-01-01 09:00:00 +0900\n");
  p.write(".gitignore", "target/\n");
  await initProject(p, imports(p));
  for (let i = 0; i < 2; i++) await verifyUnchanged(p);
  assert.ok(p.read("CLAUDE.md").startsWith("# Team rules\n\nKeep tests green.\n\n"));
  assert.ok(p.read("AGENTS.md").startsWith("# Agents\r\nUse CRLF.\r\n\r\n"));
  assert.equal(
    p.read("instructions/completed.log"),
    "u-1 | earlier work | 2026-01-01 09:00:00 +0900\n",
  );
  assert.ok(p.read(".gitignore").startsWith("target/\n\n"));
  for (const f of ["CLAUDE.md", "CLAUDE_ko.md", "AGENTS.md", "AGENTS_ko.md", ".gitignore"]) {
    assert.equal(p.read(f).split("agent-harness:begin").length - 1, 1, f);
  }
});

test("English and Korean guidance state the same policy", async () => {
  using p = new Project();
  await initProject(p, imports(p));
  for (const [en, ko] of [
    ["CLAUDE.md", "CLAUDE_ko.md"],
    ["AGENTS.md", "AGENTS_ko.md"],
  ] as const) {
    const enSpans = codeSpans(block(p.read(en)));
    assert.deepEqual(enSpans, codeSpans(block(p.read(ko))));
    for (const must of [
      "instructions/NNNN_short-name.md",
      "instructions/.completed/<UUID>.md",
      "instructions/completed.log",
      "UUID | summary | datetime with offset",
      "TODO",
      "IN_PROGRESS",
      "DONE",
    ]) {
      assert.ok(enSpans.has(must), must);
    }
  }
});

test("rejects path escapes and collisions", async () => {
  using p = new Project();
  const root = p.root();
  assert.throws(
    () => root.resolve("../outside"),
    (e) => e instanceof PathError && e.kind === "outside-root",
  );
  assert.throws(() =>
    fromText(
      HARNESS_CONFIG,
      'schema_version = 1\n[paths]\nreferences = "/etc"\noutput = ".o"\n[backlog]\nenabled = false\npath = "agent.db"\n',
    ),
  );
  p.write("references", "a file where a directory belongs");
  await assert.rejects(init.prepare(root, init.load(root), defaultConfig(), []), /collides/);
});

test("rejects package paths that escape", { skip: !unix && "needs Unix symlinks" }, () => {
  using p = new Project();
  using outside = new Project();
  fs.symlinkSync(outside.abs("vendor/review/SKILL.md"), p.abs("vendor/review/leak.md"));
  assert.throws(
    () => new LocalDir(p.abs("vendor")).fetch("review"),
    (e) => e instanceof SourceError && e.kind === "path-escape" && e.message.includes("leak.md"),
  );
});

test("guidance edits inside the block are conflicts", async () => {
  using p = new Project();
  await initProject(p, imports(p));
  fs.writeFileSync(p.abs("CLAUDE.md"), p.read("CLAUDE.md").replace("### Git", "### Git (edited)"));
  const root = p.root();
  const loaded = init.load(root);
  const c = loaded.config!;
  const changed = { ...c, backlog: { ...c.backlog, enabled: true } };
  const prepared = await init.prepare(root, loaded, changed, []);
  assert.deepEqual(
    conflicts(prepared.plan).map((i) => i.desired.path.toString()),
    ["CLAUDE.md"],
  );
});

test("clone without local state does not overwrite", async () => {
  using p = new Project();
  await initProject(p, imports(p));
  // Simulate a fresh clone: tracked files present, ignored state missing.
  fs.rmSync(p.abs(".harness"), { recursive: true });
  fs.rmSync(p.abs(".claude"), { recursive: true });
  const text = p.read("AGENTS.md").replace("### Git", "### Git (local edit)");
  fs.writeFileSync(p.abs("AGENTS.md"), text);
  const root = p.root();
  const loaded = init.load(root);
  const prepared = await init.prepare(root, loaded, loaded.config!, []);
  const item = prepared.plan.items.find((i) => i.desired.path.toString() === "AGENTS.md");
  assert.ok(item?.action.kind === "conflict" && item.action.reason.includes("ownership"));
  assert.equal(p.read("AGENTS.md"), text, "planning wrote nothing");
});

test("interrupted apply restores files and records no success", {
  skip: !unix && "needs Unix permissions",
}, async (t) => {
  using p = new Project();
  p.write("CLAUDE.md", "# Original\n");
  fs.mkdirSync(p.abs(".claude/skills"), { recursive: true });
  const root = p.root();
  const loaded = init.load(root);
  const prepared = await init.prepare(root, loaded, config(), imports(p));
  // A real filesystem failure part-way through: the copy target turns
  // read-only after planning.
  const locked = p.abs(".claude/skills");
  fs.chmodSync(locked, 0o555);
  try {
    fs.writeFileSync(path.join(locked, "probe"), "");
    // Running with privileges that ignore permissions: cannot simulate.
    fs.chmodSync(locked, 0o755);
    t.skip("permissions are not enforced for this user");
    return;
  } catch {}
  let report: ReturnType<typeof init.applyPrepared>[0];
  try {
    [report] = init.applyPrepared(root, prepared, loaded.state);
  } finally {
    fs.chmodSync(locked, 0o755);
  }
  assert.ok(report.failed !== undefined);
  assert.deepEqual(report.residual, []);
  assert.ok(report.restored.length > 0);
  assert.equal(p.read("CLAUDE.md"), "# Original\n", "restored from backup");
  assert.ok(!fs.existsSync(p.abs("harness.toml")), "new files removed");
  assert.ok(!fs.existsSync(p.abs(".skills")), "created dirs removed");
  assert.equal(loadContract(HARNESS_STATE, p.dir), undefined, "no success recorded");
  assert.ok(!fs.existsSync(p.abs(".harness/apply.lock")));
});

function git(dir: string, ...args: string[]): [boolean, string] {
  const out = spawnSync("git", ["-C", dir, "-c", "core.quotePath=false", ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: "/dev/null" /* Git for Windows maps this; it cannot read \\.\nul */,
    },
  });
  return [out.status === 0, out.stdout ?? ""];
}

test("Git tracks shared files and ignores local state", async (t) => {
  using p = new Project();
  if (!git(p.dir, "init", "-q")[0]) {
    t.skip("Git is missing; the tool skips Git checks the same way");
    return;
  }
  await initProject(p, imports(p));
  // Empty directories are invisible to Git; give the output dir a file.
  p.write(".산출물/report.txt", "generated");
  const [, status] = git(p.dir, "status", "--porcelain", "--untracked-files=all");
  for (const tracked of [
    "harness.toml",
    "harness.lock.json",
    "CLAUDE.md",
    "AGENTS_ko.md",
    "instructions/completed.log",
    ".skills/review/scripts/run.sh",
    ".gitignore",
  ]) {
    assert.ok(status.includes(tracked), `${tracked} should be visible to Git:\n${status}`);
  }
  for (const ignored of [".harness/", ".산출물", ".claude/skills/review"]) {
    assert.ok(!status.includes(ignored), `${ignored} should be ignored:\n${status}`);
  }
  // The tool never staged or committed anything.
  assert.equal(git(p.dir, "diff", "--cached", "--name-only")[1], "");
  assert.ok(!git(p.dir, "rev-parse", "HEAD")[0], "no commits");
});

test("state keeps what Stage 2 needs", async () => {
  using p = new Project();
  const state = await initProject(p, imports(p));
  assert.ok(state.lastSuccess);
  assert.ok(state.managed.some((m) => m.block === "agent-harness"));
  assert.ok(state.managed.some((m) => m.block === undefined));
  assert.ok(state.deployments.every((d) => d.hash !== undefined));
  assert.equal(state.adapters.length, 2);
  const lock = loadContract(HARNESS_LOCK, p.dir)!;
  assert.ok(lock.skills[0]!.files.every((f) => f.hash.startsWith("sha256:")));
});
