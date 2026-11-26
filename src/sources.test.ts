import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import type { AddressInfo } from "node:net";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { apply, fileItem, plan, sha256 } from "./apply.js";
import {
  defaultLock,
  defaultState,
  fromText,
  HARNESS_LOCK,
  LOCK_FILE,
  load,
  toText,
} from "./contract.js";
import { Secret } from "./local.js";
import { ProjectRoot, RelPath } from "./paths.js";
import {
  type ImportRequest,
  importItems,
  LocalDir,
  lockWith,
  type Package,
  parseGateway,
  type SourceErrorKind,
  slugify,
  WikiGateway,
} from "./sources.js";

const temps: string[] = [];
after(() => {
  for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
});

function tempDir() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-sources-"));
  temps.push(dir);
  return {
    dir,
    write: (r: string, text: string) => {
      fs.mkdirSync(path.dirname(path.join(dir, r)), { recursive: true });
      fs.writeFileSync(path.join(dir, r), text);
    },
  };
}

const TOKEN = "test-token-9f8e7d";
const UUID = "14cea96f-f1e2-461e-b9a7-96b5a6f63b67";

function body(en?: string, ko?: string): string {
  const page = (c?: string) =>
    c === undefined
      ? null
      : {
          id: 1,
          path: "skill/x",
          locale: "l",
          title: "Wiki Skill Import",
          content: c,
          tags: [],
          contentHash: "sha256:abc",
        };
  return JSON.stringify({ uuid: UUID, type: "skill", pages: { en: page(en), ko: page(ko) } });
}

interface Seen {
  url: string;
  headers: http.IncomingHttpHeaders;
}

