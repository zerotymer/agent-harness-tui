// Local JSON catalogs (Stage 2 spec §1, `HARNESS_CATALOG_CONTRACT`, v1).
//
// Three files share one envelope: `wiki.json`, `skills.json`, `docs.json`.
// This module only reads local files and never touches the network.
// ponytail: only entry validation and the built-in samples are ported
// (0025). The environment-variable and beside-the-executable lookups are
// dropped; catalog v2 (0023) replaces this file.

import fs from "node:fs";
import { isPageType, isSlug, isUuid } from "./contract.js";

export const SCHEMA_VERSION = 1;
export const SKILLS_CLI = "skills-cli";

export type Kind = "wiki" | "skills" | "docs";

/** `{ en?, ko? }`; at least one language. Missing ones are never made up. */
export interface Text {
  en?: string;
  ko?: string;
}

/** Korean first (the UI language), then English. */
export const display = (t: Text) => t.ko ?? t.en ?? "";

interface Common {
  id: string;
  title: Text;
  summary?: Text;
  version?: string;
  updated?: string;
}

export interface WikiEntry extends Common {
  uuid: string;
  pageType: string;
  target?: "skill" | "doc";
  status: "active" | "archived";
}

export interface SkillsEntry extends Common {
  installer: string;
  source: string;
  skill?: string;
  install?: string;
}

export interface DocsEntry extends Common {
  url: string;
  filename: string;
  sha256?: string;
}

export interface Parsed<E> {
  catalogVersion?: string;
  entries: E[];
}

type Obj = Record<string, unknown>;

/** Thrown for structural problems; collected per entry otherwise. */
class Shape extends Error {}

function obj(v: unknown, what: string, keys: readonly string[]): Obj {
  if (typeof v !== "object" || v === null || Array.isArray(v))
    throw new Shape(`${what}: expected an object`);
  const unknown = Object.keys(v).filter((k) => !keys.includes(k));
  if (unknown.length > 0) throw new Shape(`${what}: unknown field \`${unknown[0]}\``);
  return v as Obj;
}

function str(o: Obj, key: string, what: string): string;
function str(o: Obj, key: string, what: string, optional: true): string | undefined;
function str(o: Obj, key: string, what: string, optional = false): string | undefined {
  const v = o[key];
  if (v === undefined || v === null) {
    if (optional) return undefined;
    throw new Shape(`${what}: missing field \`${key}\``);
  }
  if (typeof v !== "string") throw new Shape(`${what}: \`${key}\` must be a string`);
  return v;
}

function text(v: unknown, what: string): Text {
  const o = obj(v, what, ["en", "ko"]);
  const t: Text = {};
  const en = str(o, "en", what, true);
  const ko = str(o, "ko", what, true);
  if (en !== undefined) t.en = en;
  if (ko !== undefined) t.ko = ko;
  return t;
}

const validText = (t: Text) => {
  const vals = [t.en, t.ko].filter((s) => s !== undefined);
  return vals.length > 0 && vals.every((s) => s.trim() !== "");
};

function oneOf<T extends string>(
  v: string | undefined,
  allowed: readonly T[],
  what: string,
): T | undefined {
  if (v === undefined) return undefined;
  if (!(allowed as readonly string[]).includes(v)) {
    throw new Shape(`${what}: unknown variant \`${v}\`, expected one of ${allowed.join(", ")}`);
  }
  return v as T;
}

const COMMON = ["id", "title", "summary", "version", "updated"] as const;

function common(o: Obj, what: string): Common {
  const c: Common = { id: str(o, "id", what), title: text(o.title, `${what}.title`) };
  if (o.summary !== undefined && o.summary !== null) c.summary = text(o.summary, `${what}.summary`);
  const version = str(o, "version", what, true);
  const updated = str(o, "updated", what, true);
  if (version !== undefined) c.version = version;
  if (updated !== undefined) c.updated = updated;
  return c;
}

interface EntrySpec<E extends Common> {
  kind: Kind;
  decode(o: unknown, what: string): E;
  /** Rules specific to this entry type. */
  check(e: E, problems: string[]): void;
}

/** Unicode `Cc`, like Rust `char::is_control`. */
const CTRL = /\p{Cc}/u;

/** A value passed to the CLI as one argument, without a shell. */
const isCliArg = (s: string) => s !== "" && !s.startsWith("-") && !/\s/.test(s) && !CTRL.test(s);

export const WIKI: EntrySpec<WikiEntry> = {
  kind: "wiki",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "uuid", "type", "target", "status"]);
    const e: WikiEntry = {
      ...common(o, what),
      uuid: str(o, "uuid", what),
      pageType: str(o, "type", what),
      status: oneOf(str(o, "status", what, true), ["active", "archived"], what) ?? "active",
    };
    const target = oneOf(str(o, "target", what, true), ["skill", "doc"], what);
    if (target) e.target = target;
    return e;
  },
  check(e, p) {
    if (!isUuid(e.uuid)) p.push(`\`${e.id}\`: \`uuid\` is not a UUID`);
    if (!isPageType(e.pageType)) p.push(`\`${e.id}\`: \`type\` must be slugs joined by \`/\``);
    if (e.target === "skill" && !isSlug(e.id)) {
      p.push(`\`${e.id}\`: a \`skill\` target needs a slug \`id\` (it becomes the directory name)`);
    }
  },
};

