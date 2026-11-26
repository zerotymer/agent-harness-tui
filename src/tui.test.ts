import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import type { AddressInfo } from "node:net";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { builtinText } from "./catalog.js";
import { NPX } from "./install.js";
import { loadToken } from "./local.js";
import { ProjectRoot } from "./paths.js";
import { type Key, Wizard } from "./tui.js";

// Row indices on the skills step (see `tui.ts`).
const SK_KIND = 0;
const SK_TOKEN = 2;
const SK_REF = 4;
const SK_ADD = 8;
const SK_LIST = 9;
const SK_CATALOGS = 10;
const SK_HOOKS = 11;
const SK_FIXED_ROWS = 12;

/** A directory with stub executables, used as `PATH`. */
function stubBin(names: string[]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-tui-bin-"));
  for (const n of names) {
    if (process.platform === "win32") {
      fs.writeFileSync(path.join(dir, `${n}.cmd`), `@echo off\r\necho ${n} 1.0.0\r\n`);
    } else {
      fs.writeFileSync(path.join(dir, n), `#!/bin/sh\necho '${n} 1.0.0'\n`, { mode: 0o755 });
    }
  }
  return dir;
}

function project() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-tui-"));
  // Claude Code is "installed"; Codex is not.
  const bin = stubBin(["claude"]);
  /** A wizard with detection finished, using the stub `PATH`. */
  const wizard = async (pathValue: string = bin) => {
    const w = await Wizard.create(new ProjectRoot(dir), { pathValue });
    await w.detect();
    return w;
  };
  return {
    dir,
    bin,
    abs: (rel: string) => path.join(dir, rel),
    read: (rel: string) => fs.readFileSync(path.join(dir, rel), "utf8"),
    files: () => fs.readdirSync(dir).length,
    wizard,
    [Symbol.dispose]: () => {
      fs.rmSync(dir, { recursive: true, force: true });
      fs.rmSync(bin, { recursive: true, force: true });
    },
  };
}

const named = (name: Exclude<Key["name"], "char">): Key => ({ name }) as Key;
async function press(w: Wizard, name: Exclude<Key["name"], "char">, times = 1) {
  for (let i = 0; i < times; i++) await w.key(named(name));
}
async function typed(w: Wizard, text: string) {
  for (const ch of text) await w.key({ name: "char", ch });
}
const screen = (w: Wizard) =>
  w
    .lines()
    .map((l) => l.text)
    .join("\n");

test("walks all steps and applies only on Enter", async () => {
  using p = project();
  const w = await p.wizard();
  assert.equal(w.step, "agents");
  assert.match(w.header(), /구성: 내장 default\.json \(버전 /);
  for (const expect of ["paths", "skills", "preview"]) {
    await press(w, "enter");
    assert.equal(w.step, expect);
    assert.equal(p.files(), 0, "nothing written before apply");
  }
  assert.match(screen(w), /create/);
  await press(w, "enter");
  assert.equal(w.step, "done");
  assert.ok(fs.existsSync(p.abs("CLAUDE.md")) && fs.existsSync(p.abs("harness.toml")));
  assert.match(screen(w), /적용을 완료했습니다/);
  await press(w, "enter");
  assert.ok(w.quit);
});

test("cancel, back, Esc and q never write", async () => {
  using p = project();
  let w = await p.wizard();
  await press(w, "enter", 2);
  await press(w, "esc");
  assert.equal(w.step, "paths");
  await press(w, "ctrl-c");
  assert.ok(w.quit);

  w = await p.wizard();
  await press(w, "esc");
  assert.ok(w.quit, "Esc on the first step quits");

  // On the paths step the first field is a text input: `q` is typed.
  w = await p.wizard();
  await press(w, "enter");
  await typed(w, "q");
  assert.ok(!w.quit);
  assert.equal(w.instructions, "instructionsq");
  // The last row (backlog toggle) is not an input: `q` quits.
  await press(w, "up");
  await typed(w, "q");
  assert.ok(w.quit);
  assert.equal(p.files(), 0);
});

test("Korean path input, any name, duplicates rejected", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter");
  await press(w, "down", 3); // references
  await press(w, "backspace", "references".length);
  await typed(w, "참조 문서");
  assert.match(screen(w), /참조 문서/);
  // Artifacts with any name; a duplicate of another path is rejected.
  await press(w, "up", 2);
  await press(w, "backspace", ".artifacts".length);
  await typed(w, "참조 문서");
  await press(w, "enter");
  assert.equal(w.step, "paths");
  assert.match(w.message ?? "", /same destination/);
  await press(w, "backspace", "참조 문서".length);
  await typed(w, "산출물");
  await press(w, "enter");
  assert.equal(w.step, "skills");
  assert.equal(w.config().paths.artifacts?.toString(), "산출물", "no dot needed");
});

