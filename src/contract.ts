// Versioned shared config, lock and local state (Stage 1 spec §3,
// instruction 0003). See `references/HARNESS_STATE_CONTRACT.md`.
//
// This module only parses, validates and serializes. Writing files with
// backups and recovery is the apply engine's job (instruction 0004); it must
// load first, so a file with an unsupported version is never overwritten.

import fs from "node:fs";
import path from "node:path";
import { parse as parseToml, stringify as stringifyToml } from "smol-toml";
import { BACKLOG_FILE, backlogPath, defaultLayout, type Layout, RelPath } from "./paths.js";

export const SCHEMA_VERSION = 1;
export const CONFIG_FILE = "harness.toml";
export const LOCK_FILE = "harness.lock.json";
export const STATE_FILE = ".harness/state.json";
export const BACKUPS_DIR = ".harness/backups";
/** The only wiki gateway (`wiki-skill-import` contract); not configurable. */
export const WIKI_GATEWAY_URL = "https://md-gw.zerotymer.net";
export const DEFAULT_TOKEN_ENV = "GATEWAY_TOKEN";

export type ContractErrorKind = "io" | "parse" | "unsupported-version" | "invalid";

export class ContractError extends Error {
  constructor(
    readonly kind: ContractErrorKind,
    readonly file: string,
    message: string,
    readonly found?: number,
    readonly problems: string[] = [],
  ) {
    super(message);
  }
}

/** A field-level decode failure; reported as a parse error. */
class DecodeError extends Error {}

// ---------------------------------------------------------------- decoding

type Obj = Record<string, unknown>;

/** A plain table with only the allowed keys (unknown keys are refused). */
function table(v: unknown, at: string, keys: readonly string[]): Obj {
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new DecodeError(`${at}: expected a table`);
  }
  for (const k of Object.keys(v)) {
    if (!keys.includes(k)) throw new DecodeError(`${at}: unknown field \`${k}\``);
  }
  return v as Obj;
}

function opt<T>(
  o: Obj,
  key: string,
  at: string,
  read: (v: unknown, at: string) => T,
): T | undefined {
  return o[key] === undefined ? undefined : read(o[key], `${at}.${key}`);
}

function req<T>(o: Obj, key: string, at: string, read: (v: unknown, at: string) => T): T {
  if (o[key] === undefined) throw new DecodeError(`${at}: missing field \`${key}\``);
  return read(o[key], `${at}.${key}`);
}

function str(v: unknown, at: string): string {
  if (typeof v !== "string") throw new DecodeError(`${at}: expected a string`);
  return v;
}

function bool(v: unknown, at: string): boolean {
  if (typeof v !== "boolean") throw new DecodeError(`${at}: expected a boolean`);
  return v;
}

/** Unsigned 32-bit integer, like the Rust `u32` it replaces. */
function u32(v: unknown, at: string): number {
  // ponytail: smol-toml reads `1.0` as 1, so a float with no fraction passes.
  if (typeof v !== "number" || !Number.isInteger(v) || v < 0 || v > 0xffffffff) {
    throw new DecodeError(`${at}: expected an unsigned integer`);
  }
  return v;
}

function rel(v: unknown, at: string): RelPath {
  try {
    return RelPath.parse(str(v, at));
  } catch (e) {
    throw new DecodeError(`${at}: ${(e as Error).message}`);
  }
}

function oneOf<T extends string>(values: readonly T[]) {
  return (v: unknown, at: string): T => {
    if (!values.includes(v as T)) {
      throw new DecodeError(`${at}: expected one of ${values.join(", ")}`);
    }
    return v as T;
  };
}

function list<T>(read: (v: unknown, at: string) => T) {
  return (v: unknown, at: string): T[] => {
    if (!Array.isArray(v)) throw new DecodeError(`${at}: expected an array`);
    return v.map((item, i) => read(item, `${at}[${i}]`));
  };
}

