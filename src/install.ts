// Public skill install through the skills CLI (Stage 2 spec §3).
//
// The TUI only decides *what* to install: `npx skills add <source>
// [-s <skill>]`. Agents, scope and skill selection are asked by the CLI
// itself, so no `-a`, `-g` or `-y` is added. The command runs without a
// shell, only after the user confirms, with the terminal handed over.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { SKILLS_CLI, type SkillsEntry } from "./catalog.js";

/** The package runner; `npx.cmd` on Windows. */
export const NPX = process.platform === "win32" ? "npx.cmd" : "npx";

/** A confirmed install, waiting for the terminal to be handed over. */
export interface RunRequest {
  program: string;
  args: string[];
}

/** The command line as shown to the user. */
export const display = (req: RunRequest) => [path.basename(req.program), ...req.args].join(" ");

/** Arguments after the program, or undefined for installers the TUI never runs. */
export function args(entry: SkillsEntry): string[] | undefined {
  if (entry.installer !== SKILLS_CLI) return undefined;
  const out = ["skills", "add", entry.source];
  if (entry.skill !== undefined && entry.skill !== "*") out.push("-s", entry.skill);
  return out;
}

/** The skills CLI's project lock file; its presence means CLI-installed project skills. */
export const SKILLS_LOCK = "skills-lock.json";

/**
 * Updates project skills only (`-p`; skills CLI 1.9.0 docs, checked
 * 2026-11-27). No `-y`: the CLI's own prompts stay with the user.
 */
export const UPDATE_ARGS = ["skills", "update", "-p"];

/** `unknown`: installing every skill, so names are not known in advance. */
export type Conflict = { kind: "none" } | { kind: "with"; name: string } | { kind: "unknown" };

/** Compares the entry's `skill` with the harness skill names, ignoring case. */
export function conflict(entry: SkillsEntry, harness: Iterable<string>): Conflict {
  const name = entry.skill;
  if (name === undefined || name === "*") return { kind: "unknown" };
  for (const h of harness) {
    if (h.toLowerCase() === name.toLowerCase()) return { kind: "with", name: h };
  }
  return { kind: "none" };
}

/**
 * Looks up `name` in the directories of a `PATH`-style value, without a
 * shell. On Windows a name without an extension also tries each `PATHEXT`
 * extension, so `npm` finds `npm.cmd`.
 */
export function findProgram(
  name: string,
  pathValue: string | undefined,
  platform: NodeJS.Platform = process.platform,
  pathext: string | undefined = process.env.PATHEXT,
): string | undefined {
  if (!pathValue) return undefined;
  const win = platform === "win32";
  const names = [name];
  if (win && path.win32.extname(name) === "") {
    const exts = (pathext || ".COM;.EXE;.BAT;.CMD").split(";").filter((e) => e !== "");
    names.push(...exts.map((e) => name + e.toLowerCase()));
  }
  for (const dir of pathValue.split(win ? ";" : ":")) {
    if (dir === "") continue;
    for (const n of names) {
      const full = path.join(dir, n);
      if (fs.statSync(full, { throwIfNoEntry: false })?.isFile()) return full;
    }
  }
  return undefined;
}

/** Characters `cmd.exe` would interpret, plus whitespace. */
const CMD_UNSAFE = /[\s&|<>^%!"()]/;

/** What to spawn for a request on a platform. */
export interface Spawn {
  file: string;
  args: string[];
  /** Pass arguments to `cmd.exe` exactly as given (Windows only). */
  verbatim: boolean;
}

/**
 * Node cannot start `.cmd`/`.bat` files without `cmd.exe`, and the `shell`
 * option is deprecated (DEP0190). Batch files run through
 * `cmd.exe /d /s /c` only after every argument is checked for whitespace and
 * `cmd.exe` metacharacters; anything else runs directly. Throws on a
 * rejected argument. Pure, so it is tested on every OS.
 */
export function spawnFor(req: RunRequest, platform: NodeJS.Platform = process.platform): Spawn {
  const batch = platform === "win32" && /\.(cmd|bat)$/i.test(req.program);
  if (!batch) return { file: req.program, args: req.args, verbatim: false };
  for (const a of req.args) {
    if (a === "" || CMD_UNSAFE.test(a)) {
      throw new Error(`argument \`${a}\` cannot be passed to a batch file safely; not run`);
    }
  }
  // The program path is quoted, so spaces are fine; `%`, `!` and `"` are not.
  if (/[%!"]/.test(req.program)) {
    throw new Error(`program path \`${req.program}\` cannot be run safely through cmd.exe`);
  }
  // `/s` strips the outer quotes, leaving `"<program>" arg ...`.
  const line = [`"${req.program}"`, ...req.args].join(" ");
  return { file: "cmd.exe", args: ["/d", "/s", "/c", `"${line}"`], verbatim: true };
}

/**
 * Runs the command without a shell, inheriting stdio. Returns the exit
 * code, or null when a signal ended the process. Throws when it cannot start.
 */
export function run(req: RunRequest, cwd: string): number | null {
  const s = spawnFor(req);
  const out = spawnSync(s.file, s.args, {
    cwd,
    stdio: "inherit",
    windowsVerbatimArguments: s.verbatim,
  });
  if (out.error) throw new Error(`${req.program}: ${out.error.message}`);
  return out.status;
}

/** After a handover: waits for Enter so the user can read the output (spec §6.1). */
export async function waitForEnter(): Promise<void> {
  const { createInterface } = await import("node:readline/promises");
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    await rl.question("\nEnter를 누르면 agent-harness-tui로 돌아갑니다. ");
  } finally {
    rl.close();
  }
}
