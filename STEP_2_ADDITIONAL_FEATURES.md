# Step 2: Additional Harness Features

## Prerequisite and objective

Start only after [Step 1](STEP_1_INITIAL_CREATION.md) passes its acceptance criteria in the application repository. Extend that standalone TUI with bounded updates, configuration overrides, modes, hooks and optional Git worktree integrations. Keep Stage 1 initialization usable without these optional tools.

This instruction does not authorize building external backend services, a scheduler, a central backlog service, or a general container administration product.

## 1. Bounded harness updater

Allow these mutations only:

- Add, remove, or explicitly refresh selected skills and their managed agent-specific deployments.
- Add, modify, disable, or remove managed hooks.
- Enable or disable supported integration profiles and mode profiles.
- Regenerate the affected managed guidance blocks and compatible configuration/state metadata.

Do not modify application source, user-written instructions, references, instruction history, or database contents. Do not rename/move project roots or the Stage 1 directory layout through this updater. Path migration is a separate future feature, not an implicit consequence of a profile.

Before applying, compare desired configuration, last-applied baseline and current filesystem contents. Show additions, changes, removals, conflicts and their ownership. Never overwrite or delete user-modified content automatically. Removing a skill must only affect files owned by the matching deployment; preserve unrelated contents.

Version and migrate the Stage 1 metadata with backups. Commit successful state only after application succeeds. Make repeat application a no-op and provide recovery for partial failure. Disabling an integration removes only managed configuration; it must not delete worktrees, containers, networks, volumes or databases.

## 2. Override model

The following order belongs to the harness, not to third-party tools:

```text
built-in defaults
  -> enabled integration defaults
  -> project overrides
  -> machine/worktree-local overrides
  -> invocation-only selections
```

- Persist shared choices and project overrides in `harness.toml` and resolved imported versions in `harness.lock.json`.
- Introduce `.harness/local.toml` for non-secret machine/worktree-specific overrides. Keep it and local applied state outside Git.
- Invocation-only selections are transient unless the user explicitly saves them. A temporary mode must not rewrite shared guidance and create unnecessary Git changes.
- Define allowed override keys per feature. A profile must not bypass fixed paths or the mutation boundary.
- Later values replace scalar values. Key skills and hooks by stable IDs; support explicit disable/removal. Do not merge lists blindly or register the same command twice.
- Resolve conflicts between enabled integration defaults explicitly rather than depending on accidental iteration order.
- Show each effective value, its source layer, and the final generated changes in the preview.
- Translate the resolved model into each tool's actual configuration semantics. In particular, Worktrunk hooks may accumulate across native configuration layers; do not assume a harness replacement automatically replaces native hooks.

## 3. Modes

Provide analysis, implementation and review profiles as project policies. Each profile selects relevant guidance, skills and supported checks. Keep the project's default profile distinct from the current invocation's selection.

State exactly whether a profile applies guidance only or also configures supported agent permissions/sandbox controls. Do not claim that a Markdown rule enforces read-only behavior. Detect the supported mechanisms and versions for each agent; show unsupported enforcement as unavailable, without a silent fallback to the same safety claim.

Switch modes through supported session mechanisms or provide explicit instructions when a restart is needed. Do not claim a running agent has reloaded a changed file without verifying that behavior. Do not add model selection, billing policies or autonomous task orchestration in this stage.

## 4. Hook architecture

Separate harness lifecycle hooks from agent runtime hooks and worktree/container lifecycle hooks.

| Kind | Owner and purpose |
| --- | --- |
| Harness | Before/after configuration application, validation and application-result checks |
| Agent | Supported session/tool events; project consistency checks and configured restrictions |
| Worktree/container | Optional Worktrunk/Docktree lifecycle integration |

- Start with native validators; importing a document must not execute its bundled scripts.
- Define hook ID, owner, supported event, executable/argument vector, root/cwd contract, supported OS/agent versions, timeout, blocking/background behavior, failure policy and enablement.
- Prefer calling a harness executable subcommand with structured arguments over generating OS-specific shell fragments. If a tool requires a shell, declare and verify the runtime dependency.
- Treat arguments, variables and paths as data; do not interpolate untrusted remote content into shell commands.
- A remotely supplied executable hook requires separate visible activation. Preview the command and its origin; do not bypass the external tool's own trust or approval mechanism.
- Define hook order and failure handling. A failed required precondition blocks its associated operation; optional post-processing failure is reported and must not masquerade as success.
- Assign one lifecycle owner for each action. Avoid duplicate initialization or cleanup through agent and Worktrunk hooks.
- Keep execution logs local and redact credentials. Disable recursion and repeated self-triggering.
- Session termination is not task completion. Do not add automatic instruction archival or synthetic completion events to stop hooks. A future explicit task workflow can use validated lifecycle actions.
- Check current official hook contracts per adapter. Unsupported and deprecated events must not be presented as available; do not add legacy compatibility shims.

## 5. Git worktree foundation, then Worktrunk

Implement worktree-aware identity and state before adding the Worktrunk adapter. Worktrunk is an optional manager of Git worktrees, not a replacement for the project-root contract.