test("empty optional paths are disabled: not created, ignored or mentioned", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter");
  await press(w, "down"); // artifacts
  await press(w, "backspace", ".artifacts".length);
  await press(w, "down"); // docs
  await press(w, "backspace", "docs".length);
  assert.match(screen(w), /산출물\(문서\): {6}▏/);
  await press(w, "up", 2); // instructions cannot be empty
  await press(w, "backspace", "instructions".length);
  await press(w, "enter");
  assert.equal(w.step, "paths");
  assert.match(w.message ?? "", /비울 수 없습니다/);
  await typed(w, "instructions");
  await press(w, "enter", 3);
  assert.equal(w.step, "done");
  assert.ok(!fs.existsSync(p.abs(".artifacts")) && !fs.existsSync(p.abs("docs")));
  assert.ok(fs.existsSync(p.abs("references/.gitkeep")));
  assert.ok(!p.read(".gitignore").includes("artifacts"));
  assert.ok(!p.read("CLAUDE.md").includes("docs/"));
  const toml = p.read("harness.toml");
  assert.ok(!toml.includes("artifacts") && !toml.includes('docs = "'), toml);
});

test("custom instructions dir is validated and applied", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter");
  const retype = async (text: string) => {
    await press(w, "backspace", [...w.instructions].length);
    await typed(w, text);
  };
  await retype("references/tasks");
  await press(w, "enter");
  assert.equal(w.step, "paths");
  assert.match(w.message ?? "", /nested/);
  await retype("../tasks");
  await press(w, "enter");
  assert.equal(w.step, "paths");
  assert.match(w.message ?? "", /지시서 경로/);

  await retype("work/tasks");
  await press(w, "enter", 3);
  assert.equal(w.step, "done");
  assert.ok(fs.statSync(p.abs("work/tasks/.completed")).isDirectory());
  assert.ok(fs.statSync(p.abs("work/tasks/completed.log")).isFile());
  assert.ok(!fs.existsSync(p.abs("instructions")));
  assert.match(p.read("harness.toml"), /instructions = "work\/tasks"/);
  assert.ok(p.read("CLAUDE.md").includes("`work/tasks/completed.log`"));
});

test("backlog creates db and scripts and keeps an existing db", async () => {
  const run = async (dir: ReturnType<typeof project>) => {
    const w = await dir.wizard();
    await press(w, "enter");
    await press(w, "up"); // last row: backlog toggle
    await typed(w, " ");
    assert.match(screen(w), /instructions\/agent\.db/);
    await press(w, "enter", 3);
    assert.equal(w.step, "done", screen(w));
  };
  using p = project();
  await run(p);
  for (const f of ["agent.db", "agent-db.sh", "agent-db.ps1"]) {
    assert.ok(fs.statSync(p.abs(`instructions/${f}`)).isFile(), f);
  }
  assert.ok(!fs.existsSync(p.abs("agent.db")));
  assert.match(p.read("harness.toml"), /path = "instructions\/agent\.db"/);

  // A fresh project that already has a database keeps it untouched.
  using q = project();
  fs.mkdirSync(q.abs("instructions"));
  fs.writeFileSync(q.abs("instructions/agent.db"), "user data");
  await run(q);
  assert.equal(q.read("instructions/agent.db"), "user data");
  assert.ok(fs.existsSync(q.abs("instructions/agent-db.sh")));
});

