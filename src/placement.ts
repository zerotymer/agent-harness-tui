// Agent-specific skill placement (Stage 1 spec §5, instruction 0008).
//
// `.skills/` stays the shared source. Per agent, skills are either copied
// into the directory the agent discovers, or only referenced from guidance.
//
// Official discovery rules, checked 2026-11-16 (adapter version 1):
//
// | Agent | Project skill directory | Source |
// | --- | --- | --- |
// | Claude Code | `.claude/skills/<name>/SKILL.md` | Claude Code CHANGELOG and skill docs (`name`/`description` frontmatter required) |
// | Codex | `.agents/skills/<name>/SKILL.md` | learn.chatgpt.com "Build skills" / customization overview (`name`/`description` required) |
//
// Listing `.skills/` in guidance does not make either agent discover it, so
// "reference" placement is guidance only. Symlink placement is not offered:
// it needs extra privileges on Windows and copies work everywhere.

import fs from "node:fs";
import { type Desired, fileItem, sha256 } from "./apply.js";
import type {
  AdapterRecord,
  AgentId,
  Deployment,
  HarnessConfig,
  HarnessLock,
  HarnessState,
} from "./contract.js";
import { type ProjectRoot, RelPath, skillsDir as sharedSkillsDir } from "./paths.js";

/** Bumped when an agent's discovery rules are re-checked and change. */
export const ADAPTER_VERSION = "1";
export const RULES_CHECKED = "2026-11-16";

export const agentName = (agent: AgentId) => (agent === "claude-code" ? "Claude Code" : "Codex");

/** Project directory the agent scans for skills. */
export const skillsDir = (agent: AgentId) =>
  RelPath.unchecked(agent === "claude-code" ? ".claude/skills" : ".agents/skills");

export type Method = "copy" | "reference" | "symlink";

/** Placement methods with availability; unavailable ones carry a reason. */
export function methods(_agent: AgentId): [Method, string | undefined][] {
  return [
    ["copy", undefined],
    ["reference", undefined],
    ["symlink", "not offered: symlinks need extra privileges on Windows; use copy"],
  ];
}

export interface PlacementPlan {
  desired: Desired[];
  deployments: Deployment[];
  adapters: AdapterRecord[];
  warnings: string[];
}

const unresolvable = (p: RelPath, reason: string): Desired => ({
  path: p,
  content: Buffer.alloc(0),
  ownership: { kind: "unresolvable", reason },
});

/** Agents in config order, first entry wins for duplicates. */
const uniqueAgents = (config: HarnessConfig) =>
  config.agents.filter((a, i) => config.agents.findIndex((b) => b.id === a.id) === i);

/**
 * Plans copies for every agent with `copy` placement and every locked
 * skill. `pending` holds files planned in the same apply (e.g. a fresh
 * import) whose bytes are not on disk yet.
 */
export function planPlacement(
  root: ProjectRoot,
  config: HarnessConfig,
  lock: HarnessLock,
  state: HarnessState,
  pending: Desired[],
): PlacementPlan {
  const out: PlacementPlan = { desired: [], deployments: [], adapters: [], warnings: [] };
  for (const a of uniqueAgents(config)) {
    out.adapters.push({ agent: a.id, version: ADAPTER_VERSION });
    if (a.placement !== "copy") continue;
    for (const skill of lock.skills) {
      const targetDir = skillsDir(a.id).join(skill.id);
      const deployed = state.deployments.some((d) => d.agent === a.id && d.skill === skill.id);
      if (!deployed && fs.existsSync(root.abs(targetDir))) {
        out.desired.push(
          unresolvable(
            targetDir.join("SKILL.md"),
            `\`${targetDir}/\` already exists and was not deployed by this tool`,
          ),
        );
        continue;
      }
      for (const f of skill.files) {
        const source = sharedSkillsDir().join(skill.id).join(f.path.toString());
        const target = targetDir.join(f.path.toString());
        let bytes = pending.find((d) => d.path.equals(source))?.content;
        if (!bytes) {
          try {
            bytes = fs.readFileSync(root.abs(source));
          } catch (e) {
            if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
            out.desired.push(unresolvable(target, `source \`${source}\` is missing`));
            continue;
          }
        }
        if (f.path.toString() === "SKILL.md" && !bytes.subarray(0, 3).equals(Buffer.from("---"))) {
          out.warnings.push(
            `\`${source}\` has no frontmatter; ${agentName(a.id)} needs \`name\` and \`description\` to discover it`,
          );
        }
        out.deployments.push({
          skill: skill.id,
          agent: a.id,
          placement: "copy",
          target,
          hash: sha256(bytes),
        });
        out.desired.push(fileItem(target, bytes));
      }
    }
  }
  return out;
}

/**
 * Regenerable copies, ignored by Git by default. Per skill, so unrelated
 * skills in the same agent directory stay tracked.
 */
export function ignorePaths(config: HarnessConfig, lock: HarnessLock): RelPath[] {
  const out: RelPath[] = [];
  for (const a of config.agents.filter((a) => a.placement === "copy")) {
    for (const s of lock.skills) {
      const p = skillsDir(a.id).join(s.id);
      if (!out.some((o) => o.equals(p))) out.push(p);
    }
  }
  return out;
}
