import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { emptyDb, PS1_FILE, planItems, SH_FILE } from "./backlog.js";
import { defaultConfig, type HarnessConfig } from "./contract.js";

function enabled(): HarnessConfig {
  const c = defaultConfig();
  c.backlog.enabled = true;
  return c;
}

function tempDir(tag: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), `agent-harness-${tag}-`));
}

test("disabled plans nothing", async () => {
  assert.deepEqual(await planItems(defaultConfig()), []);
});

test("plans db and scripts in instruction root", async () => {
  const items = await planItems(enabled());
  assert.deepEqual(
    items.map((d) => d.path.toString()),
    ["instructions/agent.db", "instructions/agent-db.sh", "instructions/agent-db.ps1"],
  );
  assert.ok(items.every((d) => d.ownership.kind === "create-only"));
  for (const d of items.slice(1)) {
    const text = d.content.toString("utf8");
    assert.ok(!text.includes("@SCHEMA@"));
    assert.ok(text.includes("CREATE TABLE IF NOT EXISTS backlog"));
    assert.ok(text.includes("AGENT_DB_ACTOR"));
  }
});

test("empty db has schema and WAL", async () => {
  const dir = tempDir("dbt");
  try {
    const file = path.join(dir, "agent.db");
    fs.writeFileSync(file, await emptyDb());
    const db = new DatabaseSync(file);
    try {
      assert.equal(db.prepare("PRAGMA journal_mode").get()?.journal_mode, "wal");
      const names = db
        .prepare(
          "SELECT name FROM sqlite_master WHERE type IN ('table','index') " +
            "AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .all()
        .map((r) => r.name);
      assert.deepEqual(names, [
        "backlog",
        "idx_backlog_status",
        "idx_instructions_status",
        "instructions",
      ]);
    } finally {
      db.close();
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

const has = (cmd: string, ...args: string[]) => spawnSync(cmd, args).status === 0;

type Run = (dir: string, args: string[]) => [boolean, string];

/** Runs the generated script end to end in a temp instruction root. */
async function exercise(tag: string, run: Run): Promise<void> {
  const dir = tempDir(tag);
  try {
    for (const d of await planItems(enabled())) {
      fs.writeFileSync(path.join(dir, d.path.fileName()), d.content);
    }
    const old = "0000-old | 기존 완료 | 2026-01-01 00:00:00 +0900";
    // No trailing newline: the script must not join lines.
    fs.writeFileSync(path.join(dir, "completed.log"), old);
    const ok = (...args: string[]) => {
      const [success, out] = run(dir, args);
      assert.ok(success, `${args.join(" ")}: ${out}`);
      return out;
    };
    const fails = (...args: string[]) => assert.ok(!run(dir, args)[0], args.join(" "));

    ok("init");
    assert.ok(ok("instruction-show", "0000-old").includes("completed"));
    assert.equal(ok("backlog-add", "할 일 '하나'", "설명").trim(), "1");
    assert.ok(ok("backlog-list").includes("할 일 '하나'"));
    ok("backlog-complete", "1");
    fails("backlog-complete", "9");
    ok("instruction-upsert", "u-1", "새 지시서");
    ok("instruction-complete", "u-1", "요약");
    fails("instruction-complete", "u-1", "again");
    fails("instruction-complete", "u-2", "a|b");

    const lines = fs.readFileSync(path.join(dir, "completed.log"), "utf8").split(/\r?\n/);
    if (lines.at(-1) === "") lines.pop();
    assert.equal(lines.length, 2, lines.join("\n"));
    assert.equal(lines[0], old, "existing line untouched");
    assert.ok(lines[1]?.startsWith("u-1 | 요약 | "), lines[1]);

    const db = new DatabaseSync(path.join(dir, "agent.db"));
    try {
      assert.deepEqual(
        {
          ...db
            .prepare("SELECT title, status, created_by FROM instructions WHERE uuid='u-1'")
            .get(),
        },
        { title: "새 지시서", status: "completed", created_by: "tester" },
      );
      assert.deepEqual(
        { ...db.prepare("SELECT updated_by FROM backlog WHERE id=1").get() },
        {
          updated_by: "tester",
        },
      );
    } finally {
      db.close();
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runner(cmd: string, prefix: (dir: string) => string[]): Run {
  return (dir, args) => {
    const out = spawnSync(cmd, [...prefix(dir), ...args], {
      encoding: "utf8",
      env: { ...process.env, AGENT_DB_ACTOR: "tester" },
    });
    return [out.status === 0, out.stdout + out.stderr];
  };
}

const sqlite3 = has("sqlite3", "--version");

test(
  "bash script works end to end",
  {
    skip:
      process.platform === "win32" || !has("bash", "--version") || !sqlite3
        ? "bash or sqlite3 not available"
        : false,
  },
  () =>
    exercise(
      "sh",
      runner("bash", (dir) => [path.join(dir, SH_FILE)]),
    ),
);

const pwsh = ["pwsh", "powershell"].find((s) => has(s, "-NoProfile", "-Command", "exit 0"));

test(
  "PowerShell script works end to end",
  {
    skip: !pwsh || !sqlite3 ? "PowerShell or sqlite3 not available" : false,
  },
  () =>
    exercise(
      "ps1",
      runner(pwsh ?? "pwsh", (dir) => [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        path.join(dir, PS1_FILE),
      ]),
    ),
);
