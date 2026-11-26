// Project configuration (Stage 2 spec §1, `HARNESS_CATALOG_CONTRACT`, v2).
//
// One file: `project.json` in the project root, or the built-in
// `catalog/default.json` when it does not exist. The two are never merged,
// and an invalid `project.json` never falls back to the default. This
// module only reads local files and never touches the network.

import fs from "node:fs";
import path from "node:path";
import { isEnvName, isPageType, isSlug, isUuid } from "./contract.js";
import { PathError, RelPath } from "./paths.js";

export const SCHEMA_VERSION = 2;
export const PROJECT_FILE = "project.json";
export const SKILLS_CLI = "skills-cli";

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

export type Os = "linux" | "macos" | "windows";

export interface AgentEntry extends Common {
  /** Argument vector; exit code 0 means installed. */
  detect: string[];
  /** Guidance file name in the project root, e.g. `CLAUDE.md`. */
  guidance: string;
  /** Project-relative directory where the agent discovers skills. */
  skillsDir: RelPath;
  install: Partial<Record<Os, string[]>>;
  default: boolean;
}

export interface SkillsEntry extends Common {
  installer: string;
  source: string;
  skill?: string;
  install?: string;
}

export interface WikiSkillEntry extends Common {
  uuid: string;
  pageType: string;
  status: "active" | "archived";
}

export interface DocsEntry extends Common {
  url: string;
  filename: string;
  sha256?: string;
}

export interface HookEntry extends Common {
  url?: string;
}

/** Initial values for the paths screen (instruction 0024). `null` = disabled. */
export interface PathsInit {
  instructions?: string;
  artifacts?: string | null;
  docs?: string | null;
  references?: string | null;
}

/** Initial content-service settings. */
export interface ServiceInit {
  url?: string;
  project?: string;
  tokenEnv?: string;
}

export interface ProjectConfig {
  catalogVersion?: string;
  paths?: PathsInit;
  service?: ServiceInit;
  agents: AgentEntry[];
  skills: SkillsEntry[];
  wikiSkills: WikiSkillEntry[];
  docs: DocsEntry[];
  hooks: HookEntry[];
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

/** Argument vector: array of strings (rules checked later). */
function argv(v: unknown, what: string): string[] {
  if (!Array.isArray(v) || v.some((a) => typeof a !== "string")) {
    throw new Shape(`${what}: expected an array of strings`);
  }
  return v as string[];
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
  key: string;
  decode(o: unknown, what: string): E;
  /** Rules specific to this entry type. */
  check(e: E, problems: string[]): void;
}

/** Unicode `Cc`. */
const CTRL = /\p{Cc}/u;

/** A value passed to a program as one argument, without a shell. */
const isCliArg = (s: string) => s !== "" && !s.startsWith("-") && !/\s/.test(s) && !CTRL.test(s);

/** Problem with an argument vector, or undefined. */
export function argvProblem(a: string[]): string | undefined {
  if (a.length === 0) return "is empty";
  if (a.some((x) => x === "" || CTRL.test(x))) return "has an empty element or a control character";
  if (a[0]?.startsWith("-")) return "starts with an option instead of a program";
  return undefined;
}

const OSES: readonly Os[] = ["linux", "macos", "windows"];

const AGENTS: EntrySpec<AgentEntry> = {
  key: "agents",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "detect", "guidance", "skills_dir", "install", "default"]);
    const install: Partial<Record<Os, string[]>> = {};
    if (o.install !== undefined && o.install !== null) {
      const i = obj(o.install, `${what}.install`, OSES);
      for (const os of OSES) {
        if (i[os] !== undefined) install[os] = argv(i[os], `${what}.install.${os}`);
      }
    }
    if (o.default !== undefined && typeof o.default !== "boolean") {
      throw new Shape(`${what}: \`default\` must be a boolean`);
    }
    const dir = str(o, "skills_dir", what);
    let skillsDir: RelPath;
    try {
      skillsDir = RelPath.parse(dir);
    } catch (e) {
      if (!(e instanceof PathError)) throw e;
      throw new Shape(`${what}: \`skills_dir\` ${e.message}`);
    }
    return {
      ...common(o, what),
      detect: argv(o.detect, `${what}.detect`),
      guidance: str(o, "guidance", what),
      skillsDir,
      install,
      default: o.default === true,
    };
  },
  check(e, p) {
    const id = e.id;
    if (!isSlug(id)) p.push(`\`${id}\`: agent \`id\` must be a slug`);
    const d = argvProblem(e.detect);
    if (d) p.push(`\`${id}\`: \`detect\` ${d}`);
    for (const os of OSES) {
      const a = e.install[os];
      const problem = a && argvProblem(a);
      if (problem) p.push(`\`${id}\`: \`install.${os}\` ${problem}`);
    }
    const g = e.guidance;
    if (!g.endsWith(".md") || /[/\\]/.test(g) || CTRL.test(g) || g.startsWith(".")) {
      p.push(`\`${id}\`: \`guidance\` must be a file name ending in \`.md\``);
    }
  },
};

