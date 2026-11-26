import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { parse, SKILLS_CLI, type SkillsEntry } from "./catalog.js";
import { args, conflict, display, findProgram, NPX, run, spawnFor } from "./install.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function tempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-install-"));
  temps.push(dir);
  return dir;
}

function entry(skill?: string): SkillsEntry {
  const e: SkillsEntry = {
    id: "x",
    title: { en: "x" },
    installer: SKILLS_CLI,
    source: "owner/repo@v1.2.3",
  };
  if (skill !== undefined) e.skill = skill;
  return e;
}

test("argument vectors never add agent, scope or yes", () => {
  assert.deepEqual(args(entry("pdf")), ["skills", "add", "owner/repo@v1.2.3", "-s", "pdf"]);
  for (const all of [undefined, "*"]) {
    assert.deepEqual(args(entry(all)), ["skills", "add", "owner/repo@v1.2.3"]);
  }
  for (const skill of [undefined, "pdf", "*"]) {
    const a = args(entry(skill)) ?? [];
    assert.ok(!a.some((x) => ["-a", "-g", "-y", "--yes", "--all"].includes(x)));
  }
  assert.equal(args({ ...entry(), installer: "claude-code-cli" }), undefined, "never run");
  const req = { program: path.join("/usr/bin", NPX), args: args(entry("pdf")) ?? [] };
  assert.equal(display(req), `${NPX} skills add owner/repo@v1.2.3 -s pdf`);
});

test("option-like values are rejected by the catalog", () => {
  for (const [source, skill] of [
    ["-x", "pdf"],
    ["o/r", "--global"],
  ]) {
    const text = `{"schema_version":2,"skills":[{"id":"x","installer":"skills-cli","source":"${source}","skill":"${skill}","title":{"en":"x"}}]}`;
    assert.throws(() => parse(text), `${source} ${skill}`);
  }
});

test("conflicts by skill name", () => {
  const harness = ["Review", "pdf-tools"];
  assert.deepEqual(conflict(entry("review"), harness), { kind: "with", name: "Review" });
  assert.deepEqual(conflict(entry("pdf"), harness), { kind: "none" });
  assert.deepEqual(conflict(entry("*"), harness), { kind: "unknown" });
  assert.deepEqual(conflict(entry(), harness), { kind: "unknown" });
});

test("finds programs on the PATH only", () => {
  const d = tempDir();
  fs.writeFileSync(path.join(d, NPX), "");
  const p = ["/nonexistent-dir", d].join(path.delimiter);
  assert.equal(findProgram(NPX, p), path.join(d, NPX));
  assert.equal(findProgram("no-such-runner", p), undefined);
  assert.equal(findProgram(NPX, undefined), undefined);
});

test("Windows lookup tries PATHEXT extensions (all OSes)", () => {
  const d = tempDir();
  fs.writeFileSync(path.join(d, "tool.cmd"), "");
  const p = `C:\\nowhere;${d}`;
  assert.equal(findProgram("tool", p, "win32", ".EXE;.CMD"), path.join(d, "tool.cmd"));
  assert.equal(findProgram("tool", p, "win32", ".EXE"), undefined);
  assert.equal(findProgram("tool", d, "linux"), undefined, "no PATHEXT outside Windows");
});

test("batch files go through cmd.exe only with safe arguments (all OSes)", () => {
  const program = "C:\\Program Files\\nodejs\\npx.cmd";
  const ok = spawnFor({ program, args: ["skills", "add", "owner/repo@v1.2.3"] }, "win32");
  assert.deepEqual(ok, {
    file: "cmd.exe",
    args: ["/d", "/s", "/c", `""${program}" skills add owner/repo@v1.2.3"`],
    verbatim: true,
  });
  assert.equal(spawnFor({ program: "C:\\x\\NPX.BAT", args: [] }, "win32").file, "cmd.exe");
  for (const bad of ["a b", "a&b", "a|b", "a<b", "a>b", "a^b", "%PATH%", "a!b", 'a"b', "(a)", ""]) {
    assert.throws(() => spawnFor({ program, args: ["skills", bad] }, "win32"), bad);
  }
  assert.throws(() => spawnFor({ program: "C:\\%x%\\npx.cmd", args: [] }, "win32"));
  // Executables and other platforms run directly with the arguments untouched.
  for (const [prog, platform] of [
    ["C:\\nodejs\\npx.exe", "win32"],
    ["/usr/bin/npx.cmd", "linux"],
  ] as const) {
    assert.deepEqual(spawnFor({ program: prog, args: ["a&b"] }, platform), {
      file: prog,
      args: ["a&b"],
      verbatim: false,
    });
  }
});

test("stub runner reports exit codes and gets plain arguments", {
  skip: process.platform === "win32",
}, () => {
  const d = tempDir();
  const stub = path.join(d, "npx");
  // Records its arguments one per line, then exits with the code in code.txt.
  fs.writeFileSync(
    stub,
    '#!/bin/sh\nfor a in "$@"; do printf \'%s\\n\' "$a"; done > args.txt\nexit $(cat code.txt)\n',
    { mode: 0o755 },
  );
  const e = { ...entry("pdf"), source: "owner/repo;touch_PWNED" }; // no shell: stays one argument
  const req = { program: stub, args: args(e) ?? [] };
  for (const code of [0, 3]) {
    fs.writeFileSync(path.join(d, "code.txt"), String(code));
    assert.equal(run(req, d), code);
  }
  assert.equal(
    fs.readFileSync(path.join(d, "args.txt"), "utf8"),
    "skills\nadd\nowner/repo;touch_PWNED\n-s\npdf\n",
  );
  assert.ok(!fs.existsSync(path.join(d, "touch_PWNED")));
  assert.throws(() => run({ program: path.join(d, "missing"), args: [] }, d));
});