/** Drops `undefined` values so they are not written. */
function compact<T extends object>(o: T): T {
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as T;
}

// ---------------------------------------------------------- contract files

interface ContractSpec<T> {
  file: string;
  format: "toml" | "json";
  decode(doc: Obj): T;
  encode(value: T): Obj;
  problems(value: T): string[];
}

/** Parses, checks the version first, then validates. */
export function fromText<T>(spec: ContractSpec<T>, text: string): T {
  const fail = (message: string) =>
    new ContractError("parse", spec.file, `${spec.file}: ${message}`);
  let doc: unknown;
  try {
    doc =
      spec.format === "toml" ? parseToml(text, { unsafeKeyBehaviour: "throw" }) : JSON.parse(text);
  } catch (e) {
    throw fail((e as Error).message);
  }
  if (typeof doc !== "object" || doc === null || Array.isArray(doc)) {
    throw fail("expected a table at the top level");
  }
  const raw = (doc as Obj).schema_version;
  if (raw === undefined) throw fail("missing field `schema_version`");
  let found: number;
  try {
    found = u32(raw, "schema_version");
  } catch (e) {
    throw fail((e as Error).message);
  }
  if (found !== SCHEMA_VERSION) {
    throw new ContractError(
      "unsupported-version",
      spec.file,
      `${spec.file}: schema_version ${found} is not supported (expected ${SCHEMA_VERSION}); ` +
        "the file is left unchanged",
      found,
    );
  }
  let value: T;
  try {
    value = spec.decode(doc as Obj);
  } catch (e) {
    if (e instanceof DecodeError) throw fail(e.message);
    throw e;
  }
  validate(spec, value);
  return value;
}

export function validate<T>(spec: ContractSpec<T>, value: T): void {
  const problems = spec.problems(value);
  if (problems.length > 0) {
    throw new ContractError(
      "invalid",
      spec.file,
      `${spec.file}: ${problems.join("; ")}`,
      undefined,
      problems,
    );
  }
}

/** Validates, then serializes. */
export function toText<T>(spec: ContractSpec<T>, value: T): string {
  validate(spec, value);
  const doc = spec.encode(value);
  return spec.format === "toml" ? stringifyToml(doc) : `${JSON.stringify(doc, null, 2)}\n`;
}

/** `undefined` when the file does not exist. */
export function load<T>(spec: ContractSpec<T>, root: string): T | undefined {
  let text: string;
  try {
    text = fs.readFileSync(path.join(root, spec.file), "utf8");
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw new ContractError("io", spec.file, (e as Error).message);
  }
  return fromText(spec, text);
}

// ------------------------------------------------------------ harness.toml

export const AGENT_IDS = ["claude-code", "codex"] as const;
export type AgentId = (typeof AGENT_IDS)[number];
export type Placement = "copy" | "reference";
export type SourceKind = "local-dir" | "wiki-gateway";

export interface PathsConfig {
  /** Absent in configs written before it became configurable. */
  instructions: RelPath;
  references: RelPath;
  output: RelPath;
}

export interface BacklogConfig {
  enabled: boolean;
  path: RelPath;
}

export interface AgentConfig {
  id: AgentId;
  placement: Placement;
}

export interface SourceConfig {
  id: string;
  kind: SourceKind;
  /** `local-dir` only: project-relative directory. */
  path?: RelPath;
  /** `wiki-gateway` only: name of the environment variable, never a value. */
  tokenEnv?: string;
}

export interface SkillConfig {
  /** Stable ID; also the directory name under `.skills/`. */
  id: string;
  source: string;
  /** Wiki page UUID, or a directory relative to a local source. */
  reference: string;
  /** Wiki path prefix such as `skill`; only for `wiki-gateway`. */
  pageType?: string;
}

export interface HarnessConfig {
  schemaVersion: number;
  paths: PathsConfig;
  backlog: BacklogConfig;
  agents: AgentConfig[];
  sources: SourceConfig[];
  skills: SkillConfig[];
}