test("agents come first, are detected and one must be installed", async () => {
  using p = project();
  const w = await p.wizard();
  const text = screen(w);
  assert.match(text, /\[x\] Claude Code {2}설치됨 \(claude 1\.0\.0\)/);
  assert.match(text, /\[ \] Codex {2}미설치/);
  assert.match(text, /심볼릭 링크 배치: 선택 불가/);
  await typed(w, " "); // deselect Claude Code
  await press(w, "enter");
  assert.equal(w.step, "agents");
  assert.match(w.message ?? "", /하나 이상 선택/);
  await press(w, "down", 2);
  await typed(w, " "); // select Codex only: not installed
  await press(w, "enter");
  assert.equal(w.step, "agents");
  assert.match(w.message ?? "", /설치된 것이 없습니다/);
  await press(w, "up", 2);
  await typed(w, " "); // Claude Code again
  await press(w, "enter");
  assert.equal(w.step, "paths");
  assert.deepEqual(
    w.config().agents.map((a) => a.id),
    ["claude-code", "codex"],
  );
});

test("agent installs run only after confirmation, several at once", async () => {
  using p = project();
  const bin = stubBin(["npm"]); // nothing detected, npm available
  try {
    const w = await p.wizard(bin);
    assert.equal(w.agents.filter((r) => r.detection?.kind === "installed").length, 0);
    await press(w, "down", 2);
    await typed(w, " "); // select Codex too
    w.focus = w.agents.length * 2; // [설치]
    await press(w, "enter");
    assert.equal(w.step, "agentInstall");
    assert.equal(w.takeRun(), undefined, "opening the screen runs nothing");
    assert.match(
      screen(w),
      /Claude Code: {2}npm(\.cmd)? install -g @anthropic-ai\/claude-code@latest/,
    );
    assert.match(screen(w), /Codex: {2}npm(\.cmd)? install -g @openai\/codex/);
    await press(w, "enter"); // confirm
    const run = w.takeRun();
    assert.equal(run?.kind, "agents");
    assert.deepEqual(
      run?.requests.map((r) => r.args.at(-1)),
      ["@anthropic-ai/claude-code@latest", "@openai/codex"],
    );
    await w.finishRun(run!, [
      { ok: true, value: 0 },
      { ok: true, value: 243 },
    ]);
    assert.match(screen(w), /지난 결과: 실패 \(종료 코드 243\)/);
    await press(w, "esc");
    assert.equal(w.step, "agents");
    assert.equal(p.files(), 0, "the TUI itself wrote nothing");
  } finally {
    fs.rmSync(bin, { recursive: true, force: true });
  }
});

test("invalid project.json is shown and never replaced by the default", async () => {
  using p = project();
  fs.writeFileSync(p.abs("project.json"), '{"schema_version":2,"agents":{}}');
  const w = await p.wizard();
  assert.equal(w.step, "verify");
  assert.match(screen(w), /project\.json: `agents` must be an array/);
  assert.match(screen(w), /대체하지 않습니다/);
  await press(w, "enter");
  assert.ok(w.quit);
});

test("local skill import end to end", async () => {
  using p = project();
  fs.mkdirSync(p.abs("vendor/review"), { recursive: true });
  fs.writeFileSync(p.abs("vendor/review/SKILL.md"), "---\nname: review\n---\n");
  const w = await p.wizard();
  await press(w, "enter", 2);
  assert.equal(w.step, "skills");
  await press(w, "down"); // location
  await typed(w, "vendor");
  w.focus = SK_LIST;
  await press(w, "enter");
  assert.equal(w.catalog.length, 1);
  w.focus = SK_FIXED_ROWS;
  await press(w, "enter"); // pick → fills reference and name
  await press(w, "enter"); // fetch
  assert.ok(w.fetched, w.message);
  w.focus = SK_ADD;
  await press(w, "enter");
  assert.equal(w.imports.length, 1);
  w.focus = SK_KIND;
  await press(w, "enter"); // to preview
  assert.equal(w.step, "preview");
  await press(w, "enter"); // apply
  assert.ok(fs.existsSync(p.abs(".skills/review/SKILL.md")));
  assert.ok(fs.existsSync(p.abs(".claude/skills/review/SKILL.md")));
  const toml = p.read("harness.toml");
  assert.ok(toml.includes('path = "vendor"') && toml.includes('id = "review"'));
});

