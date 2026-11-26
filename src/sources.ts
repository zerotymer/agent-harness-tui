// Skill sources and import (Stage 1 spec §4, instruction 0007).
//
// Two sources share one boundary: a local directory, and the read-only
// `md.zerotymer.net` wiki gateway (gateway API v2,
// `GET /api/v2/documents/{uuid}`). Fetching happens only on an explicit
// request; nothing is cached. Imported content is stored verbatim
// (frontmatter is not interpreted) and bundled files are copied, never
// executed.

import fs from "node:fs";
import path from "node:path";
import { type Desired, fileItem, sha256, TOOL_VERSION } from "./apply.js";
import {
  type HarnessLock,
  isPageType,
  isSlug,
  isUuid,
  type LockedFile,
  type LockedSkill,
  WIKI_GATEWAY_URL,
} from "./contract.js";
import type { Secret } from "./local.js";
import { type ProjectRoot, RelPath, skillsDir } from "./paths.js";

export const GATEWAY_TIMEOUT_MS = 15_000;

export interface CatalogEntry {
  /** Page UUID (gateway) or directory name (local). */
  reference: string;
  title: string;
}

export interface PackageFile {
  /** Relative to the skill directory. */
  path: RelPath;
  bytes: Buffer;
  locale?: string;
  /** Revision reported by the source, verbatim. */
  revision?: string;
}

/** A fetched skill, ready to preview and import. */
export interface Package {
  title?: string;
  files: PackageFile[];
  /** Locales the source did not provide (or provided empty). */
  missingLocales: string[];
  warnings: string[];
}

const emptyPackage = (): Package => ({ files: [], missingLocales: [], warnings: [] });

export type SourceErrorKind =
  | "invalid-reference"
  | "unreachable"
  | "timeout"
  /** 401 or 403. */
  | "auth"
  /** `PAGE_NOT_FOUND`: no locale exists for the UUID. */
  | "not-found"
  /** A 404 that is not the page-not-found problem: the endpoint is missing. */
  | "api-mismatch"
  | "http"
  | "invalid-response"
  | "io"
  /** A package path or symlink leaves the package directory. */
  | "path-escape"
  | "not-a-package";

export class SourceError extends Error {
  constructor(
    readonly kind: SourceErrorKind,
    message: string,
    readonly status?: number,
  ) {
    super(message);
  }
}

const invalidRef = (r: string) =>
  new SourceError("invalid-reference", `invalid reference \`${r}\``);
const pathEscape = (p: string) =>
  new SourceError("path-escape", `\`${p}\` leaves the package directory`);
const notAPackage = (why: string) =>
  new SourceError("not-a-package", `not a skill package: ${why}`);
const ioError = (p: string, e: unknown) =>
  new SourceError("io", `${p}: ${(e as NodeJS.ErrnoException).code ?? (e as Error).message}`);

// ------------------------------------------------------------------- local dir

const isInside = (parent: string, child: string) => {
  const rel = path.relative(parent, child);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
};

const isFile = (p: string) => fs.statSync(p, { throwIfNoEntry: false })?.isFile() ?? false;

/**
 * A directory whose subdirectories with a `SKILL.md` are skill packages.
 * The path may be outside the project for a one-time import; only
 * project-relative paths can be saved in `harness.toml`.
 */
export class LocalDir {
  constructor(readonly dir: string) {}

  list(): CatalogEntry[] {
    let names: string[];
    try {
      names = fs.readdirSync(this.dir);
    } catch (e) {
      throw ioError(this.dir, e);
    }
    return names
      .filter((n) => isFile(path.join(this.dir, n, "SKILL.md")))
      .sort()
      .map((n) => ({ reference: n, title: n }));
  }

  fetch(reference: string): Package {
    let rel: RelPath;
    try {
      rel = RelPath.parse(reference);
    } catch {
      throw invalidRef(reference);
    }
    let base: string;
    try {
      base = fs.realpathSync.native(this.dir);
    } catch (e) {
      throw ioError(this.dir, e);
    }
    let pkg: string;
    try {
      pkg = fs.realpathSync.native(path.join(base, ...rel.toString().split("/")));
    } catch {
      throw notAPackage(`\`${reference}\` does not exist`);
    }
    if (!isInside(base, pkg)) throw pathEscape(reference);
    if (!isFile(path.join(pkg, "SKILL.md"))) throw notAPackage(`\`${reference}\` has no SKILL.md`);
    const files: PackageFile[] = [];
    walk(pkg, pkg, "", files);
    return { ...emptyPackage(), title: rel.fileName(), files };
  }
}

/**
 * Collects regular files under `dir`. Symlinks must resolve to a file
 * inside the package; symlinked directories are refused.
 */
