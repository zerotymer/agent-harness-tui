// Apply engine: plan, preview, backup, write, recover (Stage 1 spec §3,
// §7 steps 5-6; instruction 0004).
//
// Callers describe the desired files; this module decides what is safe to
// write using ownership recorded in `.harness/state.json`, then applies
// everything or restores what it can. There is no directory-wide
// transaction: after a failure the report lists what was restored and what
// could not be.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import {
  type AdapterRecord,
  BACKUPS_DIR,
  backupPath,
  type Deployment,
  HARNESS_STATE,
  type HarnessState,
  STATE_FILE,
  toText,
} from "./contract.js";
import { PathError, type ProjectRoot, RelPath } from "./paths.js";

export const APPLY_LOCK = ".harness/apply.lock";

const TOOL_VERSION: string = JSON.parse(
  fs.readFileSync(new URL("../package.json", import.meta.url), "utf8"),
).version;

/** `sha256:<hex>` of `data`. */
export function sha256(data: string | Uint8Array): string {
  return `sha256:${createHash("sha256").update(data).digest("hex")}`;
}

/** What the tool owns: the whole file, a managed block, or nothing. */
export type Ownership =
  | { kind: "file" }
  /** Hashes come from the block parser. `currentHash` is undefined if the block is absent. */
  | { kind: "block"; id: string; newHash: string; currentHash?: string | undefined }
  /** Created only when missing, never changed, never owned (e.g. an empty `completed.log`). */
  | { kind: "create-only" }
  /** The caller could not build safe content; always a conflict, never written. */
  | { kind: "unresolvable"; reason: string };

/** One file the caller wants to exist with `content`. */
export interface Desired {
  path: RelPath;
  content: Buffer;
  ownership: Ownership;
}

export const fileItem = (p: RelPath, content: string | Buffer): Desired => ({
  path: p,
  content: Buffer.from(content),
  ownership: { kind: "file" },
});

export const createOnly = (p: RelPath, content: string | Buffer): Desired => ({
  path: p,
  content: Buffer.from(content),
  ownership: { kind: "create-only" },
});

const blockId = (d: Desired) => (d.ownership.kind === "block" ? d.ownership.id : undefined);

/** `preserve`: exists and is left alone by design (create-only). */
export type Action =
  | { kind: "create" | "update" | "unchanged" | "preserve" }
  | { kind: "conflict"; reason: string };

const writes = (a: Action) => a.kind === "create" || a.kind === "update";

export interface PlanItem {
  desired: Desired;
  action: Action;
  existed: boolean;
}

export interface Plan {
  /** Directories to create, with whether they already exist. */
  dirs: [RelPath, boolean][];
  items: PlanItem[];
  /** Deployments to record (replacing entries with the same target). */
  deployments: Deployment[];
  /** Adapter versions to record (replacing entries for the same agent). */
  adapters: AdapterRecord[];
}

export const conflicts = (p: Plan) => p.items.filter((i) => i.action.kind === "conflict");

export const hasWrites = (p: Plan) =>
  p.items.some((i) => writes(i.action)) || p.dirs.some(([, exists]) => !exists);

/** Text preview, independent of the TUI. */
export function preview(p: Plan): string {
  const out: string[] = [];
  for (const [dir, exists] of p.dirs) out.push(`${(exists ? "keep" : "mkdir").padEnd(9)} ${dir}/`);
  for (const i of p.items) {
    const id = blockId(i.desired);
    const target = id ? `${i.desired.path} [block ${id}]` : `${i.desired.path}`;
    const a = i.action;
    out.push(
      a.kind === "conflict"
        ? `${"CONFLICT".padEnd(9)} ${target}: ${a.reason}`
        : `${a.kind.padEnd(9)} ${target}`,
    );
  }
  return out.map((l) => `${l}\n`).join("");
}

function readIfExists(file: string): Buffer | undefined {
  try {
    return fs.readFileSync(file);
  } catch (e) {
    const code = (e as NodeJS.ErrnoException).code;
    // EISDIR: a directory at the path is reported as a conflict by `plan`.
    if (code === "ENOENT" || code === "EISDIR") return undefined;
    throw e;
  }
}

const isDir = (p: string) => fs.statSync(p, { throwIfNoEntry: false })?.isDirectory() ?? false;

/**
 * Builds a plan by comparing desired content, the recorded baseline and
 * the filesystem. Reads only.
 */
export function plan(
  root: ProjectRoot,
  state: HarnessState,
  dirs: RelPath[],
  desired: Desired[],
): Plan {
  const items = desired.map((d): PlanItem => {
    const abs = root.abs(d.path);
    const current = readIfExists(abs);
    let action: Action;
    try {
      root.resolve(d.path.toString());
      action = isDir(abs)
        ? { kind: "conflict", reason: "a directory exists at this path" }
        : classify(state, d, current);
    } catch (e) {
      if (!(e instanceof PathError)) throw e;
      action = { kind: "conflict", reason: e.message };
    }
    return { desired: d, action, existed: current !== undefined };
  });
  return {
    dirs: dirs.map((d) => [d, isDir(root.abs(d))]),
    items,
    deployments: [],
    adapters: [],
  };
}