export function configLayout(c: HarnessConfig): Layout {
  return { ...c.paths, backlogEnabled: c.backlog.enabled };
}

export function defaultConfig(): HarnessConfig {
  const l = defaultLayout();
  return {
    schemaVersion: SCHEMA_VERSION,
    paths: { instructions: l.instructions, references: l.references, output: l.output },
    backlog: { enabled: false, path: backlogPath(l) },
    agents: [],
    sources: [],
    skills: [],
  };
}

const placement = oneOf<Placement>(["copy", "reference"]);
const agentId = oneOf<AgentId>(AGENT_IDS);

export const HARNESS_CONFIG: ContractSpec<HarnessConfig> = {
  file: CONFIG_FILE,
  format: "toml",

  decode(doc) {
    const o = table(doc, "root", [
      "schema_version",
      "paths",
      "backlog",
      "agents",
      "sources",
      "skills",
    ]);
    const paths = table(
      req(o, "paths", "root", (v) => v),
      "paths",
      ["instructions", "references", "output"],
    );
    const backlog = table(
      req(o, "backlog", "root", (v) => v),
      "backlog",
      ["enabled", "path"],
    );
    return {
      schemaVersion: req(o, "schema_version", "root", u32),
      paths: {
        instructions: opt(paths, "instructions", "paths", rel) ?? defaultLayout().instructions,
        references: req(paths, "references", "paths", rel),
        output: req(paths, "output", "paths", rel),
      },
      backlog: {
        enabled: req(backlog, "enabled", "backlog", bool),
        path: req(backlog, "path", "backlog", rel),
      },
      agents:
        opt(
          o,
          "agents",
          "root",
          list((v, at) => {
            const a = table(v, at, ["id", "placement"]);
            return { id: req(a, "id", at, agentId), placement: req(a, "placement", at, placement) };
          }),
        ) ?? [],
      sources:
        opt(
          o,
          "sources",
          "root",
          list((v, at) => {
            const s = table(v, at, ["id", "kind", "path", "token_env"]);
            return compact({
              id: req(s, "id", at, str),
              kind: req(s, "kind", at, oneOf<SourceKind>(["local-dir", "wiki-gateway"])),
              path: opt(s, "path", at, rel),
              tokenEnv: opt(s, "token_env", at, str),
            });
          }),
        ) ?? [],
      skills:
        opt(
          o,
          "skills",
          "root",
          list((v, at) => {
            const k = table(v, at, ["id", "source", "ref", "type"]);
            return compact({
              id: req(k, "id", at, str),
              source: req(k, "source", at, str),
              reference: req(k, "ref", at, str),
              pageType: opt(k, "type", at, str),
            });
          }),
        ) ?? [],
    };
  },

  encode(c) {
    return {
      schema_version: c.schemaVersion,
      paths: {
        instructions: c.paths.instructions.toString(),
        references: c.paths.references.toString(),
        output: c.paths.output.toString(),
      },
      backlog: { enabled: c.backlog.enabled, path: c.backlog.path.toString() },
      agents: c.agents.map((a) => ({ id: a.id, placement: a.placement })),
      sources: c.sources.map((s) =>
        compact({ id: s.id, kind: s.kind, path: s.path?.toString(), token_env: s.tokenEnv }),
      ),
      skills: c.skills.map((k) =>
        compact({ id: k.id, source: k.source, ref: k.reference, type: k.pageType }),
      ),
    };
  },

  problems(c) {
    const p: string[] = [];
    // `agent.db` at the root is what v0.1.0 wrote; it never created a file.
    const expected = backlogPath(configLayout(c));
    if (!c.backlog.path.equals(expected) && c.backlog.path.toString() !== BACKLOG_FILE) {
      p.push(`backlog.path must be \`${expected}\`, got \`${c.backlog.path}\``);
    }
    unique(
      p,
      "agent",
      c.agents.map((a) => a.id),
    );
    unique(
      p,
      "source",
      c.sources.map((s) => s.id),
    );
    unique(
      p,
      "skill",
      c.skills.map((s) => s.id),
    );

    for (const s of c.sources) {
      if (!isSlug(s.id)) p.push(`source id \`${s.id}\` must be a slug`);
      if (s.kind === "local-dir") {
        if (s.path === undefined) p.push(`source \`${s.id}\`: local-dir needs \`path\``);
        if (s.tokenEnv !== undefined)
          p.push(`source \`${s.id}\`: local-dir takes no \`token_env\``);
      } else {
        if (s.path !== undefined) p.push(`source \`${s.id}\`: wiki-gateway takes no \`path\``);
        if (s.tokenEnv === undefined) {
          p.push(`source \`${s.id}\`: wiki-gateway needs \`token_env\``);
        } else if (!isEnvName(s.tokenEnv)) {
          p.push(
            `source \`${s.id}\`: token_env \`${s.tokenEnv}\` must be an environment variable ` +
              "name (A-Z, 0-9, _), never a token value",
          );
        }
      }
    }

    for (const k of c.skills) {
      if (!isSlug(k.id)) p.push(`skill id \`${k.id}\` must be a slug`);
      const src = c.sources.find((s) => s.id === k.source);
      if (!src) {
        p.push(`skill \`${k.id}\`: unknown source \`${k.source}\``);
        continue;
      }
      if (src.kind === "wiki-gateway") {
        if (!isUuid(k.reference)) p.push(`skill \`${k.id}\`: ref must be a page UUID`);
        if (k.pageType !== undefined && !isPageType(k.pageType)) {
          p.push(`skill \`${k.id}\`: type must be slugs joined by \`/\``);
        }
      } else {
        try {
          RelPath.parse(k.reference);
        } catch (e) {
          p.push(`skill \`${k.id}\`: ref ${(e as Error).message}`);
        }
        if (k.pageType !== undefined) p.push(`skill \`${k.id}\`: type is only for wiki-gateway`);
      }
    }
    return p;
  },
};