function walk(pkg: string, dir: string, prefix: string, out: PackageFile[]): void {
  let names: string[];
  try {
    names = fs.readdirSync(dir).sort();
  } catch (e) {
    throw ioError(dir, e);
  }
  for (const name of names) {
    const relStr = prefix === "" ? name : `${prefix}/${name}`;
    const p = path.join(dir, name);
    const meta = fs.lstatSync(p);
    let real = p;
    if (meta.isSymbolicLink()) {
      try {
        real = fs.realpathSync.native(p);
      } catch {
        throw pathEscape(relStr);
      }
      if (!isInside(pkg, real)) throw pathEscape(relStr);
      if (fs.statSync(real).isDirectory()) throw notAPackage(`symlinked directory \`${relStr}\``);
    } else if (meta.isDirectory()) {
      walk(pkg, p, relStr, out);
      continue;
    }
    let rel: RelPath;
    try {
      rel = RelPath.parse(relStr);
    } catch {
      throw pathEscape(relStr);
    }
    // ponytail: file modes (e.g. the executable bit) are not carried over;
    // scripts are data here and are never run during import.
    let bytes: Buffer;
    try {
      bytes = fs.readFileSync(real);
    } catch (e) {
      throw ioError(real, e);
    }
    out.push({ path: rel, bytes });
  }
}

// ---------------------------------------------------------------- wiki gateway

/** Read-only wiki gateway client (REST interface of `wiki-skill-import`). */
export class WikiGateway {
  baseUrl = WIKI_GATEWAY_URL;
  /** Token from `.harness/local.toml` or typed in the TUI. */
  storedToken: Secret | undefined;
  timeoutMs = GATEWAY_TIMEOUT_MS;

  /**
   * `tokenEnv` names the environment variable holding the optional token.
   * It is read per request and wins over `storedToken`.
   */
  constructor(public tokenEnv: string) {}

  /** Reads one page. Without a token the request has no `Authorization` header. */
  fetch(uuid: string, pageType?: string): Promise<Package> {
    const env = process.env[this.tokenEnv]?.trim();
    return this.fetchWith(env || this.storedToken?.expose(), uuid, pageType);
  }

  async fetchWith(token: string | undefined, uuid: string, pageType?: string): Promise<Package> {
    if (!isUuid(uuid)) throw invalidRef(uuid);
    if (pageType !== undefined && !isPageType(pageType)) throw invalidRef(pageType);
    const endpoint = `${this.baseUrl}/api/v2/documents/${uuid}`;
    // `/` in a page type is sent as is, like the Rust client did.
    const url = pageType === undefined ? endpoint : `${endpoint}?type=${pageType}`;
    // The gateway asks scripts for an explicit User-Agent.
    const headers: Record<string, string> = { "User-Agent": `agent-harness-tui/${TOOL_VERSION}` };
    if (token) headers.Authorization = `Bearer ${token}`;
    let status: number;
    let body: string;
    try {
      const resp = await fetch(url, { headers, signal: AbortSignal.timeout(this.timeoutMs) });
      status = resp.status;
      body = await resp.text();
    } catch (e) {
      const err = e as Error & { cause?: Error };
      if (err.name === "TimeoutError") {
        throw new SourceError("timeout", `gateway ${this.baseUrl} timed out for ${uuid}`);
      }
      const detail = err.cause?.message ?? err.message;
      throw new SourceError(
        "unreachable",
        `gateway ${this.baseUrl} unreachable for ${uuid}: ${detail}`,
      );
    }
    if (status === 200) return parseGateway(body, uuid);
    if (status === 401 || status === 403) {
      throw new SourceError(
        "auth",
        `gateway refused the request (HTTP ${status}); a valid token is required`,
        status,
      );
    }
    // v2 reports a missing page as a problem document; any other 404 means
    // the endpoint itself is gone.
    if (status === 404 && body.includes("page-not-found")) throw notFound(uuid);
    if (status === 404) {
      throw new SourceError(
        "api-mismatch",
        `gateway API mismatch: ${endpoint} has no such endpoint (HTTP 404); the gateway contract may have changed`,
        404,
      );
    }
    throw new SourceError("http", `gateway returned HTTP ${status}`, status);
  }
}

const notFound = (uuid: string) => new SourceError("not-found", `PAGE_NOT_FOUND: ${uuid}`);

const invalidResponse = (why: string) =>
  new SourceError("invalid-response", `unexpected gateway response: ${why}`);