const SKILLS: EntrySpec<SkillsEntry> = {
  key: "skills",
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

const WIKI_SKILLS: EntrySpec<WikiSkillEntry> = {
  key: "wiki_skills",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "uuid", "type", "status"]);
    return {
      ...common(o, what),
      uuid: str(o, "uuid", what),
      pageType: str(o, "type", what),
      status: oneOf(str(o, "status", what, true), ["active", "archived"], what) ?? "active",
    };
  },
  check(e, p) {
    if (!isSlug(e.id)) p.push(`\`${e.id}\`: wiki skill \`id\` must be a slug (directory name)`);
    if (!isUuid(e.uuid)) p.push(`\`${e.id}\`: \`uuid\` is not a UUID`);
    if (!isPageType(e.pageType)) p.push(`\`${e.id}\`: \`type\` must be slugs joined by \`/\``);
  },
};

const isHttps = (u: string) => /^https:\/\/.+/.test(u) && !/\s/.test(u) && !CTRL.test(u);

const DOCS: EntrySpec<DocsEntry> = {
  key: "docs",
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
    if (!isHttps(e.url)) p.push(`\`${id}\`: \`url\` must be an \`https://\` URL`);
    const f = e.filename;
    if (f === "" || f === "." || f === ".." || /[/\\]/.test(f) || CTRL.test(f)) {
      p.push(`\`${id}\`: \`filename\` must be a plain file name without path separators`);
    }
    if (e.sha256 !== undefined && !/^[0-9a-f]{64}$/.test(e.sha256)) {
      p.push(`\`${id}\`: \`sha256\` must be 64 lowercase hex digits`);
    }
  },
};

const HOOKS: EntrySpec<HookEntry> = {
  key: "hooks",
  decode(v, what) {
    const o = obj(v, what, [...COMMON, "url"]);
    const e: HookEntry = common(o, what);
    const url = str(o, "url", what, true);
    if (url !== undefined) e.url = url;
    return e;
  },
  check(e, p) {
    if (e.url !== undefined && !isHttps(e.url)) {
      p.push(`\`${e.id}\`: \`url\` must be an \`https://\` URL`);
    }
  },
};

/** `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`. */
// ponytail: shape only; field ranges (month 13, hour 25) are not checked.
export const isRfc3339 = (s: string) =>
  /^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/.test(s);

function list<E extends Common>(file: Obj, spec: EntrySpec<E>, problems: string[]): E[] {
  const v = file[spec.key];
  if (v === undefined) return [];
  if (!Array.isArray(v)) throw new Shape(`\`${spec.key}\` must be an array`);
  const entries = v.map((e, i) => spec.decode(e, `${spec.key}[${i}]`));
  const seen = new Set<string>();
  for (const e of entries) {
    const id = e.id;
    const where = `${spec.key}: \`${id}\``;
    if (id === "" || CTRL.test(id))
      problems.push(`${spec.key}: invalid \`id\` \`${JSON.stringify(id).slice(1, -1)}\``);
    if (seen.has(id.toLowerCase())) problems.push(`${spec.key}: duplicate \`id\` \`${id}\``);
    seen.add(id.toLowerCase());
    if (!validText(e.title)) problems.push(`${where}: \`title\` needs a non-empty language`);
    if (e.summary && !validText(e.summary)) {
      problems.push(`${where}: \`summary\` needs a non-empty language`);
    }
    if (e.updated !== undefined && !isRfc3339(e.updated)) {
      problems.push(`${where}: \`updated\` is not an RFC 3339 time`);
    }
    const own: string[] = [];
    spec.check(e, own);
    problems.push(...own.map((m) => `${spec.key}: ${m}`));
  }
  return entries;
}