test("wiki import through a mock gateway saves the token", async () => {
  const TOKEN = "tui-token-3b9d";
  const UUID = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";
  const body = JSON.stringify({
    uuid: UUID,
    type: "skill",
    pages: {
      en: { title: "wiki-skill-import", content: "# en\n", contentHash: "sha256:e1" },
      ko: { title: "wiki-skill-import", content: "# ko\n", contentHash: "sha256:k1" },
    },
  });
  const requests: http.IncomingMessage[] = [];
  const server = http.createServer((req, res) => {
    requests.push(req);
    res.writeHead(200, { "Content-Type": "application/json" }).end(body);
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  using p = project();
  try {
    const w = await p.wizard();
    w.gatewayUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    await press(w, "enter", 2);
    await typed(w, " "); // switch to wiki
    w.focus = SK_LIST;
    await press(w, "enter");
    assert.equal(w.catalog.length, 5, "wiki skills of the configuration");
    w.focus = SK_TOKEN;
    await typed(w, TOKEN);
    assert.ok(!screen(w).includes(TOKEN));
    w.focus = SK_REF;
    await typed(w, UUID);
    await press(w, "down"); // [조회]
    await press(w, "enter");
    assert.ok(w.fetched, w.message);
    assert.equal(requests[0]?.url, `/api/v2/documents/${UUID}?type=skill`);
    assert.equal(requests[0]?.headers.authorization, `Bearer ${TOKEN}`);
    w.focus = SK_ADD;
    await press(w, "enter");
    assert.equal(w.imports.length, 1, w.message);
    w.focus = SK_KIND;
    await press(w, "enter");
    assert.equal(w.step, "preview");
    assert.ok(!screen(w).includes(TOKEN));
    await press(w, "enter"); // apply
    assert.equal(w.step, "done");
    assert.equal(p.read(".skills/wiki-skill-import/SKILL_ko.md"), "# ko\n");
    const lock = p.read("harness.lock.json");
    assert.ok(lock.includes("sha256:e1") && lock.includes("sha256:k1"));
    const done = screen(w);
    assert.ok(done.includes(".harness/local.toml") && !done.includes(TOKEN));
    assert.equal(loadToken(p.dir)?.expose(), TOKEN);
    for (const tracked of ["harness.toml", "harness.lock.json", ".gitignore"]) {
      assert.ok(!p.read(tracked).includes(TOKEN), tracked);
    }
    assert.ok(p.read(".gitignore").includes("/.harness/"));
  } finally {
    server.close();
  }
});

test("catalog view shows the configuration lists and returns", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter", 2);
  w.focus = SK_CATALOGS;
  await press(w, "enter");
  assert.equal(w.step, "catalog");
  assert.equal(w.title(), "3. 소스·스킬 ─ 카탈로그");
  const text = screen(w);
  for (const want of [
    "공개 스킬 3개",
    "위키 스킬 5개 (목록만)",
    "문서 1개 (참조 문서 경로에 저장)",
    "[내려받기 가능] anthropics-skills-readme",
    "훅 1개 (참고용)",
    "wiki-skill-import",
    "직접 실행: claude plugin",
    "anthropics-skills-README.md",
  ]) {
    assert.ok(text.includes(want), `${want}\n${text}`);
  }
  await press(w, "esc");
  assert.equal(w.step, "skills");
  assert.equal(w.focus, SK_CATALOGS);
  assert.equal(p.files(), 0, "viewing catalogs writes nothing");
});

test("documents are downloaded before the preview and recorded in the lock", async () => {
  using p = project();
  const body = "# README\n";
  const urls: string[] = [];
  const w = await p.wizard();
  w.fetchDoc = async (url) => {
    urls.push(String(url));
    return new Response(body);
  };
  await press(w, "enter", 2);
  w.focus = SK_CATALOGS;
  await press(w, "enter");
  w.focus = 3; // first doc row after the three public skills
  await press(w, "enter");
  assert.match(screen(w), /\[선택됨\] anthropics-skills-readme/);
  assert.equal(urls.length, 0, "nothing is downloaded while picking");
  await press(w, "esc");
  assert.match(screen(w), /문서 1개 선택/);
  w.focus = SK_KIND;
  await press(w, "enter"); // preview
  assert.equal(urls.length, 1);
  assert.match(screen(w), /references\/anthropics-skills-README\.md/);
  assert.match(screen(w), /not verified/, "no sha256 is a warning");
  await press(w, "enter");
  assert.equal(w.step, "done", screen(w));
  assert.equal(p.read("references/anthropics-skills-README.md"), body);
  const lock = JSON.parse(p.read("harness.lock.json"));
  assert.equal(lock.docs[0].id, "anthropics-skills-readme");
  assert.equal(lock.docs[0].path, "references/anthropics-skills-README.md");
  assert.match(lock.docs[0].hash, /^sha256:[0-9a-f]{64}$/);

  // Downloaded once: the catalog shows it and it cannot be picked again.
  const again = await p.wizard();
  assert.equal(again.step, "verify");
  assert.match(screen(again), /검증: 설정과 파일이 일치합니다/);
});

test("documents never overwrite other files and need the references path", async () => {
  using p = project();
  fs.mkdirSync(p.abs("references"));
  fs.writeFileSync(p.abs("references/anthropics-skills-README.md"), "mine\n");
  const w = await p.wizard();
  w.fetchDoc = async () => new Response("# README\n");
  await press(w, "enter", 2);
  w.focus = SK_CATALOGS;
  await press(w, "enter");
  w.focus = 3;
  await press(w, "enter");
  await press(w, "esc");
  w.focus = SK_KIND;
  await press(w, "enter"); // preview
  assert.match(screen(w), /충돌 1개/);
  await press(w, "enter");
  assert.notEqual(w.step, "done");
  assert.equal(p.read("references/anthropics-skills-README.md"), "mine\n");

  // A failed download is left out and reported; the rest still applies.
  w.fetchDoc = async () => new Response("x", { status: 500 });
  await press(w, "esc");
  w.focus = SK_KIND;
  await press(w, "enter");
  assert.match(screen(w), /문서 제외: .*HTTP 500/);
  assert.doesNotMatch(screen(w), /충돌/);

  // Without a references path nothing can be picked.
  using q = project();
  const v = await q.wizard();
  await press(v, "enter");
  v.references = "";
  await press(v, "enter");
  v.focus = SK_CATALOGS;
  await press(v, "enter");
  v.focus = 3;
  await press(v, "enter");
  assert.match(screen(v), /\[사용 불가\] anthropics-skills-readme/);
  assert.match(v.message ?? "", /참조 문서 경로/);
});

async function toInstall(w: Wizard, i: number) {
  await press(w, "enter", 2);
  w.focus = SK_CATALOGS;
  await press(w, "enter");
  assert.equal(w.step, "catalog");
  w.focus = i;
  await press(w, "enter");
  assert.equal(w.step, "install");
}

test("install runs only after confirmation and reports the exit code", async () => {
  using p = project();
  const w = await p.wizard();
  w.npx = path.join("/opt/node/bin", NPX);
  await toInstall(w, 0); // built-in `anthropic-pdf`
  assert.equal(w.takeRun(), undefined, "opening the screen runs nothing");
  const source = "anthropics/skills@e4bb5d960d87126ff3b3a8af1a104b7db67560ba";
  assert.ok(screen(w).includes(`${NPX} skills add ${source} -s pdf`), screen(w));
  assert.match(screen(w), /이름 충돌: 없음/);
  await press(w, "enter"); // confirm
  const run = w.takeRun()!;
  assert.deepEqual(run.requests[0]?.args, ["skills", "add", source, "-s", "pdf"]);
  assert.equal(w.takeRun(), undefined, "runs once");
  await w.finishRun(run, [{ ok: true, value: 3 }]);
  assert.match(screen(w), /실패 \(종료 코드 3\)/);
  await w.finishRun(run, [{ ok: true, value: 0 }]);
  assert.match(screen(w), /결과: 성공 \(종료 코드 0\)/);
  await press(w, "esc");
  assert.deepEqual([w.step, w.focus], ["catalog", 0]);
  assert.equal(p.files(), 0, "the TUI itself wrote nothing");
});

test("install is blocked without npx, on conflict and for other installers", async () => {
  using p = project();
  let w = await p.wizard();
  w.npx = undefined;
  await toInstall(w, 0);
  assert.match(screen(w), /사용 불가/);
  await press(w, "enter");
  assert.equal(w.takeRun(), undefined);

  w = await p.wizard();
  w.npx = NPX;
  w.imports.push({
    skillId: "PDF",
    sourceId: "local",
    reference: "pdf",
    acceptPartial: false,
    package: { files: [], missingLocales: [], warnings: [] },
  });
  await toInstall(w, 0);
  assert.match(screen(w), /하네스 스킬 `PDF`/);
  await press(w, "enter");
  assert.equal(w.takeRun(), undefined, "name conflict blocks the run");

  // `claude-code-cli` entry: text only.
  w = await p.wizard();
  w.npx = NPX;
  await toInstall(w, 2);
  assert.match(screen(w), /claude plugin marketplace add/);
  await press(w, "enter");
  assert.equal(w.takeRun(), undefined);
});

test("initialized project is summary and verify only", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter", 4);
  assert.equal(w.step, "done");
  const again = await p.wizard();
  assert.match(screen(again), /이미 초기화된 프로젝트/);
  assert.match(screen(again), /변경할 항목이 없습니다/);
  await press(again, "esc");
  assert.ok(again.quit, "no re-init path");
  assert.equal(again.step, "verify");
});