export function classify(state: HarnessState, d: Desired, current: Buffer | undefined): Action {
  const o = d.ownership;
  if (o.kind === "unresolvable") return { kind: "conflict", reason: o.reason };
  if (current === undefined) return { kind: "create" };
  if (o.kind === "create-only") return { kind: "preserve" };
  const id = blockId(d);
  const baseline = state.managed.find((m) => m.path.equals(d.path) && m.block === id)?.hash;
  const [currentOwned, newOwned] =
    o.kind === "file" ? [sha256(current), sha256(d.content)] : [o.currentHash, o.newHash];
  if (current.equals(d.content) || currentOwned === newOwned) return { kind: "unchanged" };
  if (currentOwned !== undefined && currentOwned === baseline) return { kind: "update" };
  if (o.kind === "block" && currentOwned === undefined) {
    // Inserting a new block into a user file is the intended use.
    return baseline === undefined
      ? { kind: "update" }
      : { kind: "conflict", reason: "the managed block was removed by the user" };
  }
  if (baseline === undefined) {
    return {
      kind: "conflict",
      reason: "exists but ownership is not recorded (no local state); resolve manually",
    };
  }
  return { kind: "conflict", reason: "modified since the last apply" };
}

export interface ApplyReport {
  /** Backup directory name and `lastSuccess.id`; undefined if nothing ran. */
  applyId?: string;
  created: string[];
  updated: string[];
  unchanged: string[];
  preserved: string[];
  /** Set when applying failed; nothing was recorded as success. */
  failed?: string;
  restored: string[];
  /** Changes that could not be undone after a failure. */
  residual: string[];
}

export class ApplyError extends Error {
  /** `conflicts`: the plan has conflicts, nothing was written. `locked`: lock file exists. */
  constructor(
    readonly kind: "conflicts" | "locked",
    message: string,
    readonly paths: string[] = [],
  ) {
    super(message);
  }
}

function cloneState(s: HarnessState): HarnessState {
  return {
    ...s,
    managed: s.managed.map((m) => ({ ...m })),
    deployments: s.deployments.map((d) => ({ ...d })),
    adapters: s.adapters.map((a) => ({ ...a })),
  };
}

/**
 * Applies `p`. On success `state` is updated in place and written last; on
 * failure files are restored where possible and `state` is untouched.
 * Throws `ApplyError`; I/O errors before writing start propagate as is.
 * `failAt` is a test hook that fails the n-th write (0-based).
 */
export function apply(
  root: ProjectRoot,
  p: Plan,
  state: HarnessState,
  failAt?: number,
): ApplyReport {
  const bad = conflicts(p).map((i) => i.desired.path.toString());
  if (bad.length > 0) {
    throw new ApplyError("conflicts", `resolve conflicts first: ${bad.join(", ")}`, bad);
  }

  const next = cloneState(state);
  for (const i of p.items) recordOwnership(next, i.desired);
  for (const d of p.deployments) {
    next.deployments = next.deployments.filter((o) => !o.target.equals(d.target));
    next.deployments.push(d);
  }
  for (const a of p.adapters) {
    next.adapters = next.adapters.filter((o) => o.agent !== a.agent);
    next.adapters.push(a);
  }
  const report: ApplyReport = {
    created: [],
    updated: [],
    unchanged: [],
    preserved: [],
    restored: [],
    residual: [],
  };
  if (!hasWrites(p) && toText(HARNESS_STATE, next) === toText(HARNESS_STATE, state)) {
    // Idempotent: nothing to do, nothing recorded.
    for (const i of p.items) {
      (i.action.kind === "preserve" ? report.preserved : report.unchanged).push(
        `${i.desired.path}`,
      );
    }
    return report;
  }

  const lock = acquireLock(root);
  try {
    const [stamp, at] = utcNow();
    const applyId = uniqueBackupId(root, stamp);
    next.lastSuccess = { id: applyId, at, toolVersion: TOOL_VERSION };
    report.applyId = applyId;

    const tx = new Tx(root, applyId, failAt);
    try {
      for (const [dir, exists] of p.dirs) if (!exists) tx.mkdir(dir);
      for (const i of p.items) {
        if (writes(i.action)) tx.write(i.desired.path, i.desired.content, i.existed);
      }
      // Success is recorded last, inside the same recoverable sequence.
      const stateRel = RelPath.parse(STATE_FILE);
      const text = toText(HARNESS_STATE, next);
      tx.write(stateRel, Buffer.from(text), fs.existsSync(root.abs(stateRel)));
    } catch (e) {
      report.failed = e instanceof Error ? e.message : String(e);
      [report.restored, report.residual] = tx.rollback();
      return report;
    }
    const lists = {
      create: report.created,
      update: report.updated,
      unchanged: report.unchanged,
      preserve: report.preserved,
    } as const;
    for (const i of p.items) {
      if (i.action.kind !== "conflict") lists[i.action.kind].push(`${i.desired.path}`);
    }
    Object.assign(state, next);
    return report;
  } finally {
    fs.rmSync(lock, { force: true });
  }
}