- Keep the current worktree root as the effective project root. Discover shared repository identity separately through Git; do not assume `.git` is a directory.
- Keep managed relative paths unchanged inside each worktree. Instructions, references and canonical skills follow that worktree's checkout, not one globally writable copy.
- Keep `.output/`, `.harness/` and optional `agent.db` separate per worktree by default. Do not automatically share or copy SQLite files, credentials or transient state.
- A new worktree without local applied state must bootstrap ownership checks from its tracked configuration/lock data. Do not assume untracked pending instructions will be copied.
- Preserve append-only instruction events with unique event IDs. Detect log/sequence conflicts when branches are merged; never claim a global sequence or silently rewrite history. Keep Git merges explicit rather than introducing a central log service.
- Detect Worktrunk availability and supported version. Default its integration to disabled; installation alone does not enable it.
- Generate only supported project-scoped settings (currently documented at `.config/wt.toml`) and managed workflow guidance/skills. Preserve existing settings and show native hook accumulation effects.
- Project settings must not silently modify user-home configuration or shell startup files. Inform users about required shell integration or dependencies separately.
- Make worktree creation/switch/removal integration actions explicit. Do not automatically merge, delete branches or remove a worktree when changing a profile.
- Validate Windows support against the current Worktrunk contract, including Git Bash dependencies and executable-name conflicts. Do not claim that a native Rust TUI makes external hook runtimes cross-platform automatically.

## 6. Optional Docktree integration

- Gate this integration on worktree-aware state and verified Docktree/Docker Compose prerequisites, not on Worktrunk being enabled. Users may manage worktrees by other means.
- Keep Worktrunk and Docktree enablement independent. Support neither, either, or both, with a dependency/status preview.
- When both are enabled, define one explicit flow: worktree creation -> local harness preparation -> optional Docktree environment preparation. Report intermediate failures; do not provision an environment twice.
- Verify current Docktree configuration, state locations, platform support and lifecycle commands before implementation. Generate only the managed project settings and documentation required by the selected configuration.
- Distinguish harness configuration overrides from Docktree-generated Compose overrides. Preserve original Compose files; do not hand-maintain a competing allocation registry.
- Treat Docktree as the source of truth for its assigned ports, container/project identifiers, volumes and other runtime resources. Query them; store only integration references and last observed status locally.
- Preview environment start/stop actions separately from configuration updates. Do not start containers merely by selecting a profile or importing a skill.
- Integration disablement must not invoke resource cleanup. Destructive worktree or volume removal belongs to explicit commands with a resource preview.
- Show unsupported operating systems or tool versions as unsupported. No legacy emulation layer is required by this instruction.

## 7. External systems: planning boundaries only

Document the following optional systems and integration seams. Do not build or deploy them under this instruction.

| Candidate | Intended role |
| --- | --- |
| `md.zerotymer.net` or another skill/guidance source | Catalog, documents, revisions and authentication |
| Hook/harness-template repository | Reusable profiles and platform/agent compatibility metadata |
| Backlog/task-state system | Optional instruction linkage; SQLite adapter only after the real `agent.db` contract is supplied |
| Credential facility | Keep actual tokens outside shared project files |

Define connection checks, bounded timeouts, API/version capability reporting and stale-cache handling. Existing local documents must remain usable during a remote outage. Do not require a daemon, web server, queue or central database for the TUI to work.

Do not implement a shared SQLite database across machines or worktrees, or assume a generic registry API. Separate future integration instructions must specify those contracts.

## 8. TUI additions and persisted state

Add views for current applied state, skill/hook updates, modes, integration selection, layered effective configuration, conflict resolution and recovery results.

Record stable IDs, source revisions, enablement, managed files/blocks, baseline hashes, copy/reference mappings, repository/worktree identity, supported adapter versions, and application results. Keep shared intent separate from observed local runtime state.

Configuring an integration does not prove that its external runtime is healthy. Display configured, unavailable, unsupported and verified states distinctly.

## 9. Acceptance criteria

- All Stage 1 acceptance criteria continue to pass without Worktrunk, Docktree or Docker installed.
- Skills and hooks can be added/removed/updated within the declared boundary; user edits survive conflict detection and cancelled updates.
- Override precedence is deterministic, allowed keys are enforced, and each effective setting has visible provenance.
- Missing local state, metadata migration and interrupted updates do not authorize destructive overwrites or false success records.
- Tests verify hook argument handling, timeout/failure policy, duplicate execution prevention and no execution during remote import.
- Two worktrees have independent output, database and applied state; relative paths resolve within the current worktree.
- Verify plain Git-worktree operation, Worktrunk-only, Docktree-only and combined profiles where their prerequisites are available. Clearly mark integrations not exercised against real supported tools.
- Disabling integrations deletes no worktree or runtime resources. Root paths and user documents are unchanged by updates.
- Guidance-only modes are clearly distinguished from genuinely configured permission enforcement; unsupported mechanisms are visible.
- Shared configuration, lock data and all generated guidance remain consistent across English/Korean documents. No credentials enter tracked output.

## Deliverables

The extended application, configuration/state migrations, focused tests and bilingual update/integration documentation. Include a verified OS/agent/tool compatibility matrix and a list of unresolved external contracts. Do not create or deploy any new external service.