/** An initialized project, opened again on the check screen. */
async function initialized(p: ReturnType<typeof project>) {
  const w = await p.wizard();
  await press(w, "enter", 4);
  assert.equal(w.step, "done");
  const again = await p.wizard();
  assert.equal(again.step, "verify");
  return again;
}

const snapshot = (p: ReturnType<typeof project>, files: string[]) =>
  Object.fromEntries(files.map((f) => [f, fs.existsSync(p.abs(f)) ? p.read(f) : undefined]));

test("initialized project: add a skill, only additions are applied", async () => {
  using p = project();
  fs.mkdirSync(p.abs("vendor/review"), { recursive: true });
  fs.writeFileSync(p.abs("vendor/review/SKILL.md"), "---\nname: review\n---\n");
  const w = await initialized(p);
  const before = snapshot(p, [
    "CLAUDE_ko.md",
    "instructions/completed.log",
    "instructions/.gitkeep",
  ]);
  assert.match(screen(w), /\[스킬 추가\]/);
  await press(w, "enter"); // [스킬 추가]
  assert.equal(w.step, "skills");
  assert.ok(w.adding);
  await press(w, "down");
  await typed(w, "vendor");
  w.focus = SK_LIST;
  await press(w, "enter");
  w.focus = SK_FIXED_ROWS;
  await press(w, "enter"); // pick
  await press(w, "enter"); // fetch
  w.focus = SK_ADD;
  await press(w, "enter");
  w.focus = SK_KIND;
  await press(w, "enter"); // preview
  assert.equal(w.step, "preview");
  assert.doesNotMatch(screen(w), /CONFLICT/);
  await press(w, "enter"); // apply
  assert.equal(w.step, "done", screen(w));
  assert.ok(fs.existsSync(p.abs(".skills/review/SKILL.md")));
  assert.ok(fs.existsSync(p.abs(".claude/skills/review/SKILL.md")));
  assert.match(p.read("harness.toml"), /id = "review"/);
  assert.match(p.read("CLAUDE.md"), /\| review \| `\.skills\/review\/` \|/);
  // Korean guidance gains the same table row; everything else stays.
  const after = snapshot(p, [
    "CLAUDE_ko.md",
    "instructions/completed.log",
    "instructions/.gitkeep",
  ]);
  assert.equal(after["instructions/completed.log"], before["instructions/completed.log"]);
  assert.equal(after["instructions/.gitkeep"], before["instructions/.gitkeep"]);
  assert.notEqual(after["CLAUDE_ko.md"], before["CLAUDE_ko.md"]);
});