/** Serves one canned response; resolves with what the client sent. */
async function serveOnce(status: number, text: string): Promise<[string, Promise<Seen>]> {
  let done: (s: Seen) => void = () => {};
  const seen = new Promise<Seen>((r) => {
    done = r;
  });
  const server = http.createServer((req, res) => {
    res.writeHead(status, { "Content-Type": "application/json", Connection: "close" });
    res.end(text);
    done({ url: req.url ?? "", headers: req.headers });
    server.close();
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  return [`http://127.0.0.1:${(server.address() as AddressInfo).port}`, seen];
}

function gateway(url: string): WikiGateway {
  const g = new WikiGateway("AHT_TEST_UNSET_TOKEN_VAR");
  g.baseUrl = url;
  g.timeoutMs = 5000;
  return g;
}

async function rejectsKind(p: Promise<unknown>, kind: SourceErrorKind): Promise<Error> {
  try {
    await p;
  } catch (e) {
    assert.equal((e as { kind?: string }).kind, kind, (e as Error).message);
    return e as Error;
  }
  assert.fail(`expected ${kind}`);
}

test("gateway request shape and both locales", async () => {
  const [url, seen] = await serveOnce(200, body("# en\n", "# ko\n"));
  const pkg = await gateway(url).fetchWith(TOKEN, UUID, "skill");
  const req = await seen;
  assert.equal(req.url, `/api/v2/documents/${UUID}?type=skill`);
  assert.equal(req.headers.authorization, `Bearer ${TOKEN}`);
  assert.match(req.headers["user-agent"] ?? "", /^agent-harness-tui\//);
  assert.deepEqual(
    pkg.files.map((f) => `${f.path}`),
    ["SKILL.md", "SKILL_ko.md"],
  );
  assert.equal(pkg.files[0]?.bytes.toString(), "# en\n", "stored verbatim");
  assert.equal(pkg.files[1]?.revision, "sha256:abc");
  assert.equal(pkg.title, "Wiki Skill Import");
  assert.deepEqual(pkg.missingLocales, []);
});

test("gateway without type omits parameter", async () => {
  const [url, seen] = await serveOnce(200, body("x"));
  await gateway(url).fetchWith(TOKEN, UUID);
  assert.equal((await seen).url, `/api/v2/documents/${UUID}`);
});

test("gateway status errors", async () => {
  for (const [status, kind] of [
    [401, "auth"],
    [403, "auth"],
    [500, "http"],
  ] as const) {
    const [url] = await serveOnce(status, '{"error":"x"}');
    const e = await rejectsKind(gateway(url).fetchWith(TOKEN, UUID), kind);
    assert.equal((e as { status?: number }).status, status);
  }
});

test("gateway timeout and unreachable", async () => {
  const sockets: import("node:net").Socket[] = [];
  const server = http.createServer(() => {}); // never answers
  server.on("connection", (s) => sockets.push(s));
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  const g = gateway(url);
  g.timeoutMs = 200;
  await rejectsKind(g.fetchWith(TOKEN, UUID), "timeout");
  for (const s of sockets) s.destroy();
  await new Promise((r) => server.close(r));

  // Nothing listens on this port any more.
  let err: Error | undefined;
  try {
    await gateway(url).fetchWith(TOKEN, UUID);
  } catch (e) {
    err = e as Error;
  }
  assert.ok(err && ["unreachable", "timeout"].includes((err as { kind?: string }).kind ?? ""));
  assert.ok(!`${err.message} ${JSON.stringify(err)}`.includes(TOKEN), "token never in errors");
});

test("gateway without token sends no Authorization", async () => {
  // `fetch` reads the (unset) variable and proceeds without a token.
  const [url, seen] = await serveOnce(200, body("x"));
  await gateway(url).fetch(UUID);
  assert.equal((await seen).headers.authorization, undefined);
  await rejectsKind(
    gateway("http://127.0.0.1:9").fetchWith(TOKEN, "not-a-uuid"),
    "invalid-reference",
  );
});

test("gateway accepts multi-segment type only", async () => {
  const [url, seen] = await serveOnce(200, body("x"));
  await gateway(url).fetchWith(undefined, UUID, "claude/hook");
  assert.equal((await seen).url, `/api/v2/documents/${UUID}?type=claude/hook`);
  for (const bad of ["../x", "claude//hook", "/skill", "skill/", "a b"]) {
    await rejectsKind(
      gateway("http://127.0.0.1:9").fetchWith(undefined, UUID, bad),
      "invalid-reference",
    );
  }
});

test("gateway 404 distinguishes missing page from missing endpoint", async () => {
  const problem = JSON.stringify({
    type: "urn:wikijs-gateway:problem:page-not-found",
    title: "Page not found",
    status: 404,
    uuid: UUID,
  });
  let [url] = await serveOnce(404, problem);
  await rejectsKind(gateway(url).fetchWith(undefined, UUID), "not-found");
  // An old or unknown route: plain 404 without the problem type.
  [url] = await serveOnce(404, '{"detail":"Not Found"}');
  const e = await rejectsKind(gateway(url).fetchWith(TOKEN, UUID), "api-mismatch");
  assert.ok(e.message.includes("/api/v2/documents/"));
  assert.ok(!e.message.includes(TOKEN));
});

test("gateway body mapping", () => {
  const one = parseGateway(body("# en"), UUID);
  assert.deepEqual(one.missingLocales, ["ko"]);
  assert.equal(one.files.length, 1);
  const empty = parseGateway(body("  \n", "# ko"), UUID);
  assert.deepEqual(empty.missingLocales, ["en"]);
  assert.equal(empty.warnings.length, 1);
  assert.equal(`${empty.files[0]?.path}`, "SKILL_ko.md");
  assert.throws(() => parseGateway(body(), UUID), { kind: "not-found" });
  assert.throws(() => parseGateway('{"error":"PAGE_NOT_FOUND"}', UUID), { kind: "not-found" });
  assert.throws(() => parseGateway("<html>", UUID), { kind: "invalid-response" });
});

test("gateway sends stored token and never shows it", async () => {
  const [url, seen] = await serveOnce(200, body("x"));
  const g = gateway(url);
  g.storedToken = Secret.from(TOKEN);
  await g.fetch(UUID);
  assert.equal((await seen).headers.authorization, `Bearer ${TOKEN}`);
  assert.ok(!JSON.stringify(g).includes(TOKEN));
});

test("environment token wins over stored token", async () => {
  const VAR = "AHT_TEST_ENV_TOKEN_PRECEDENCE";
  process.env[VAR] = "env-token-1";
  try {
    const [url, seen] = await serveOnce(200, body("x"));
    const g = gateway(url);
    g.tokenEnv = VAR;
    g.storedToken = Secret.from(TOKEN);
    await g.fetch(UUID);
    assert.equal((await seen).headers.authorization, "Bearer env-token-1");
  } finally {
    delete process.env[VAR];
  }
});

test("local list and fetch", () => {
  const src = tempDir();
  src.write("review/SKILL.md", "# Review\n");
  src.write("review/scripts/run.sh", "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n");
  src.write("review/templates/a.md", "tpl\n");
  src.write("notes/readme.md", "not a skill\n");
  const local = new LocalDir(src.dir);
  assert.deepEqual(local.list(), [{ reference: "review", title: "review" }]);
  assert.deepEqual(
    local.fetch("review").files.map((f) => `${f.path}`),
    ["SKILL.md", "scripts/run.sh", "templates/a.md"],
  );
  assert.throws(() => local.fetch("notes"), { kind: "not-a-package" });
  assert.throws(() => local.fetch("../x"), { kind: "invalid-reference" });
});

test("local rejects escaping symlinks", { skip: process.platform === "win32" }, () => {
  const src = tempDir();
  const outside = tempDir();
  outside.write("secret.txt", "secret");
  src.write("a/SKILL.md", "# A\n");
  fs.symlinkSync(path.join(outside.dir, "secret.txt"), path.join(src.dir, "a/leak.txt"));
  src.write("b/SKILL.md", "# B\n");
  fs.symlinkSync(path.join(src.dir, "b/missing"), path.join(src.dir, "b/dangling"));
  src.write("c/SKILL.md", "# C\n");
  fs.symlinkSync(path.join(src.dir, "c/SKILL.md"), path.join(src.dir, "c/alias.md"));
  fs.symlinkSync(outside.dir, path.join(src.dir, "d"));
  const local = new LocalDir(src.dir);
  assert.throws(() => local.fetch("a"), {
    kind: "path-escape",
    message: "`leak.txt` leaves the package directory",
  });
  assert.throws(() => local.fetch("b"), {
    kind: "path-escape",
    message: "`dangling` leaves the package directory",
  });
  assert.equal(local.fetch("c").files.length, 2);
  assert.throws(
    () => local.fetch("d"),
    (e: { kind?: string }) => e.kind === "path-escape" || e.kind === "not-a-package",
  );
});

test("import plans files and lock without running scripts", () => {
  const src = tempDir();
  src.write("review/SKILL.md", "# Review\n");
  src.write("review/scripts/run.sh", "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n");
  const pkg = new LocalDir(src.dir).fetch("review");

  const project = tempDir();
  const root = new ProjectRoot(project.dir);
  const lock = defaultLock();
  const req: ImportRequest = {
    skillId: "review",
    sourceId: "vendored",
    reference: "review",
    acceptPartial: false,
  };
  const [desired, locked] = importItems(root, lock, req, pkg);
  assert.equal(locked.files[0]?.hash, sha256("# Review\n"));
  const next = lockWith(lock, locked);
  desired.push(fileItem(RelPath.parse(LOCK_FILE), toText(HARNESS_LOCK, next)));
  const state = defaultState();
  apply(root, plan(root, state, [], desired), state);

  assert.equal(
    fs.readFileSync(path.join(project.dir, ".skills/review/scripts/run.sh"), "utf8"),
    "#!/bin/sh\ntouch SHOULD_NOT_EXIST\n",
  );
  assert.ok(!fs.existsSync(path.join(project.dir, "SHOULD_NOT_EXIST")));
  assert.ok(!fs.existsSync(path.join(src.dir, "review/SHOULD_NOT_EXIST")));
  assert.deepEqual(
    load(HARNESS_LOCK, project.dir),
    fromText(HARNESS_LOCK, toText(HARNESS_LOCK, next)),
  );

  // Same skill again: refused (refresh is Stage 2).
  assert.throws(() => importItems(root, next, req, pkg), { kind: "already-installed" });
});

test("import refuses unmanaged destination and partial", () => {
  const project = tempDir();
  project.write(".skills/mine/SKILL.md", "user's own skill\n");
  const root = new ProjectRoot(project.dir);
  const pkg: Package = parseGateway(body("# en"), UUID);
  const req: ImportRequest = {
    skillId: "mine",
    sourceId: "wiki",
    reference: UUID,
    pageType: "skill",
    acceptPartial: false,
  };
  const lock = defaultLock();
  assert.throws(() => importItems(root, lock, req, pkg), { kind: "destination-exists" });
  req.skillId = "wiki-skill-import";
  assert.throws(() => importItems(root, lock, req, pkg), {
    kind: "partial-not-accepted",
    message: "missing locale(s) ko; accept a partial import to continue",
  });
  req.acceptPartial = true;
  const [desired, locked] = importItems(root, lock, req, pkg);
  assert.equal(desired.length, 1, "no fabricated translation");
  assert.equal(locked.files[0]?.locale, "en");
  req.skillId = "../x";
  assert.throws(() => importItems(root, lock, req, pkg), { kind: "invalid-id" });
});

test("slugify titles", () => {
  assert.equal(slugify("Wiki Skill Import"), "wiki-skill-import");
  assert.equal(slugify("  Git: Branch / Guideline! "), "git-branch-guideline");
  assert.equal(slugify("한글 제목"), "skill");
});
