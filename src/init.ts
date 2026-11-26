// Initialization orchestration (instruction 0010): loads existing state and
// combines every module into one plan, so the TUI can preview and apply it
// in a single explicit step. No terminal code here.

import fs from "node:fs";
import * as apply from "./apply.js";
import { type AgentEntry, builtinAgents, type HookEntry } from "./catalog.js";
import {
  CONFIG_FILE,
  configLayout,
  defaultLock,
  defaultState,
  HARNESS_CONFIG,
  HARNESS_LOCK,
  HARNESS_STATE,
  type HarnessConfig,
  type HarnessLock,
  type HarnessState,
  LOCK_FILE,
  load as loadContract,
  toText,
  validate,
} from "./contract.js";
import * as gitignore from "./gitignore.js";
import * as guidance from "./guidance.js";
import {
  applyHiddenAttribute,
  type HiddenAttr,
  type ProjectRoot,
  RelPath,
  validateLayout,
} from "./paths.js";
import * as placement from "./placement.js";
import { importItems, lockWith, type Package } from "./sources.js";

/** Existing files in the project. `config` is undefined before the first init. */
export interface Loaded {
  config?: HarnessConfig;
  lock: HarnessLock;
  state: HarnessState;
}

/** Throws on unreadable or unsupported files instead of ignoring them. */
export function load(root: ProjectRoot): Loaded {
  const config = loadContract(HARNESS_CONFIG, root.path);
  const out: Loaded = {
    lock: loadContract(HARNESS_LOCK, root.path) ?? defaultLock(),
    state: loadContract(HARNESS_STATE, root.path) ?? defaultState(),
  };
  if (config !== undefined) out.config = config;
  return out;
}

/** A skill fetched in this session, to import on apply. */
export interface PendingImport {
  skillId: string;
  sourceId: string;
  reference: string;
  pageType?: string;
  acceptPartial: boolean;
  package: Package;
}

export interface Prepared {
  config: HarnessConfig;
  lock: HarnessLock;
  plan: apply.Plan;
  git: gitignore.GitReport;
  warnings: string[];
}

/**
 * Builds the full plan: imports, instruction dirs, guidance blocks, skill
 * copies, `.gitignore` block, `harness.toml` and `harness.lock.json`.
 * `agents` is the agent list of the project configuration; configured IDs
 * missing from it are kept but get no guidance or copies.
 * Reads only; nothing is written. Throws an `Error` with a readable message.
 */
export async function prepare(
  root: ProjectRoot,
  loaded: Loaded,
  config: HarnessConfig,
  imports: PendingImport[],
  agents: AgentEntry[] = builtinAgents(),
  hooks: HookEntry[] = [],
): Promise<Prepared> {
  validate(HARNESS_CONFIG, config);
  const errs = validateLayout(configLayout(config), root);
  if (errs.length > 0) throw new Error(errs.map((e) => e.message).join("; "));

  let lock = loaded.lock;
  const imported: apply.Desired[] = [];
  for (const i of imports) {
    try {
      const req = {
        skillId: i.skillId,
        sourceId: i.sourceId,
        reference: i.reference,
        acceptPartial: i.acceptPartial,
        ...(i.pageType === undefined ? {} : { pageType: i.pageType }),
      };
      const [desired, locked] = importItems(root, lock, req, i.package);
      lock = lockWith(lock, locked);
      imported.push(...desired);
    } catch (e) {
      throw new Error(`${i.skillId}: ${(e as Error).message}`);
    }
  }

  const [dirs, desired] = await guidance.planItems(root, config, agents, hooks);
  const { artifacts, docs, references } = config.paths;
  for (const d of [docs, references, artifacts]) if (d) dirs.push(d);
  // Git keeps no empty directories: a new tracked directory gets an empty,
  // create-only `.gitkeep`. Existing directories and ignored ones get none.
  for (const d of dirs) {
    if ((!artifacts || !d.equals(artifacts)) && !fs.existsSync(root.abs(d))) {
      desired.push(apply.createOnly(d.join(".gitkeep"), ""));
    }
  }
  desired.push(...imported);

  const placed = placement.planPlacement(root, config, lock, loaded.state, imported, agents);
  desired.push(...placed.desired);
  const copies = placement.ignorePaths(config, lock, agents);
  desired.push(gitignore.desired(root, config, copies));
  desired.push(apply.fileItem(RelPath.unchecked(CONFIG_FILE), toText(HARNESS_CONFIG, config)));
  desired.push(apply.fileItem(RelPath.unchecked(LOCK_FILE), toText(HARNESS_LOCK, lock)));

  const plan = apply.plan(root, loaded.state, dirs, desired);
  plan.deployments = placed.deployments;
  plan.adapters = placed.adapters;
  return {
    config,
    lock,
    plan,
    git: gitignore.gitReport(root, config, copies, "git", agents),
    warnings: placed.warnings,
  };
}

/** Applies a prepared plan, then tries to hide the output directory. Throws `ApplyError`. */
export function applyPrepared(
  root: ProjectRoot,
  prepared: Prepared,
  state: HarnessState,
): [apply.ApplyReport, HiddenAttr] {
  const report = apply.apply(root, prepared.plan, state);
  // Only a new artifacts directory whose name starts with `.`; a failure is
  // information, never an apply failure.
  const artifacts = prepared.config.paths.artifacts;
  const created = prepared.plan.dirs.some(([d, existed]) => !existed && artifacts?.equals(d));
  const hidden: HiddenAttr =
    report.failed === undefined && artifacts && created && artifacts.fileName().startsWith(".")
      ? applyHiddenAttribute(root.abs(artifacts))
      : { kind: "not-applicable" };
  return [report, hidden];
}