function pathsInit(v: unknown): PathsInit {
  const o = obj(v, "paths", ["instructions", "artifacts", "docs", "references"]);
  const out: PathsInit = {};
  if (o.instructions !== undefined) {
    if (typeof o.instructions !== "string")
      throw new Shape("paths: `instructions` must be a string");
    out.instructions = o.instructions;
  }
  for (const k of ["artifacts", "docs", "references"] as const) {
    const x = o[k];
    if (x === undefined) continue;
    if (x !== null && typeof x !== "string")
      throw new Shape(`paths: \`${k}\` must be a string or null`);
    out[k] = x;
  }
  return out;
}

function serviceInit(v: unknown): ServiceInit {
  const o = obj(v, "service", ["url", "project", "token_env"]);
  const out: ServiceInit = {};
  const url = str(o, "url", "service", true);
  const project = str(o, "project", "service", true);
  const tokenEnv = str(o, "token_env", "service", true);
  if (url !== undefined) out.url = url;
  if (project !== undefined) out.project = project;
  if (tokenEnv !== undefined) {
    if (!isEnvName(tokenEnv)) throw new Shape("service: `token_env` must match `[A-Z_][A-Z0-9_]*`");
    out.tokenEnv = tokenEnv;
  }
  return out;
}

/**
 * Parses and validates one configuration file. `schema_version` is read
 * first. Throws an `Error` whose message lists every problem.
 */
export function parse(input: string): ProjectConfig {
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
  const problems: string[] = [];
  let out: ProjectConfig;
  try {
    const file = obj(doc, "configuration", [
      "schema_version",
      "catalog_version",
      "paths",
      "service",
      "agents",
      "skills",
      "wiki_skills",
      "docs",
      "hooks",
    ]);
    out = {
      agents: list(file, AGENTS, problems),
      skills: list(file, SKILLS, problems),
      wikiSkills: list(file, WIKI_SKILLS, problems),
      docs: list(file, DOCS, problems),
      hooks: list(file, HOOKS, problems),
    };
    const catalogVersion = str(file, "catalog_version", "configuration", true);
    if (catalogVersion !== undefined) out.catalogVersion = catalogVersion;
    if (file.paths !== undefined) out.paths = pathsInit(file.paths);
    if (file.service !== undefined) out.service = serviceInit(file.service);
  } catch (e) {
    if (e instanceof Shape) throw new Error(e.message);
    throw e;
  }
  if (problems.length > 0) throw new Error(problems.join("; "));
  return out;
}

export type Source = "project" | "builtin";

export interface Loaded {
  source: Source;
  config: ProjectConfig;
}

/** The built-in `default.json` shipped in the package's `catalog/` directory. */
export const builtinText = () =>
  fs.readFileSync(new URL("../catalog/default.json", import.meta.url), "utf8");

/**
 * `project.json` when it exists, otherwise the built-in default. Never
 * merges and never falls back. Throws with the file name as a prefix.
 */
export function load(root: string): Loaded {
  let text: string | undefined;
  try {
    text = fs.readFileSync(path.join(root, PROJECT_FILE), "utf8");
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== "ENOENT") {
      throw new Error(`${PROJECT_FILE}: ${(e as NodeJS.ErrnoException).code ?? "read failed"}`);
    }
  }
  const source: Source = text === undefined ? "builtin" : "project";
  try {
    return { source, config: parse(text ?? builtinText()) };
  } catch (e) {
    const name = source === "project" ? PROJECT_FILE : "built-in default.json";
    throw new Error(`${name}: ${(e as Error).message}`);
  }
}

/** This OS as named in `install`, or undefined for other platforms. */
export function currentOs(platform: NodeJS.Platform = process.platform): Os | undefined {
  return platform === "linux"
    ? "linux"
    : platform === "darwin"
      ? "macos"
      : platform === "win32"
        ? "windows"
        : undefined;
}

/** Display name of an agent. */
export const agentTitle = (a: AgentEntry) => display(a.title) || a.id;

let builtin: AgentEntry[] | undefined;
/** Agents of the built-in default (cached); the fallback where no list is passed. */
export function builtinAgents(): AgentEntry[] {
  builtin ??= parse(builtinText()).agents;
  return builtin;
}

/**
 * Configured agents that the agent list knows, with their entries. First
 * entry wins for duplicates. Unknown IDs are skipped: they stay in
 * `harness.toml` but nothing is deployed for them.
 */
export function knownAgents<C extends { id: string }>(
  configured: C[],
  agents: AgentEntry[],
): [C, AgentEntry][] {
  const out: [C, AgentEntry][] = [];
  for (const c of configured) {
    if (out.some(([o]) => o.id === c.id)) continue;
    const e = agents.find((a) => a.id === c.id);
    if (e) out.push([c, e]);
  }
  return out;
}
