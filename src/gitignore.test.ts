import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { apply, plan } from "./apply.js";
import { defaultConfig, defaultState, type HarnessConfig } from "./contract.js";
import { desired, gitReport, ignoreBody } from "./gitignore.js";
import { ProjectRoot, RelPath } from "./paths.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function tempDir() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-gitignore-"));
  temps.push(dir);
  return {
    dir,
    write: (r: string, text: string) => {
      fs.mkdirSync(path.dirname(path.join(dir, r)), { recursive: true });
      fs.writeFileSync(path.join(dir, r), text);
    },
    git: (...args: string[]) => {
      const out = spawnSync("git", ["-C", dir, ...args], {
        encoding: "utf8",
        env: {
          ...process.env,
          GIT_CONFIG_GLOBAL: "/dev/null" /* Git for Windows maps this; it cannot read \\.\nul */,
        },
      });
      assert.equal(out.status, 0, `git ${args.join(" ")}: ${out.stderr}`);
      return out.stdout;
    },
    root: () => new ProjectRoot(dir),
  };
}

const hasGit = spawnSync("git", ["--version"]).status === 0;

function config(): HarnessConfig {
  return { ...defaultConfig(), agents: [{ id: "claude-code", placement: "copy" }] };
}

const copies = () => [RelPath.parse(".claude/skills/review")];

test("body lists only harness paths", () => {
  const c = config();
  c.paths.output = RelPath.parse("build/.out");
  assert.equal(
    ignoreBody(c, copies()),
    "/build/.out/\n/.harness/\n/instructions/agent.db\n/instructions/agent.db-wal\n/instructions/agent.db-shm\n/instructions/agent.db-journal\n/.claude/skills/review/\n",
  );
});

test("block keeps user rules and is idempotent", () => {
  const p = tempDir();
  p.write(".gitignore", "target/\n*.log\n");
  const root = p.root();
  const state = defaultState();
  const step = () => apply(root, plan(root, state, [], [desired(root, config(), copies())]), state);
  step();
  const text = () => fs.readFileSync(path.join(p.dir, ".gitignore"), "utf8");
  assert.ok(
    text().startsWith("target/\n*.log\n\n# agent-harness:begin agent-harness\n/.output/\n"),
  );
  assert.equal(step().applyId, undefined);
  assert.equal(text().split("agent-harness:begin").length - 1, 1);
});

test("generated rules ignore what they should in real Git", { skip: !hasGit }, () => {
  const p = tempDir();
  p.git("init", "-q");
  p.write(
    ".gitignore",
    `# agent-harness:begin agent-harness\n${ignoreBody(config(), copies())}# agent-harness:end agent-harness\n`,
  );
  const r = gitReport(p.root(), config(), copies());
  assert.equal(r.unavailable, undefined);
  assert.deepEqual(r.excludedShared, []);
  // Spot-check the rules with Git itself.
  for (const [file, ignored] of [
    [".output/x", true],
    [".harness/state.json", true],
    ["instructions/agent.db-wal", true],
    ["instructions/agent-db.sh", false],
    ["instructions/agent-db.ps1", false],
    [".claude/skills/review/SKILL.md", true],
    [".claude/skills/other/SKILL.md", false],
    ["docs/agent.db", false],
    ["instructions/completed.log", false],
  ] as const) {
    const status = spawnSync("git", ["-C", p.dir, "check-ignore", "-q", "--no-index", file]).status;
    assert.equal(status === 0, ignored, file);
  }
});

test("reports parent rules, tracked files and empty dirs", { skip: !hasGit }, () => {
  const repo = tempDir();
  repo.git("init", "-q");
  // A parent directory rule hides the project's references.
  repo.write(".gitignore", "references/\n");
  repo.write("proj/.harness/state.json", "{}");
  repo.git("add", "-f", "proj/.harness/state.json");
  repo.write("proj/references/a.md", "x");
  const r = gitReport(new ProjectRoot(path.join(repo.dir, "proj")), config(), copies());
  assert.ok(
    r.excludedShared.some((e) => e.startsWith("references: excluded by")),
    r.excludedShared.join("; "),
  );
  assert.deepEqual(r.trackedIgnored, [".harness/state.json"]);
  assert.ok(r.notes.some((n) => n.includes("git rm --cached")));
  assert.ok(r.notes.some((n) => n.includes("instructions/.completed/")));
  assert.ok(!r.notes.some((n) => n.includes("`references/`")), "has a file");
  // Read-only: still tracked, nothing committed.
  assert.equal(repo.git("ls-files"), "proj/.harness/state.json\n");
});

test("skips without Git or repository", () => {
  const p = tempDir();
  const r = gitReport(p.root(), config(), [], "git-binary-that-does-not-exist");
  assert.ok(r.unavailable?.includes("not available"));
  const outside = tempDir(); // temp dirs are not inside a repository
  const r2 = gitReport(outside.root(), config(), []);
  if (r2.unavailable) assert.ok(r2.unavailable.includes("never initialized") || !hasGit);
  assert.ok(!fs.existsSync(path.join(outside.dir, ".git")), "no git init");
});