/** Maps a gateway body to a package: `en` → `SKILL.md`, `ko` → `SKILL_ko.md`. */
export function parseGateway(body: string, uuid: string): Package {
  let doc: unknown;
  try {
    doc = JSON.parse(body);
  } catch (e) {
    if (body.includes("PAGE_NOT_FOUND")) throw notFound(uuid);
    throw invalidResponse((e as Error).message);
  }
  const pages = (doc as { pages?: unknown } | null)?.pages;
  if (typeof pages !== "object" || pages === null) {
    if (body.includes("PAGE_NOT_FOUND")) throw notFound(uuid);
    throw invalidResponse("missing field `pages`");
  }
  const pkg = emptyPackage();
  for (const [locale, file] of [
    ["en", "SKILL.md"],
    ["ko", "SKILL_ko.md"],
  ] as const) {
    const page = (pages as Record<string, unknown>)[locale];
    if (page === undefined || page === null) {
      pkg.missingLocales.push(locale);
      continue;
    }
    const { title, content, contentHash } = page as Record<string, unknown>;
    if (typeof content !== "string") throw invalidResponse(`\`${locale}.content\` is not a string`);
    if (pkg.title === undefined && typeof title === "string") pkg.title = title;
    if (content.trim() === "") {
      pkg.warnings.push(`\`${locale}\` content is empty; \`${file}\` will not be written`);
      pkg.missingLocales.push(locale);
      continue;
    }
    const f: PackageFile = { path: RelPath.unchecked(file), bytes: Buffer.from(content), locale };
    if (typeof contentHash === "string") f.revision = contentHash;
    pkg.files.push(f);
  }
  if (pkg.files.length === 0) throw notFound(uuid);
  return pkg;
}

// --------------------------------------------------------------------- import

export interface ImportRequest {
  /** Directory name under `.skills/`, chosen by the user. */
  skillId: string;
  sourceId: string;
  reference: string;
  pageType?: string;
  /** Required when the source lacks a locale. */
  acceptPartial: boolean;
}

export type ImportErrorKind =
  | "invalid-id"
  /** Refresh is a Stage 2 update, not a re-import. */
  | "already-installed"
  /** `.skills/<id>/` exists but is not a skill this tool imported. */
  | "destination-exists"
  | "partial-not-accepted"
  | "empty";

export class ImportError extends Error {
  constructor(
    readonly kind: ImportErrorKind,
    message: string,
  ) {
    super(message);
  }
}

/** Default skill name from a title: lowercase ASCII slug, or `skill`. */
export function slugify(title: string): string {
  let out = "";
  for (const c of title) {
    if (/^[A-Za-z0-9]$/.test(c)) out += c.toLowerCase();
    else if (out !== "" && !out.endsWith("-")) out += "-";
  }
  out = out.replace(/-+$/, "");
  return out === "" ? "skill" : out;
}

/**
 * Plans the files for `.skills/<id>/` and the lock entry. The caller adds
 * the updated lock (see `lockWith`) to the same apply. Throws `ImportError`.
 */
export function importItems(
  root: ProjectRoot,
  lock: HarnessLock,
  req: ImportRequest,
  pkg: Package,
): [Desired[], LockedSkill] {
  const id = req.skillId;
  if (!isSlug(id)) throw new ImportError("invalid-id", `skill name \`${id}\` must be a slug`);
  if (lock.skills.some((s) => s.id.toLowerCase() === id.toLowerCase())) {
    throw new ImportError("already-installed", `\`${id}\` is already installed`);
  }
  const dest = skillsDir().join(id);
  if (fs.existsSync(root.abs(dest))) {
    throw new ImportError("destination-exists", `\`${dest}\` already exists and is not managed`);
  }
  if (pkg.missingLocales.length > 0 && !req.acceptPartial) {
    throw new ImportError(
      "partial-not-accepted",
      `missing locale(s) ${pkg.missingLocales.join(", ")}; accept a partial import to continue`,
    );
  }
  if (pkg.files.length === 0) throw new ImportError("empty", "the package has no files");
  const desired = pkg.files.map((f) => fileItem(dest.join(f.path.toString()), f.bytes));
  const files = pkg.files.map((f): LockedFile => {
    const l: LockedFile = { path: f.path, hash: sha256(f.bytes) };
    if (f.locale !== undefined) l.locale = f.locale;
    if (f.revision !== undefined) l.revision = f.revision;
    return l;
  });
  const locked: LockedSkill = { id, source: req.sourceId, reference: req.reference, files };
  if (req.pageType !== undefined) locked.pageType = req.pageType;
  return [desired, locked];
}

/** `lock` plus `skill`. */
export const lockWith = (lock: HarnessLock, skill: LockedSkill): HarnessLock => ({
  ...lock,
  skills: [...lock.skills, skill],
});
