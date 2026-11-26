// Project-root and path rules (Stage 1 spec §1, instruction 0002).
//
// Every managed path is stored as a portable, project-relative `RelPath`
// (`/`-separated) and resolved against the project root, never against the
// executable or a config file location.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** Default instruction root; the user may choose another. */
export const DEFAULT_INSTRUCTIONS_DIR = "instructions";
/** Fixed in Stage 1. */
export const SKILLS_DIR = ".skills";
/** Backlog file name inside the instruction root. */
export const BACKLOG_FILE = "agent.db";
export const DEFAULT_REFERENCES_DIR = "references";
export const DEFAULT_ARTIFACTS_DIR = ".artifacts";
export const DEFAULT_DOCS_DIR = "docs";

export type PathErrorKind =
  | "empty"
  /** The input resolves to the project root itself. */
  | "is-root"
  /** `..` or an absolute path leaves the project root. */
  | "outside-root"
  /** A symlink (or dangling symlink) resolves outside the project root. */
  | "symlink-escape"
  /** Two managed paths share a destination (compared case-insensitively). Paths: [a, b]. */
  | "duplicate"
  /** One managed path is nested inside another. Paths: [outer, inner]. */
  | "nested"
  /** An existing filesystem entry has the wrong kind. */
  | "collision";

export class PathError extends Error {
  readonly kind: PathErrorKind;
  readonly paths: readonly string[];

  constructor(kind: PathErrorKind, ...paths: string[]) {
    super(describe(kind, paths));
    this.kind = kind;
    this.paths = paths;
  }
}

function describe(kind: PathErrorKind, [a = "", b = ""]: string[]): string {
  switch (kind) {
    case "empty":
      return "path is empty";
    case "is-root":
      return `\`${a}\` is the project root itself`;
    case "outside-root":
      return `\`${a}\` is outside the project root`;
    case "symlink-escape":
      return `\`${a}\` resolves through a symlink outside the project root`;
    case "duplicate":
      return `\`${a}\` and \`${b}\` are the same destination`;
    case "nested":
      return `\`${b}\` is nested inside managed path \`${a}\``;
    case "collision":
      return `\`${a}\` collides with an existing entry`;
  }
}

/** Normalized project-relative path: non-empty, `/`-separated, no `.` or `..`. */
export class RelPath {
  private constructor(private readonly value: string) {}

  /**
   * Lexically validates a stored project-relative path. No filesystem
   * access, so symlinks are checked later by `ProjectRoot.resolve`.
   * Throws `PathError`.
   */
  static parse(input: string): RelPath {
    const raw = input.trim();
    if (raw === "") throw new PathError("empty");
    const unified = raw.replaceAll("\\", "/");
    if (isWindowsDrive(unified) || unified.startsWith("/")) {
      throw new PathError("outside-root", raw);
    }
    return normalizeParts(unified.split("/"), raw);
  }

  /** For values built from already-normalized parts. */
  static unchecked(value: string): RelPath {
    return new RelPath(value);
  }

  toString(): string {
    return this.value;
  }

  toJSON(): string {
    return this.value;
  }

  /** Last path component. */
  fileName(): string {
    return this.value.slice(this.value.lastIndexOf("/") + 1);
  }

  join(child: string): RelPath {
    return new RelPath(`${this.value}/${child}`);
  }

  equals(other: RelPath): boolean {
    return this.value === other.value;
  }

  /** True if `this` is `other` or lies under it (case-insensitive). */
  isWithin(other: RelPath): boolean {
    const a = this.value.toLowerCase();
    const b = other.value.toLowerCase();
    return a === b || a.startsWith(`${b}/`);
  }
}

/** Folds `.`/`..` parts into a `RelPath`; `raw` is for errors. */
function normalizeParts(parts: string[], raw: string): RelPath {
  const out: string[] = [];
  for (const part of parts) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (out.pop() === undefined) throw new PathError("outside-root", raw);
      continue;
    }
    out.push(part);
  }
  if (out.length === 0) throw new PathError("is-root", raw);
  return RelPath.unchecked(out.join("/"));
}

function isWindowsDrive(s: string): boolean {
  return /^[A-Za-z]:/.test(s);
}