/** Records the owned hash for file/block items (adopting identical files). */
function recordOwnership(state: HarnessState, d: Desired): void {
  const o = d.ownership;
  if (o.kind === "create-only" || o.kind === "unresolvable") return;
  const hash = o.kind === "file" ? sha256(d.content) : o.newHash;
  const block = blockId(d);
  const m = state.managed.find((e) => e.path.equals(d.path) && e.block === block);
  if (m) m.hash = hash;
  else
    state.managed.push(
      block === undefined ? { path: d.path, hash } : { path: d.path, block, hash },
    );
}

/** Recoverable sequence of filesystem changes. */
class Tx {
  private writes = 0;
  /** [path, had a previous version backed up] */
  private readonly written: [RelPath, boolean][] = [];
  private readonly madeDirs: string[] = [];

  constructor(
    private readonly root: ProjectRoot,
    private readonly applyId: string,
    private readonly failAt: number | undefined,
  ) {}

  /** Creates missing ancestors and records them for rollback. */
  mkdir(rel: RelPath): void {
    const missing: string[] = [];
    for (let p = this.root.abs(rel); !fs.existsSync(p); p = path.dirname(p)) {
      missing.push(p);
      if (path.dirname(p) === p) break;
    }
    for (const dir of missing.reverse()) {
      fs.mkdirSync(dir);
      this.madeDirs.push(dir);
    }
  }

  write(rel: RelPath, content: Buffer, existed: boolean): void {
    const n = this.writes++;
    if (this.failAt === n) throw new Error(`injected failure at write ${n}`);
    const s = rel.toString();
    const slash = s.lastIndexOf("/");
    if (slash > 0) this.mkdir(RelPath.parse(s.slice(0, slash)));
    const abs = this.root.abs(rel);
    if (existed) {
      const backup = this.root.abs(this.backup(rel));
      fs.mkdirSync(path.dirname(backup), { recursive: true });
      fs.copyFileSync(abs, backup);
    }
    this.written.push([rel, existed]);
    atomicWrite(abs, content);
  }

  private backup(rel: RelPath): RelPath {
    const b = backupPath(this.applyId, rel);
    if (!b) throw new Error(`invalid apply id ${this.applyId}`);
    return b;
  }

  /** Undoes writes in reverse order. Returns [restored, residual]. */
  rollback(): [string[], string[]] {
    const restored: string[] = [];
    const residual: string[] = [];
    for (const [rel, existed] of [...this.written].reverse()) {
      const abs = this.root.abs(rel);
      try {
        if (existed) atomicWrite(abs, fs.readFileSync(this.root.abs(this.backup(rel))));
        else fs.rmSync(abs, { force: true });
        restored.push(rel.toString());
      } catch (e) {
        residual.push(`${rel}: ${(e as Error).message}`);
      }
    }
    for (const dir of [...this.madeDirs].reverse()) {
      // Only empty directories we created; anything else stays.
      try {
        fs.rmdirSync(dir);
      } catch (e) {
        residual.push(`${dir}: ${(e as Error).message}`);
      }
    }
    return [restored, residual];
  }
}

/** Writes via a sibling temp file and rename, keeping existing permissions. */
function atomicWrite(file: string, content: Buffer): void {
  const tmp = path.join(path.dirname(file), `.${path.basename(file)}.aht-tmp`);
  try {
    fs.writeFileSync(tmp, content);
    const mode = fs.statSync(file, { throwIfNoEntry: false })?.mode;
    if (mode !== undefined) fs.chmodSync(tmp, mode);
    fs.renameSync(tmp, file);
  } catch (e) {
    fs.rmSync(tmp, { force: true });
    throw e;
  }
}

/** Creates the lock file exclusively and returns its path. */
function acquireLock(root: ProjectRoot): string {
  const file = root.abs(RelPath.parse(APPLY_LOCK));
  fs.mkdirSync(path.dirname(file), { recursive: true });
  try {
    fs.closeSync(fs.openSync(file, "wx"));
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
    throw new ApplyError(
      "locked",
      `${file} exists: another apply is running, or a previous run stopped unexpectedly. ` +
        "Remove it once no other run is active.",
      [file],
    );
  }
  return file;
}

function uniqueBackupId(root: ProjectRoot, base: string): string {
  for (let n = 1; ; n++) {
    const id = n === 1 ? base : `${base}-${n}`;
    if (!fs.existsSync(root.abs(RelPath.parse(`${BACKUPS_DIR}/${id}`)))) return id;
  }
}

/** [`YYYYMMDD-HHMMSS`, RFC 3339] in UTC. */
// ponytail: UTC only; add a local offset if users ask for local times.
export function formatUtc(ms: number): [string, string] {
  const iso = new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");
  return [iso.slice(0, 19).replace(/[-:]/g, "").replace("T", "-"), iso];
}

const utcNow = () => formatUtc(Date.now());