export const SKILLS: EntrySpec<SkillsEntry> = {
  kind: "skills",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "installer", "source", "skill", "install"]);
    const e: SkillsEntry = {
      ...common(o, what),
      installer: str(o, "installer", what),
      source: str(o, "source", what),
    };
    const skill = str(o, "skill", what, true);
    const install = str(o, "install", what, true);
    if (skill !== undefined) e.skill = skill;
    if (install !== undefined) e.install = install;
    return e;
  },
  check(e, p) {
    const id = e.id;
    const argRule = "must not be empty, start with `-`, or contain spaces or control characters";
    if (e.installer === "" || CTRL.test(e.installer)) p.push(`\`${id}\`: invalid \`installer\``);
    if (!isCliArg(e.source)) p.push(`\`${id}\`: \`source\` ${argRule}`);
    if (e.installer === SKILLS_CLI) {
      if (e.install !== undefined)
        p.push(`\`${id}\`: a \`${SKILLS_CLI}\` entry must not have \`install\``);
      if (e.skill !== undefined && !isCliArg(e.skill)) p.push(`\`${id}\`: \`skill\` ${argRule}`);
    } else {
      if (e.skill !== undefined)
        p.push(`\`${id}\`: only \`${SKILLS_CLI}\` entries may have \`skill\``);
      if (e.install === undefined || e.install.trim() === "") {
        p.push(`\`${id}\`: installer \`${e.installer}\` needs \`install\` text`);
      }
    }
  },
};

export const DOCS: EntrySpec<DocsEntry> = {
  kind: "docs",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "url", "filename", "sha256"]);
    const e: DocsEntry = {
      ...common(o, what),
      url: str(o, "url", what),
      filename: str(o, "filename", what),
    };
    const sha = str(o, "sha256", what, true);
    if (sha !== undefined) e.sha256 = sha;
    return e;
  },
  check(e, p) {
    const id = e.id;
    if (!/^https:\/\/.+/.test(e.url) || /\s/.test(e.url) || CTRL.test(e.url)) {
      p.push(`\`${id}\`: \`url\` must be an \`https://\` URL`);
    }
    const f = e.filename;
    if (f === "" || f === "." || f === ".." || /[/\\]/.test(f) || CTRL.test(f)) {
      p.push(`\`${id}\`: \`filename\` must be a plain file name without path separators`);
    }
    if (e.sha256 !== undefined && !/^[0-9a-f]{64}$/.test(e.sha256)) {
      p.push(`\`${id}\`: \`sha256\` must be 64 lowercase hex digits`);
    }
  },
};

/** `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`. */
// ponytail: shape only; field ranges (month 13, hour 25) are not checked.
export const isRfc3339 = (s: string) =>
  /^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/.test(s);

/**
 * Parses and validates one catalog file. `schema_version` is read first.
 * Throws an `Error` whose message lists every problem.
 */
export function parse<E extends Common>(spec: EntrySpec<E>, input: string): Parsed<E> {
  if (input.startsWith("﻿")) throw new Error("UTF-8 byte order mark is not allowed");
  let doc: unknown;
  try {
    doc = JSON.parse(input);
  } catch (e) {
    throw new Error(`invalid JSON: ${(e as Error).message}`);
  }
  const version = (doc as Obj | null)?.schema_version;
  if (typeof version !== "number" || !Number.isInteger(version) || version < 0) {
    throw new Error("`schema_version` (integer) is missing");
  }
  if (version !== SCHEMA_VERSION) throw new Error(`unsupported schema_version ${version}`);
  let file: Obj;
  let entries: E[];
  try {
    file = obj(doc, "catalog", ["schema_version", "kind", "catalog_version", "entries"]);
    const kind = str(file, "kind", "catalog");
    if (kind !== spec.kind) throw new Shape(`\`kind\` is \`${kind}\`, expected \`${spec.kind}\``);
    if (!Array.isArray(file.entries)) throw new Shape("catalog: `entries` must be an array");
    entries = file.entries.map((v, i) => spec.decode(v, `entries[${i}]`));
  } catch (e) {
    if (e instanceof Shape) throw new Error(e.message);
    throw e;
  }
  const problems: string[] = [];
  const seen = new Set<string>();
  for (const e of entries) {
    const id = e.id;
    if (id === "" || CTRL.test(id))
      problems.push(`invalid \`id\` \`${JSON.stringify(id).slice(1, -1)}\``);
    if (seen.has(id.toLowerCase())) problems.push(`duplicate \`id\` \`${id}\``);
    seen.add(id.toLowerCase());
    if (!validText(e.title))
      problems.push(`\`${id}\`: \`title\` needs at least one non-empty language`);
    if (e.summary && !validText(e.summary)) {
      problems.push(`\`${id}\`: \`summary\` needs at least one non-empty language`);
    }
    if (e.updated !== undefined && !isRfc3339(e.updated)) {
      problems.push(`\`${id}\`: \`updated\` is not an RFC 3339 time`);
    }
    spec.check(e, problems);
  }
  if (problems.length > 0) throw new Error(problems.join("; "));
  const catalogVersion = str(file, "catalog_version", "catalog", true);
  return catalogVersion === undefined ? { entries } : { catalogVersion, entries };
}

/** Built-in sample shipped in the package's `catalog/` directory. */
export const builtinText = (kind: Kind) =>
  fs.readFileSync(new URL(`../catalog/${kind}.json`, import.meta.url), "utf8");

/** Parses a built-in sample; throws with a `built-in sample:` prefix. */
export function loadBuiltin<E extends Common>(spec: EntrySpec<E>): Parsed<E> {
  try {
    return parse(spec, builtinText(spec.kind));
  } catch (e) {
    throw new Error(`built-in sample: ${(e as Error).message}`);
  }
}