test("initialized project: conflicts hide the additions", async () => {
  using p = project();
  await initialized(p);
  fs.writeFileSync(p.abs("CLAUDE.md"), p.read("CLAUDE.md").replace("### Git", "### Git (edited)"));
  const w = await p.wizard();
  assert.equal(w.canAdd(), false);
  assert.doesNotMatch(screen(w), /\[스킬 추가\]/);
  await press(w, "enter");
  assert.ok(w.quit);
});

test("wiki list comes from the configuration and blocks archived or imported pages", async () => {
  using p = project();
  const uuid = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";
  fs.writeFileSync(
    p.abs("project.json"),
    JSON.stringify({
      schema_version: 2,
      agents: [
        {
          id: "claude-code",
          title: { en: "Claude Code" },
          detect: ["claude"],
          guidance: "CLAUDE.md",
          skills_dir: ".claude/skills",
          default: true,
        },
      ],
      wiki_skills: [
        { id: "live", uuid, type: "claude/hook", title: { en: "Live" } },
        {
          id: "old",
          uuid: "e03f48fb-3e00-41d7-b99d-c32854567d67",
          type: "skill",
          status: "archived",
          title: { en: "Old" },
        },
      ],
    }),
  );
  const w = await p.wizard();
  await press(w, "enter", 2);
  await typed(w, " "); // wiki
  w.focus = SK_LIST;
  await press(w, "enter");
  w.focus = SK_FIXED_ROWS + 1;
  await press(w, "enter");
  assert.match(w.message ?? "", /보관된 항목/);
  w.focus = SK_FIXED_ROWS;
  await press(w, "enter");
  assert.deepEqual([w.reference, w.pageType, w.skillName], [uuid, "claude/hook", "live"]);
  w.imports.push({
    skillId: "live",
    sourceId: "wiki",
    reference: uuid,
    acceptPartial: false,
    package: { files: [], missingLocales: [], warnings: [] },
  });
  w.focus = SK_LIST;
  await press(w, "enter");
  w.focus = SK_FIXED_ROWS;
  await press(w, "enter");
  assert.match(w.message ?? "", /이미 가져온 항목/);
});