// ------------------------------------------------------- harness.lock.json

export interface LockedFile {
  /** Relative to `.skills/<id>/`. */
  path: RelPath;
  /** `sha256:<hex>` of the bytes written locally. */
  hash: string;
  locale?: string;
  /** Revision reported by the source (gateway `contentHash`), verbatim. */
  revision?: string;
}

export interface LockedSkill {
  id: string;
  source: string;
  reference: string;
  pageType?: string;
  files: LockedFile[];
}

export interface HarnessLock {
  schemaVersion: number;
  skills: LockedSkill[];
}

export function defaultLock(): HarnessLock {
  return { schemaVersion: SCHEMA_VERSION, skills: [] };
}

export const HARNESS_LOCK: ContractSpec<HarnessLock> = {
  file: LOCK_FILE,
  format: "json",

  decode(doc) {
    const o = table(doc, "root", ["schema_version", "skills"]);
    return {
      schemaVersion: req(o, "schema_version", "root", u32),
      skills:
        opt(
          o,
          "skills",
          "root",
          list((v, at) => {
            const s = table(v, at, ["id", "source", "ref", "type", "files"]);
            return compact({
              id: req(s, "id", at, str),
              source: req(s, "source", at, str),
              reference: req(s, "ref", at, str),
              pageType: opt(s, "type", at, str),
              files: req(
                s,
                "files",
                at,
                list((fv, fat) => {
                  const f = table(fv, fat, ["path", "hash", "locale", "revision"]);
                  return compact({
                    path: req(f, "path", fat, rel),
                    hash: req(f, "hash", fat, str),
                    locale: opt(f, "locale", fat, str),
                    revision: opt(f, "revision", fat, str),
                  });
                }),
              ),
            });
          }),
        ) ?? [],
    };
  },

  encode(l) {
    return {
      schema_version: l.schemaVersion,
      skills: l.skills.map((s) =>
        compact({
          id: s.id,
          source: s.source,
          ref: s.reference,
          type: s.pageType,
          files: s.files.map((f) =>
            compact({
              path: f.path.toString(),
              hash: f.hash,
              locale: f.locale,
              revision: f.revision,
            }),
          ),
        }),
      ),
    };
  },

  problems(l) {
    const p: string[] = [];
    unique(
      p,
      "skill",
      l.skills.map((s) => s.id),
    );
    for (const s of l.skills) {
      if (!isSlug(s.id)) p.push(`skill id \`${s.id}\` must be a slug`);
      if (s.files.length === 0) p.push(`skill \`${s.id}\` lists no files`);
      unique(
        p,
        "file",
        s.files.map((f) => `${s.id}/${f.path}`),
      );
      for (const f of s.files) {
        if (!isSha256(f.hash)) {
          p.push(`skill \`${s.id}\` file \`${f.path}\`: hash must be sha256:<64 hex>`);
        }
      }
    }
    return p;
  },
};

