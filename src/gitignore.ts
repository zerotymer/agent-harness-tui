// `.gitignore` managed block and read-only Git checks (Stage 1 spec §6,
// instruction 0009).
//
// The block lists only harness paths; other rules in `.gitignore` are kept.
// Git is optional and only queried (`check-ignore`, `ls-files`): this module
// never changes the index, never commits and never runs `git init`.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import type { Desired } from "./apply.js";
import * as blocks from "./blocks.js";
import { CONFIG_FILE, configLayout, type HarnessConfig, LOCK_FILE } from "./contract.js";
import { guidanceFiles } from "./guidance.js";
import { completedDir, completedLog, type ProjectRoot, RelPath, skillsDir } from "./paths.js";

export const BLOCK_ID = "agent-harness";

/** Lines of the managed block. Patterns are anchored to the project root. */
export function ignoreBody(config: HarnessConfig, copies: RelPath[]): string {
  const db = config.backlog.path;
  const lines = [
    `/${config.paths.output}/`,
    "/.harness/",
    // Reserved even while the backlog is disabled.
    `/${db}`,
    `/${db}-wal`,
    `/${db}-shm`,
    `/${db}-journal`,
    ...copies.map((c) => `/${c}/`),
  ];
  return `${lines.join("\n")}\n`;
}

export const gitignorePath = () => RelPath.unchecked(".gitignore");

/** Apply-engine item for the `.gitignore` block. */
export function desired(root: ProjectRoot, config: HarnessConfig, copies: RelPath[]): Desired {
  return blocks.desired(root, gitignorePath(), "hash", BLOCK_ID, ignoreBody(config, copies));
}

/** Paths the harness expects Git to track. */
export function sharedPaths(config: HarnessConfig): RelPath[] {
  const layout = configLayout(config);
  const v = [
    completedDir(layout),
    completedLog(layout),
    layout.instructions,
    config.paths.references,
    skillsDir(),
    RelPath.unchecked(CONFIG_FILE),
    RelPath.unchecked(LOCK_FILE),
  ];
  for (const a of config.agents) {
    for (const [, p] of guidanceFiles(a.id)) if (!v.some((o) => o.equals(p))) v.push(p);
  }
  return v;
}

/** What Git says about the planned layout. Informational only. */
export interface GitReport {
  /** Set when Git checks were skipped. */
  unavailable?: string;
  /**
   * Shared paths that existing ignore rules (this repo, a parent directory
   * or the global excludes file) already exclude: "path: source".
   */
  excludedShared: string[];
  /** Paths to be ignored that Git already tracks. */
  trackedIgnored: string[];
  notes: string[];
}

/** `git` is overridable for tests. */
export function gitReport(
  root: ProjectRoot,
  config: HarnessConfig,
  copies: RelPath[],
  git = "git",
): GitReport {
  const r: GitReport = { excludedShared: [], trackedIgnored: [], notes: [] };
  const run = (...args: string[]) =>
    spawnSync(git, ["-C", root.path, ...args], { encoding: "utf8" });
  const probe = run("rev-parse", "--is-inside-work-tree");
  if (probe.error) {
    r.unavailable = `Git is not available (${probe.error.message}); Git checks skipped`;
    return r;
  }
  if (probe.status !== 0) {
    r.unavailable =
      "not inside a Git work tree; Git checks skipped (Git is never initialized automatically)";
    return r;
  }
  const lines = (s: string | null) => (s ?? "").split(/\r?\n/).filter((l) => l !== "");

  // Existing rules that would hide shared paths. `--no-index` also checks
  // paths that are already tracked.
  const shared = sharedPaths(config).map(String);
  for (const line of lines(run("check-ignore", "--no-index", "-v", "--", ...shared).stdout)) {
    // `<source>:<line>:<pattern>\t<path>`
    const tab = line.indexOf("\t");
    if (tab >= 0)
      r.excludedShared.push(`${line.slice(tab + 1)}: excluded by ${line.slice(0, tab)}`);
  }

  const ignored = [
    `${config.paths.output}`,
    ".harness",
    `${config.backlog.path}`,
    ...copies.map(String),
  ];
  r.trackedIgnored.push(...lines(run("ls-files", "--", ...ignored).stdout));
  if (r.trackedIgnored.length > 0) {
    r.notes.push(
      "`.gitignore` does not stop tracking files Git already tracks; " +
        "run `git rm --cached <path>` yourself if they should leave the index",
    );
  }

  for (const dir of [completedDir(configLayout(config)), config.paths.references]) {
    if (!hasFile(root.abs(dir))) {
      r.notes.push(`\`${dir}/\` has no files; Git does not share empty directories`);
    }
  }
  return r;
}

function hasFile(dir: string): boolean {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return false;
  }
  return entries.some((e) => !e.isDirectory() || hasFile(path.join(dir, e.name)));
}
