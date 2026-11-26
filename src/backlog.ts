// Optional `agent.db` backlog (instruction 0017, issue #21).
//
// When enabled, the instruction root gets a fresh SQLite database plus
// bash and PowerShell management scripts. All three are create-only: an
// existing database or edited script is preserved. The schema text is the
// single source for the database and both scripts.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createOnly, type Desired } from "./apply.js";
import type { HarnessConfig } from "./contract.js";

const template = (name: string) =>
  fs.readFileSync(new URL(`../templates/${name}`, import.meta.url), "utf8");

export const SCHEMA = template("agent-db.schema.sql");
export const SH_FILE = "agent-db.sh";
export const PS1_FILE = "agent-db.ps1";

/** Script text with the schema inlined. */
export const script = (name: string) => template(name).replace("@SCHEMA@", () => SCHEMA.trimEnd());

/** Files to create when the backlog is enabled; nothing otherwise. */
export async function planItems(config: HarnessConfig): Promise<Desired[]> {
  if (!config.backlog.enabled) return [];
  const inst = config.paths.instructions;
  return [
    createOnly(config.backlog.path, await emptyDb()),
    createOnly(inst.join(SH_FILE), script(SH_FILE)),
    // ponytail: LF endings; PowerShell 5.1+ reads them fine.
    createOnly(inst.join(PS1_FILE), script(PS1_FILE)),
  ];
}

/**
 * Bytes of a new database with the schema applied. Built in a temporary
 * file (WAL needs a real file) so nothing touches the project before apply.
 */
export async function emptyDb(): Promise<Buffer> {
  // Imported lazily: node:sqlite prints an ExperimentalWarning on load,
  // so only runs that enable the backlog see it.
  const { DatabaseSync } = await import("node:sqlite");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "agent-harness-db-"));
  try {
    const file = path.join(dir, "agent.db");
    const db = new DatabaseSync(file);
    try {
      db.exec(SCHEMA);
    } finally {
      // Leaves the main file complete; WAL mode stays in its header.
      db.close();
    }
    return fs.readFileSync(file);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}