// ----------------------------------------------------- .harness/state.json

export interface AppliedRecord {
  /** Also the backup directory name under `.harness/backups/`. */
  id: string;
  /** RFC 3339 timestamp with offset. */
  at: string;
  toolVersion: string;
}

/** Ownership is per file (`block` undefined) or per managed block. */
export interface ManagedEntry {
  path: RelPath;
  block?: string;
  /** Baseline: `sha256:<hex>` of the owned file or block content. */
  hash: string;
}

export interface Deployment {
  skill: string;
  agent: AgentId;
  placement: Placement;
  target: RelPath;
  /** Copies only. */
  hash?: string;
}

export interface AdapterRecord {
  agent: AgentId;
  /** Adapter contract version this tool implements for the agent. */
  version: string;
}

export interface HarnessState {
  schemaVersion: number;
  /** Only successful applies are recorded. */
  lastSuccess?: AppliedRecord;
  managed: ManagedEntry[];
  deployments: Deployment[];
  adapters: AdapterRecord[];
}

export function defaultState(): HarnessState {
  return { schemaVersion: SCHEMA_VERSION, managed: [], deployments: [], adapters: [] };
}

export const HARNESS_STATE: ContractSpec<HarnessState> = {
  file: STATE_FILE,
  format: "json",

  decode(doc) {
    const o = table(doc, "root", [
      "schema_version",
      "last_success",
      "managed",
      "deployments",
      "adapters",
    ]);
    return compact({
      schemaVersion: req(o, "schema_version", "root", u32),
      lastSuccess: opt(o, "last_success", "root", (v, at) => {
        const r = table(v, at, ["id", "at", "tool_version"]);
        return {
          id: req(r, "id", at, str),
          at: req(r, "at", at, str),
          toolVersion: req(r, "tool_version", at, str),
        };
      }),
      managed:
        opt(
          o,
          "managed",
          "root",
          list((v, at) => {
            const m = table(v, at, ["path", "block", "hash"]);
            return compact({
              path: req(m, "path", at, rel),
              block: opt(m, "block", at, str),
              hash: req(m, "hash", at, str),
            });
          }),
        ) ?? [],
      deployments:
        opt(
          o,
          "deployments",
          "root",
          list((v, at) => {
            const d = table(v, at, ["skill", "agent", "placement", "target", "hash"]);
            return compact({
              skill: req(d, "skill", at, str),
              agent: req(d, "agent", at, agentId),
              placement: req(d, "placement", at, placement),
              target: req(d, "target", at, rel),
              hash: opt(d, "hash", at, str),
            });
          }),
        ) ?? [],
      adapters:
        opt(
          o,
          "adapters",
          "root",
          list((v, at) => {
            const a = table(v, at, ["agent", "version"]);
            return { agent: req(a, "agent", at, agentId), version: req(a, "version", at, str) };
          }),
        ) ?? [],
    });
  },

  encode(s) {
    return compact({
      schema_version: s.schemaVersion,
      last_success: s.lastSuccess && {
        id: s.lastSuccess.id,
        at: s.lastSuccess.at,
        tool_version: s.lastSuccess.toolVersion,
      },
      managed: s.managed.map((m) =>
        compact({ path: m.path.toString(), block: m.block, hash: m.hash }),
      ),
      deployments: s.deployments.map((d) =>
        compact({
          skill: d.skill,
          agent: d.agent,
          placement: d.placement,
          target: d.target.toString(),
          hash: d.hash,
        }),
      ),
      adapters: s.adapters.map((a) => ({ agent: a.agent, version: a.version })),
    });
  },

  problems(s) {
    const p: string[] = [];
    if (s.lastSuccess && !isSlug(s.lastSuccess.id)) {
      p.push(`last_success.id \`${s.lastSuccess.id}\` must be a slug`);
    }
    unique(
      p,
      "managed entry",
      s.managed.map((m) => `${m.path}#${m.block ?? "*"}`),
    );
    for (const m of s.managed) {
      if (!isSha256(m.hash)) p.push(`managed \`${m.path}\`: hash must be sha256:<64 hex>`);
      if (
        m.block !== undefined &&
        s.managed.some((o) => o.path.equals(m.path) && o.block === undefined)
      ) {
        p.push(`\`${m.path}\` is owned both as a whole file and by block \`${m.block}\``);
      }
    }
    unique(
      p,
      "deployment target",
      s.deployments.map((d) => d.target.toString()),
    );
    for (const d of s.deployments) {
      if (d.placement === "copy" && (d.hash === undefined || !isSha256(d.hash))) {
        p.push(`deployment \`${d.target}\`: copy needs a sha256 hash`);
      } else if (d.placement === "reference" && d.hash !== undefined) {
        p.push(`deployment \`${d.target}\`: reference takes no hash`);
      }
    }
    unique(
      p,
      "adapter",
      s.adapters.map((a) => a.agent),
    );
    return p;
  },
};

