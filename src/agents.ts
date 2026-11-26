// Agent detection and install commands (Stage 2 spec §3, instruction 0023).
//
// Agents come from `agents` in the project configuration. Detection runs
// each `detect` argument vector without a shell, in parallel, with a
// timeout; exit code 0 means installed. Installs only build the request:
// the caller runs it after the user confirms, with the terminal handed over.

import { spawn } from "node:child_process";
import { type AgentEntry, currentOs } from "./catalog.js";
import { findProgram, type RunRequest, spawnFor } from "./install.js";

export const DETECT_TIMEOUT_MS = 5_000;

export type Detection =
  | { kind: "installed"; version?: string }
  | { kind: "missing"; reason: string };

/** Runs one `detect` vector. Never throws. */
export function detect(
  argv: string[],
  timeoutMs = DETECT_TIMEOUT_MS,
  pathValue = process.env.PATH,
): Promise<Detection> {
  const [name = "", ...args] = argv;
  const program = findProgram(name, pathValue);
  if (!program) return Promise.resolve({ kind: "missing", reason: `\`${name}\` not on PATH` });
  let s: ReturnType<typeof spawnFor>;
  try {
    s = spawnFor({ program, args });
  } catch (e) {
    return Promise.resolve({ kind: "missing", reason: (e as Error).message });
  }
  return new Promise((resolve) => {
    const child = spawn(s.file, s.args, {
      stdio: ["ignore", "pipe", "ignore"],
      windowsVerbatimArguments: s.verbatim,
      windowsHide: true,
    });
    let out = "";
    child.stdout.setEncoding("utf8").on("data", (d: string) => {
      out += d;
    });
    const timer = setTimeout(() => {
      child.kill();
      resolve({ kind: "missing", reason: `timed out after ${timeoutMs / 1000} s` });
    }, timeoutMs);
    child.on("error", (e) => {
      clearTimeout(timer);
      resolve({ kind: "missing", reason: e.message });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) {
        resolve({ kind: "missing", reason: `exit code ${code ?? "signal"}` });
        return;
      }
      const version = out.split(/\r?\n/)[0]?.trim();
      resolve(version ? { kind: "installed", version } : { kind: "installed" });
    });
  });
}

/** Detects every agent in parallel, in list order. */
export const detectAll = (agents: AgentEntry[], timeoutMs?: number, pathValue?: string) =>
  Promise.all(agents.map((a) => detect(a.detect, timeoutMs, pathValue)));

/**
 * The install request for this OS, or why there is none. The program is
 * looked up on `PATH` (with `PATHEXT` on Windows).
 */
export function installRequest(
  agent: AgentEntry,
  pathValue = process.env.PATH,
  platform: NodeJS.Platform = process.platform,
): RunRequest | { unavailable: string } {
  const os = currentOs(platform);
  const argv = os && agent.install[os];
  if (!argv) return { unavailable: "no install command for this OS" };
  const [name = "", ...args] = argv;
  const program = findProgram(name, pathValue, platform);
  if (!program) return { unavailable: `\`${name}\` not found on PATH` };
  return { program, args };
}