test("hooks are recorded and explained, never installed", async () => {
  using p = project();
  const w = await initialized(p);
  await press(w, "down"); // [훅 추가]
  await press(w, "enter");
  assert.equal(w.step, "hooks");
  assert.match(screen(w), /설치하거나 실행하지 않습니다/);
  await typed(w, " ");
  await press(w, "enter");
  assert.equal(w.step, "skills");
  assert.match(screen(w), /\[훅 추가\] 참고 항목 선택 \(1개 선택\)/);
  w.focus = SK_KIND;
  await press(w, "enter"); // preview
  await press(w, "enter"); // apply
  assert.equal(w.step, "done", screen(w));
  assert.match(p.read("harness.toml"), /\[\[hooks\]\]\nid = "claude-code-hooks"/);
  const en = p.read("CLAUDE.md");
  const ko = p.read("CLAUDE_ko.md");
  assert.match(en, /### Hooks \(reference\)/);
  assert.match(en, /Add the hooks you use to the agent's own settings yourself/);
  assert.match(ko, /에이전트 설정에 직접 추가하고/);
  const spans = (t: string) => new Set(t.split("`").filter((_, i) => i % 2 === 1));
  assert.deepEqual(spans(en), spans(ko));
  assert.ok(!fs.existsSync(p.abs(".claude/settings.json")), "nothing installed");

  // A hook the configuration no longer lists is kept and shown as unknown.
  fs.writeFileSync(p.abs("project.json"), '{"schema_version":2,"agents":[]}');
  const again = await p.wizard();
  assert.match(screen(again), /claude-code-hooks \(알 수 없음\)/);
  assert.match(p.read("harness.toml"), /claude-code-hooks/);
});

test("stale guidance blocks can be refreshed; user text is kept", async () => {
  using p = project();
  const w = await initialized(p);
  await press(w, "down"); // [훅 추가]
  await press(w, "enter");
  await typed(w, " ");
  await press(w, "enter");
  w.focus = SK_KIND;
  await press(w, "enter", 2); // preview, apply
  assert.equal(w.step, "done", screen(w));
  fs.appendFileSync(p.abs("CLAUDE.md"), "\nmy notes\n");

  // Same configuration: verified, no refresh offered.
  const same = await p.wizard();
  assert.match(screen(same), /검증: 설정과 파일이 일치합니다/);
  assert.equal(same.rows(), 2);

  // The project configuration renames the hook: only the guidance differs.
  const cfg = JSON.parse(builtinText());
  cfg.hooks[0].title = { en: "Renamed hook", ko: "바뀐 훅" };
  fs.writeFileSync(p.abs("project.json"), JSON.stringify(cfg));
  const stale = await p.wizard();
  assert.ok(stale.guidanceStale());
  assert.match(screen(stale), /\[지침 갱신\]/);
  await press(stale, "down", 2);
  await press(stale, "enter");
  assert.equal(stale.step, "preview");
  await press(stale, "esc"); // back to the check, nothing written
  assert.equal(stale.step, "verify");
  assert.ok(stale.guidanceStale());
  assert.doesNotMatch(p.read("CLAUDE.md"), /Renamed hook/);
  stale.focus = 2;
  await press(stale, "enter", 2); // preview, apply
  assert.equal(stale.step, "done", screen(stale));
  assert.match(p.read("CLAUDE.md"), /Renamed hook[\s\S]*my notes\n$/);
  assert.match(p.read("CLAUDE_ko.md"), /바뀐 훅/);
  const after = await p.wizard();
  assert.match(screen(after), /검증: 설정과 파일이 일치합니다/);
  assert.ok(!after.guidanceStale());
});

test("skills installed by the skills CLI can be updated after confirmation", async () => {
  using p = project();
  const w = await initialized(p);
  assert.ok(!w.verifyActions().includes("update"), "no skills-lock.json, no button");
  fs.writeFileSync(p.abs("skills-lock.json"), "{}");
  const v = await p.wizard();
  assert.deepEqual(v.verifyActions(), ["skills", "hooks", "update"]);
  assert.match(screen(v), /\[스킬 갱신\]/);
  v.npx = undefined;
  v.focus = 2;
  await press(v, "enter");
  assert.equal(v.step, "update");
  await press(v, "enter");
  assert.equal(v.takeRun(), undefined, "nothing runs without npx");
  assert.match(v.message ?? "", /찾을 수 없어/);
  v.npx = NPX;
  assert.ok(screen(v).includes(`${NPX} skills update -p`), screen(v));
  await press(v, "enter");
  const run = v.takeRun();
  assert.deepEqual(run?.requests[0]?.args, ["skills", "update", "-p"]);
  if (run) await v.finishRun(run, [{ ok: true, value: 0 }]);
  assert.match(screen(v), /결과: /);
  await press(v, "esc");
  assert.equal(v.step, "verify");
  assert.equal(v.focus, 2);
  assert.equal(p.read("skills-lock.json"), "{}", "the harness never touches the CLI lock");
});

test("new projects can pick hooks on the skills step too", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter", 2);
  w.focus = SK_HOOKS;
  await press(w, "enter");
  assert.equal(w.step, "hooks");
  await typed(w, " ");
  await press(w, "esc");
  assert.deepEqual(w.config().hooks, [{ id: "claude-code-hooks" }]);
});

test("agent IDs missing from the configuration are kept and shown as unknown", async () => {
  using p = project();
  const w = await p.wizard();
  await press(w, "enter", 4); // Claude Code, applied
  assert.equal(w.step, "done");
  fs.writeFileSync(
    p.abs("project.json"),
    '{"schema_version":2,"agents":[{"id":"other","title":{"en":"Other"},"detect":["other"],"guidance":"OTHER.md","skills_dir":".other/skills"}]}',
  );
  const again = await p.wizard();
  assert.equal(again.step, "verify");
  assert.match(screen(again), /에이전트: claude-code \(알 수 없음/);
  assert.match(p.read("harness.toml"), /id = "claude-code"/, "kept in harness.toml");
});