/** `.harness/backups/<apply-id>/<rel>`: the pre-apply copy of `rel`. */
export function backupPath(applyId: string, rel: RelPath): RelPath | undefined {
  if (!isSlug(applyId)) return undefined;
  try {
    return RelPath.parse(`${BACKUPS_DIR}/${applyId}/${rel}`);
  } catch {
    return undefined;
  }
}

// ----------------------------------------------------------------- helpers

/** Reports case-insensitive duplicates. */
function unique(problems: string[], what: string, ids: string[]): void {
  const seen = new Set<string>();
  for (const id of ids) {
    const key = id.toLowerCase();
    if (seen.has(key)) problems.push(`duplicate ${what} \`${id}\``);
    seen.add(key);
  }
}

/** `[A-Za-z0-9._-]+`, not starting with `.` or `-`. Safe as a directory name. */
export function isSlug(s: string): boolean {
  return /^[A-Za-z0-9_][A-Za-z0-9._-]*$/.test(s);
}

/** A wiki path type such as `skill` or `claude/hook`: slugs joined by `/`. */
export function isPageType(s: string): boolean {
  return s.split("/").every(isSlug);
}

/** `[A-Z_][A-Z0-9_]*`. */
export function isEnvName(s: string): boolean {
  return /^[A-Z_][A-Z0-9_]*$/.test(s);
}

/** Hyphenated hex UUID, any version. */
export function isUuid(s: string): boolean {
  return /^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$/.test(s);
}

/** `sha256:` followed by 64 lowercase hex digits. */
export function isSha256(s: string): boolean {
  return /^sha256:[0-9a-f]{64}$/.test(s);
}