/** True if `child` is `parent` or lies under it (native absolute paths). */
function isInside(parent: string, child: string): boolean {
  const rel = path.relative(parent, child);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

/** The project root: always the directory the tool runs in. */
export class ProjectRoot {
  readonly path: string;

  constructor(dir: string) {
    this.path = fs.realpathSync.native(dir);
  }

  /** Uses the current working directory. Never searches upward. */
  static fromCwd(): ProjectRoot {
    return new ProjectRoot(process.cwd());
  }

  abs(rel: RelPath): string {
    return path.join(this.path, ...rel.toString().split("/"));
  }

  /**
   * Validates user input (relative, or absolute inside the root) and returns
   * its project-relative form. Throws `PathError`.
   */
  resolve(input: string): RelPath {
    const raw = input.trim();
    if (raw === "") throw new PathError("empty");
    // ponytail: `\` is always a separator so configs stay portable;
    // a literal backslash in a Unix file name is not supported.
    const unified = raw.replaceAll("\\", "/");
    const windows = process.platform === "win32";
    const drive = isWindowsDrive(unified);

    let rel: RelPath;
    if (drive && !windows) {
      throw new PathError("outside-root", raw);
    } else if (drive || path.isAbsolute(raw) || unified.startsWith("/")) {
      // Canonicalizing the existing part lets `C:\..`, other letter case and
      // symlinked aliases match the canonical root (0013).
      const abs = canonicalizeExisting(path.resolve(windows ? raw : unified));
      if (!isInside(this.path, abs)) throw new PathError("outside-root", raw);
      rel = normalizeParts(path.relative(this.path, abs).split(path.sep), raw);
    } else {
      rel = normalizeParts(unified.split("/"), raw);
    }
    this.checkSymlinks(rel);
    return rel;
  }

  /** Rejects paths whose existing part resolves outside the root. */
  private checkSymlinks(rel: RelPath): void {
    let current = this.path;
    for (const part of rel.toString().split("/")) {
      current = path.join(current, part);
      try {
        fs.lstatSync(current);
      } catch {
        return; // rest does not exist yet
      }
      let real: string;
      try {
        real = fs.realpathSync.native(current);
      } catch {
        // Dangling symlink: its target is outside our control.
        throw new PathError("symlink-escape", rel.toString());
      }
      if (!isInside(this.path, real)) throw new PathError("symlink-escape", rel.toString());
    }
  }
}

/** Canonicalizes the longest existing ancestor and re-appends the rest. */
function canonicalizeExisting(p: string): string {
  const tail: string[] = [];
  let base = p;
  for (;;) {
    try {
      return path.join(fs.realpathSync.native(base), ...tail.reverse());
    } catch {
      const parent = path.dirname(base);
      if (parent === base) return p;
      tail.push(path.basename(base));
      base = parent;
    }
  }
}

export type EntryKind = "dir" | "file";

/**
 * Managed layout (Stage 2 spec §4). `instructions` is required; an
 * undefined optional path is disabled: not created, not ignored, not
 * mentioned in guidance. `.skills/` is fixed.
 */
export interface Layout {
  instructions: RelPath;
  /** Temporary artifacts, ignored by Git. */
  artifacts?: RelPath;
  /** Documents the user publishes, tracked. */
  docs?: RelPath;
  /** Documents for agents, tracked. */
  references?: RelPath;
  backlogEnabled: boolean;
}

export const OPTIONAL_PATHS = ["artifacts", "docs", "references"] as const;

export function defaultLayout(): Layout {
  return {
    instructions: RelPath.unchecked(DEFAULT_INSTRUCTIONS_DIR),
    artifacts: RelPath.unchecked(DEFAULT_ARTIFACTS_DIR),
    docs: RelPath.unchecked(DEFAULT_DOCS_DIR),
    references: RelPath.unchecked(DEFAULT_REFERENCES_DIR),
    backlogEnabled: false,
  };
}

export const completedDir = (l: Layout) => l.instructions.join(".completed");
export const completedLog = (l: Layout) => l.instructions.join("completed.log");
export const backlogPath = (l: Layout) => l.instructions.join(BACKLOG_FILE);
export const skillsDir = () => RelPath.unchecked(SKILLS_DIR);

/** Top-level managed entries (enabled ones only) that must not overlap. */
export function layoutEntries(l: Layout): [RelPath, EntryKind][] {
  const out: [RelPath, EntryKind][] = [[l.instructions, "dir"]];
  for (const k of OPTIONAL_PATHS) {
    const p = l[k];
    if (p) out.push([p, "dir"]);
  }
  out.push([skillsDir(), "dir"]);
  return out;
}

/**
 * Checks duplicates, nesting and collisions with existing entries. No name
 * format is enforced (issue #36). Returns every problem, not just the
 * first; empty means valid.
 */
export function validateLayout(l: Layout, root: ProjectRoot): PathError[] {
  const errors: PathError[] = [];
  const entries = layoutEntries(l);
  entries.forEach(([a], i) => {
    for (const [b] of entries.slice(i + 1)) {
      if (a.toString().toLowerCase() === b.toString().toLowerCase()) {
        errors.push(new PathError("duplicate", a.toString(), b.toString()));
      } else if (b.isWithin(a)) {
        errors.push(new PathError("nested", a.toString(), b.toString()));
      } else if (a.isWithin(b)) {
        errors.push(new PathError("nested", b.toString(), a.toString()));
      }
    }
  });
  // Inside the instruction root; reserved even when disabled.
  const all: [RelPath, EntryKind][] = [...entries, [backlogPath(l), "file"]];
  for (const [rel, kind] of all) {
    const e = collision(root, rel, kind);
    if (e) errors.push(e);
  }
  return errors;
}

/** An ancestor that is a file, or the entry itself with the wrong kind. */
function collision(root: ProjectRoot, rel: RelPath, kind: EntryKind): PathError | undefined {
  let current = root.path;
  const parts = rel.toString().split("/");
  for (const [i, part] of parts.entries()) {
    current = path.join(current, part);
    let stat: fs.Stats;
    try {
      stat = fs.statSync(current);
    } catch {
      return undefined;
    }
    const last = i + 1 === parts.length;
    const ok = last ? (kind === "dir" ? stat.isDirectory() : stat.isFile()) : stat.isDirectory();
    if (!ok) return new PathError("collision", rel.toString());
  }
  return undefined;
}

/** Result of trying to hide a directory with a platform attribute. */
export type HiddenAttr =
  | { kind: "applied" }
  /** The platform hides dot-prefixed names; no attribute exists. */
  | { kind: "not-applicable" }
  | { kind: "failed"; message: string };

/**
 * Applies the Windows hidden attribute. A leading dot alone does not hide a
 * directory on Windows, so callers must report `failed` to the user.
 */
export function applyHiddenAttribute(dir: string): HiddenAttr {
  if (process.platform !== "win32") return { kind: "not-applicable" };
  // ponytail: built-in `attrib.exe` instead of a native binding.
  const out = spawnSync("attrib", ["+h", dir], { encoding: "utf8" });
  if (out.error) return { kind: "failed", message: out.error.message };
  if (out.status === 0) return { kind: "applied" };
  return { kind: "failed", message: out.stderr.trim() };
}
