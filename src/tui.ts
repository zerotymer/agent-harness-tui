// Setup wizard (Stage 1 spec §7, Stage 2 spec §2–§3; instructions 0010,
// 0023). Korean UI text. Steps: agents → paths → sources and skills →
// preview → result. An initialized project gets a read-only check instead.
//
// `Wizard` is a plain state machine: `key` changes state and `view` only
// describes it, so drawing never touches the network or disk. Network
// access (skill fetch) happens only when the user activates a button.
// Nothing is written before the explicit apply on the preview step.

import { type Detection, detectAll, installRequest } from "./agents.js";
import { type ApplyReport, conflicts, hasWrites, preview } from "./apply.js";
import * as catalog from "./catalog.js";
import { type AgentEntry, agentTitle, display, SKILLS_CLI, type SkillsEntry } from "./catalog.js";
import {
  DEFAULT_TOKEN_ENV,
  defaultConfig,
  type HarnessConfig,
  isSlug,
  type Placement,
} from "./contract.js";
import type { GitReport } from "./gitignore.js";
import * as init from "./init.js";
import * as install from "./install.js";
import { LOCAL_FILE, loadToken, Secret, saveToken } from "./local.js";
import {
  backlogPath,
  defaultLayout,
  type HiddenAttr,
  type Layout,
  type ProjectRoot,
  type RelPath,
  skillsDir,
  validateLayout,
} from "./paths.js";
import { methods } from "./placement.js";
import { type CatalogEntry, LocalDir, type Package, slugify, WikiGateway } from "./sources.js";

export type Step =
  /** Initialized project, or unreadable files: summary and check only. */
  | "verify"
  | "agents"
  /** Confirmation screen for agent installs. */
  | "agentInstall"
  | "paths"
  | "skills"
  | "preview"
  | "done"
  /** View of the configuration lists, opened from the skills step. */
  | "catalog"
  /** Confirmation screen for one public skill install. */
  | "install";

export const STEPS: [Step, string][] = [
  ["agents", "에이전트"],
  ["paths", "경로·백로그"],
  ["skills", "소스·스킬"],
  ["preview", "미리보기"],
  ["done", "적용·결과"],
];

/** Terminal-independent key. */
export type Key =
  | { name: "enter" | "esc" | "up" | "down" | "pageup" | "pagedown" | "backspace" | "ctrl-c" }
  | { name: "char"; ch: string };

export interface Line {
  text: string;
  bold?: boolean;
  inverse?: boolean;
}

// Rows on the skills step.
const SK_KIND = 0;
const SK_LOCATION = 1;
const SK_TOKEN = 2;
const SK_TYPE = 3;
const SK_REF = 4;
const SK_FETCH = 5;
const SK_NAME = 6;
const SK_PARTIAL = 7;
const SK_ADD = 8;
const SK_LIST = 9;
const SK_CATALOGS = 10;
const SK_FIXED_ROWS = 11;

type Result<T> = { ok: true; value: T } | { ok: false; error: string };
const attempt = <T>(f: () => T): Result<T> => {
  try {
    return { ok: true, value: f() };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
};

/** One agent of the configuration on the agents step. */
export interface AgentRow {
  entry: AgentEntry;
  selected: boolean;
  placement: Placement;
  /** Undefined while detection runs. */
  detection?: Detection;
  /** Exit code of the last install (null: signal), or why it could not run. */
  installed?: Result<number | null>;
}

/** Confirmed runs waiting for the terminal handover (see `takeRun`). */
export interface PendingRun {
  kind: "skill" | "agents";
  requests: install.RunRequest[];
  /** Agent rows, in request order (`agents` only). */
  rows: AgentRow[];
}

type InputField =
  | "references"
  | "output"
  | "instructions"
  | "tokenEnv"
  | "localDir"
  | "token"
  | "pageType"
  | "reference"
  | "skillName";

export class Wizard {
  step: Step = "agents";
  focus = 0;
  scroll = 0;
  message: string | undefined;
  quit = false;
  /** Set while an async action runs; keys are ignored meanwhile. */
  busy = false;
  readonly loaded: Result<init.Loaded>;
  /** `project.json` or the built-in default, or why it could not be read. */
  readonly project: Result<catalog.Loaded>;
  /** `PATH` used for detection and installs (overridable for tests). */
  pathValue: string | undefined;
  // agents
  agents: AgentRow[] = [];
  // paths
  references = defaultLayout().references.toString();
  output = defaultLayout().output.toString();
  instructions = defaultLayout().instructions.toString();
  backlog = false;
  // sources and skills
  wiki = false;
  localDir = "";
  tokenEnv = DEFAULT_TOKEN_ENV;
  /** Token typed in this session; never drawn. */
  token = "";
  /** Token from `.harness/local.toml`, or why it could not be read. */
  readonly storedToken: Result<Secret | undefined>;
  gatewayUrl: string | undefined;
  pageType = "skill";
  reference = "";
  skillName = "";
  acceptPartial = false;
  catalog: CatalogEntry[] = [];
  fetched: Package | undefined;
  imports: init.PendingImport[] = [];
  /** Source fields are fixed once a skill from that source was added. */
  localLocked: RelPath | undefined;
  // steps 5-6 and the initialized summary
  prepared: Result<init.Prepared> | undefined;
  result: Result<[ApplyReport, HiddenAttr]> | undefined;
  /** Outcome of saving the typed token after a successful apply. */
  tokenSaved: Result<void> | undefined;
  /** `npx` found on `PATH`, if any. */
  npx: string | undefined;
  /** Index of the skills catalog entry on the install screen. */
  installPick = 0;
  /** Confirmed runs waiting for the terminal handover (see `takeRun`). */
  pending: PendingRun | undefined;
  /** Exit code of the last install (null: signal), or why it could not run. */
  installResult: Result<number | null> | undefined;

  private constructor(
    readonly root: ProjectRoot,
    pathValue: string | undefined,
  ) {
    this.pathValue = pathValue;
    this.loaded = attempt(() => init.load(root));
    this.project = attempt(() => catalog.load(root.path));
    this.storedToken = attempt(() => loadToken(root.path));
    this.npx = install.findProgram(install.NPX, pathValue);
    this.agents = this.agentEntries().map((entry) => ({
      entry,
      selected: entry.default,
      placement: "copy",
    }));
  }

  /**
   * Loads the project. Detection is not started here; call `detect()`
   * (the screen does so right after the first draw).
   */
  static async create(
    root: ProjectRoot,
    opts: { pathValue?: string | undefined } = {},
  ): Promise<Wizard> {
    const w = new Wizard(root, "pathValue" in opts ? opts.pathValue : process.env.PATH);
    if (w.initialized() || !w.loaded.ok || !w.project.ok) {
      w.step = "verify";
      // Already initialized: summary and a read-only verification only.
      const config = w.loaded.ok ? w.loaded.value.config : undefined;
      if (w.loaded.ok && config && w.project.ok) {
        const agents = w.project.value.config.agents;
        w.prepared = await settle(init.prepare(root, w.loaded.value, config, [], agents));
      }
    }
    return w;
  }

  /** Agent entries of the configuration (empty when it could not be read). */
  agentEntries(): AgentEntry[] {
    return this.project.ok ? this.project.value.config.agents : [];
  }

  skillsEntries(): SkillsEntry[] {
    return this.project.ok ? this.project.value.config.skills : [];
  }

  /** Detects every agent in parallel; the screen redraws when it resolves. */
  async detect(): Promise<void> {
    for (const r of this.agents) delete r.detection;
    const entries = this.agents.map((r) => r.entry);
    const found = await detectAll(entries, undefined, this.pathValue);
    found.forEach((d, i) => {
      const row = this.agents[i];
      if (row) row.detection = d;
    });
  }

  initialized(): boolean {
    return this.loaded.ok && this.loaded.value.config !== undefined;
  }

  // ------------------------------------------------------------------ keys

  async key(k: Key): Promise<void> {
    if (this.busy) return;
    if (k.name === "ctrl-c") {
      this.quit = true; // cancel: nothing has been written before apply
      return;
    }
    const input = this.focusIsInput();
    switch (k.name) {
      case "esc":
        // Esc on the first step quits, like Ctrl+C.
        if (this.step === "agents" || this.step === "verify") this.quit = true;
        else this.back();
        return;
      case "up":
        return this.moveFocus(-1);
      case "down":
        return this.moveFocus(1);
      case "pageup":
        this.scroll = Math.max(0, this.scroll - 10);
        return;
      case "pagedown":
        this.scroll += 10;
        return;
      case "enter":
        this.busy = true;
        try {
          await this.enter();
        } finally {
          this.busy = false;
        }
        return;
      case "backspace": {
        const f = this.inputField();
        if (f) this[f] = [...this[f]].slice(0, -1).join("");
        return;
      }
      case "char":
        if (k.ch === "q" && !input) {
          this.quit = true;
        } else if (k.ch === " " && !input) {
          this.toggle();
        } else {
          const f = this.inputField();
          if (f) this[f] += k.ch;
        }
        return;
    }
  }

  rows(): number {
    switch (this.step) {
      case "paths":
        return 4;
      case "agents":
        return this.agents.length * 2 + 2;
      case "skills":
        return SK_FIXED_ROWS + this.catalog.length;
      case "catalog":
        return this.skillsEntries().length;
      default:
        return 0;
    }
  }

  private moveFocus(delta: number) {
    const rows = this.rows();
    if (rows === 0) {
      // Scrollable text steps.
      this.scroll = Math.max(0, this.scroll + delta);
      return;
    }
    this.focus = (((this.focus + delta) % rows) + rows) % rows;
  }

  focusIsInput(): boolean {
    if (this.step === "paths") return this.focus <= 2;
    if (this.step === "skills") {
      return [SK_LOCATION, SK_TOKEN, SK_TYPE, SK_REF, SK_NAME].includes(this.focus);
    }
    return false;
  }

  private inputField(): InputField | undefined {
    const f = this.focus;
    if (this.step === "paths") {
      return (["references", "output", "instructions"] as const)[f];
    }
    if (this.step !== "skills") return undefined;
    if (f === SK_LOCATION) {
      if (this.wiki) return "tokenEnv";
      return this.localLocked ? undefined : "localDir";
    }
    if (this.wiki && f === SK_TOKEN) return "token";
    if (this.wiki && f === SK_TYPE) return "pageType";
    if (f === SK_REF) return "reference";
    if (f === SK_NAME) return "skillName";
    return undefined;
  }

  private toggle() {
    const f = this.focus;
    if (this.step === "paths" && f === 3) {
      this.backlog = !this.backlog;
    } else if (this.step === "agents") {
      const a = this.agents[Math.floor(f / 2)];
      if (!a) return;
      if (f % 2 === 0) a.selected = !a.selected;
      else a.placement = a.placement === "copy" ? "reference" : "copy";
    } else if (this.step === "skills" && f === SK_KIND) {
      this.wiki = !this.wiki;
      this.catalog = [];
      this.fetched = undefined;
    } else if (this.step === "skills" && f === SK_PARTIAL) {
      this.acceptPartial = !this.acceptPartial;
    }
  }

  private back() {
    this.message = undefined;
    this.focus = 0;
    this.scroll = 0;
    switch (this.step) {
      case "verify":
      case "agents":
      case "done":
        return;
      case "agentInstall":
        this.focus = this.agentInstallRow();
        this.step = "agents";
        return;
      case "paths":
        this.step = "agents";
        return;
      case "skills":
        this.step = "paths";
        return;
      case "preview":
        this.prepared = undefined;
        this.step = "skills";
        return;
      case "catalog":
        this.focus = SK_CATALOGS;
        this.step = "skills";
        return;
      case "install":
        this.focus = this.installPick;
        this.step = "catalog";
        return;
    }
  }

  private go(step: Step) {
    this.step = step;
    this.focus = 0;
    this.scroll = 0;
    this.message = undefined;
  }

  private async enter() {
    switch (this.step) {
      case "verify":
        this.quit = true;
        return;
      case "agents":
        if (this.focus === this.agentInstallRow()) {
          this.go("agentInstall");
        } else if (this.focus === this.agentInstallRow() + 1) {
          await this.detect();
          this.message = "다시 감지했습니다.";
        } else {
          this.leaveAgents();
        }
        return;
      case "agentInstall":
        return this.confirmAgentInstall();
      case "paths": {
        const l = attempt(() => this.layout());
        if (l.ok) this.go("skills");
        else this.message = l.error;
        return;
      }
      case "skills":
        switch (this.focus) {
          case SK_FETCH:
            return this.fetch();
          case SK_ADD:
            return this.addImport();
          case SK_LIST:
            return this.loadCatalog();
          case SK_CATALOGS:
            return this.go("catalog");
          default:
            if (this.focus >= SK_FIXED_ROWS) return this.pickCatalog(this.focus - SK_FIXED_ROWS);
            return this.openPreview();
        }
      case "preview":
        return this.apply();
      case "done":
        this.quit = true;
        return;
      case "catalog":
        if (this.rows() > 0) {
          this.installPick = this.focus;
          this.installResult = undefined;
          this.go("install");
        } else {
          this.back();
        }
        return;
      case "install":
        return this.confirmInstall();
    }
  }

  /** Skill names the harness deployed (lock) or selected in this session. */
  private harnessSkillNames(): string[] {
    const names = this.imports.map((i) => i.skillId);
    if (this.loaded.ok) names.push(...this.loaded.value.lock.skills.map((s) => s.id));
    return names;
  }

  installEntry(): SkillsEntry | undefined {
    return this.skillsEntries()[this.installPick];
  }

  private agentInstallRow(): number {
    return this.agents.length * 2;
  }

  /** Selected agents that are not detected as installed. */
  installTargets(): AgentRow[] {
    return this.agents.filter((r) => r.selected && r.detection?.kind !== "installed");
  }

  private leaveAgents() {
    const selected = this.agents.filter((r) => r.selected);
    if (selected.length === 0) {
      this.message = "에이전트를 하나 이상 선택하세요.";
    } else if (selected.some((r) => r.detection === undefined)) {
      this.message = "감지가 끝날 때까지 기다리세요.";
    } else if (!selected.some((r) => r.detection?.kind === "installed")) {
      this.message = "선택한 에이전트 중 설치된 것이 없습니다. [설치]로 하나 이상 설치하세요.";
    } else {
      this.go("paths");
    }
  }

  /** Queues every available install; nothing runs without this confirmation. */
  private confirmAgentInstall() {
    const run: PendingRun = { kind: "agents", requests: [], rows: [] };
    for (const r of this.installTargets()) {
      const req = installRequest(r.entry, this.pathValue);
      if ("program" in req) {
        run.requests.push(req);
        run.rows.push(r);
      }
    }
    if (run.requests.length === 0) {
      this.message = "실행할 수 있는 설치 명령이 없습니다.";
      return;
    }
    this.pending = run;
    this.message = undefined;
  }

  private installConflict(e: SkillsEntry): install.Conflict {
    return install.conflict(e, this.harnessSkillNames());
  }

  /** Queues the install; nothing runs without this explicit confirmation. */
  private confirmInstall() {
    const e = this.installEntry();
    if (!e) return;
    const args = install.args(e);
    if (!args) {
      this.message = "이 항목은 TUI가 실행하지 않습니다. 안내된 명령을 직접 실행하세요.";
      return;
    }
    const c = this.installConflict(e);
    if (c.kind === "with") {
      this.message = `하네스 스킬 \`${c.name}\`과 이름이 겹쳐 설치하지 않습니다.`;
      return;
    }
    if (!this.npx) {
      this.message = `\`${install.NPX}\`를 찾을 수 없어 설치할 수 없습니다.`;
      return;
    }
    this.pending = { kind: "skill", requests: [{ program: this.npx, args }], rows: [] };
    this.message = undefined;
  }

  /** The confirmed runs, for the caller to run with the terminal handed over. */
  takeRun(): PendingRun | undefined {
    const r = this.pending;
    this.pending = undefined;
    return r;
  }

  /**
   * Records the outcomes of a run returned by `takeRun`, in request order.
   * After agent installs, detection runs again.
   */
  async finishRun(run: PendingRun, results: Result<number | null>[]): Promise<void> {
    if (run.kind === "skill") {
      this.installResult = results[0];
      return;
    }
    run.rows.forEach((r, i) => {
      const res = results[i];
      if (res) r.installed = res;
    });
    await this.detect();
  }

  // --------------------------------------------------------------- actions

  /** Throws with a Korean-prefixed message. */
  private layout(): Layout {
    const resolve = (label: string, input: string) => {
      try {
        return this.root.resolve(input);
      } catch (e) {
        throw new Error(`${label}: ${(e as Error).message}`);
      }
    };
    const layout: Layout = {
      references: resolve("참조 문서 경로", this.references),
      output: resolve("산출물 경로", this.output),
      instructions: resolve("지시서 경로", this.instructions),
      backlogEnabled: this.backlog,
    };
    const errs = validateLayout(layout, this.root);
    if (errs.length > 0) throw new Error(errs.map((e) => e.message).join("; "));
    return layout;
  }

  private localSource(): [RelPath, LocalDir] {
    let rel = this.localLocked;
    if (!rel) {
      try {
        rel = this.root.resolve(this.localDir);
      } catch (e) {
        throw new Error(`로컬 폴더는 프로젝트 안에 있어야 합니다: ${(e as Error).message}`);
      }
    }
    return [rel, new LocalDir(this.root.abs(rel))];
  }

  private gateway(): WikiGateway {
    const g = new WikiGateway(this.tokenEnv.trim());
    if (this.gatewayUrl) g.baseUrl = this.gatewayUrl;
    g.storedToken =
      Secret.from(this.token) ?? (this.storedToken.ok ? this.storedToken.value : undefined);
    return g;
  }

  private pageTypeValue(): string | undefined {
    const t = this.pageType.trim();
    return this.wiki && t !== "" ? t : undefined;
  }

  private loadCatalog() {
    if (this.wiki) {
      // ponytail: picking from the wiki catalog is not ported (0025); 0023 replaces it.
      this.message = "위키 소스는 목록을 불러오지 않습니다. 페이지 UUID를 직접 입력하세요.";
      return;
    }
    try {
      const list = this.localSource()[1].list();
      this.message = list.length === 0 ? "목록이 비어 있습니다." : `${list.length}개 항목`;
      this.catalog = list;
    } catch (e) {
      this.message = `목록 오류: ${(e as Error).message}`;
    }
  }

  private pickCatalog(i: number) {
    const e = this.catalog[i];
    if (!e) return;
    this.reference = e.reference;
    this.skillName = slugify(e.title);
    this.fetched = undefined;
    this.focus = SK_FETCH;
    this.message = `\`${e.title}\` 선택됨. [조회]로 내용을 가져오세요.`;
  }

  private async fetch() {
    const reference = this.reference.trim();
    try {
      const pkg = this.wiki
        ? await this.gateway().fetch(reference, this.pageTypeValue())
        : this.localSource()[1].fetch(reference);
      if (this.skillName.trim() === "") this.skillName = slugify(pkg.title ?? reference);
      this.message = `파일 ${pkg.files.length}개를 가져왔습니다.`;
      this.fetched = pkg;
    } catch (e) {
      this.fetched = undefined;
      const msg = (e as Error).message;
      this.message = msg.startsWith("로컬 폴더") ? msg : `조회 오류: ${msg}`;
    }
  }

  private addImport() {
    const pkg = this.fetched;
    if (!pkg) {
      this.message = "먼저 [조회]로 내용을 가져오세요.";
      return;
    }
    const name = this.skillName.trim();
    if (!isSlug(name)) {
      this.message = "스킬 이름은 영문·숫자·`.`·`_`·`-`만 쓸 수 있습니다.";
      return;
    }
    if (this.imports.some((i) => i.skillId.toLowerCase() === name.toLowerCase())) {
      this.message = `\`${name}\`은 이미 선택 목록에 있습니다.`;
      return;
    }
    if (pkg.missingLocales.length > 0 && !this.acceptPartial) {
      this.message = `빠진 언어: ${pkg.missingLocales.join(", ")}. 부분 가져오기를 수락해야 추가할 수 있습니다.`;
      return;
    }
    let sourceId = "wiki";
    if (!this.wiki) {
      try {
        this.localLocked = this.localSource()[0];
      } catch (e) {
        this.message = (e as Error).message;
        return;
      }
      sourceId = "local";
    }
    const pageType = this.pageTypeValue();
    this.imports.push({
      skillId: name,
      sourceId,
      reference: this.reference.trim(),
      acceptPartial: this.acceptPartial,
      package: pkg,
      ...(pageType === undefined ? {} : { pageType }),
    });
    this.fetched = undefined;
    this.reference = "";
    this.skillName = "";
    this.message = `\`${name}\`을 선택 목록에 추가했습니다.`;
  }

  config(): HarnessConfig {
    const layout = this.layout();
    const c = defaultConfig();
    c.paths = {
      instructions: layout.instructions,
      references: layout.references,
      output: layout.output,
    };
    c.backlog = { enabled: this.backlog, path: backlogPath(layout) };
    c.agents = this.agents
      .filter((a) => a.selected)
      .map((a) => ({ id: a.entry.id, placement: a.placement }));
    if (this.localLocked) {
      c.sources.push({ id: "local", kind: "local-dir", path: this.localLocked });
    }
    if (this.imports.some((i) => i.sourceId === "wiki")) {
      c.sources.push({ id: "wiki", kind: "wiki-gateway", tokenEnv: this.tokenEnv.trim() });
    }
    c.skills = this.imports.map((i) => ({
      id: i.skillId,
      source: i.sourceId,
      reference: i.reference,
      ...(i.pageType === undefined ? {} : { pageType: i.pageType }),
    }));
    return c;
  }

  private async openPreview() {
    if (!this.loaded.ok) {
      this.prepared = { ok: false, error: this.loaded.error };
    } else {
      const loaded = this.loaded.value;
      this.prepared = await settle(
        (async () =>
          init.prepare(this.root, loaded, this.config(), this.imports, this.agentEntries()))(),
      );
    }
    this.go("preview");
  }

  private apply() {
    const p = this.prepared;
    if (!p?.ok) {
      this.message = "계획에 오류가 있어 적용할 수 없습니다. Esc로 돌아가 수정하세요.";
      return;
    }
    if (conflicts(p.value.plan).length > 0) {
      this.message = "충돌을 해결한 뒤 다시 시도하세요. 아무것도 쓰지 않았습니다.";
      return;
    }
    if (!this.loaded.ok) return;
    const state = this.loaded.value.state;
    const result = attempt(() => init.applyPrepared(this.root, p.value, state));
    // The typed token is kept only after a successful apply.
    const applied = result.ok && result.value[0].failed === undefined;
    const typed = Secret.from(this.token);
    const stored = this.storedToken.ok ? this.storedToken.value : undefined;
    if (applied && typed && typed.expose() !== stored?.expose()) {
      this.tokenSaved = attempt(() => saveToken(this.root.path, typed));
    }
    this.result = result;
    this.go("done");
  }

  // ----------------------------------------------------------------- view

  /** Sidebar position; the catalog views belong to the skills step. */
  stepIndex(): number {
    const shown =
      this.step === "catalog" || this.step === "install"
        ? "skills"
        : this.step === "verify" || this.step === "agentInstall"
          ? "agents"
          : this.step;
    return Math.max(
      0,
      STEPS.findIndex(([s]) => s === shown),
    );
  }

  title(): string {
    const i = this.stepIndex();
    const base = `${i + 1}. ${STEPS[i]?.[1]}`;
    if (this.step === "catalog") return `${base} ─ 카탈로그`;
    if (this.step === "install") return `${base} ─ 공개 스킬 설치`;
    if (this.step === "agentInstall") return `${base} ─ 설치 확인`;
    if (this.step === "verify") return "초기화된 프로젝트";
    return base;
  }

  /** Shown on every screen: the project root and the configuration source. */
  header(): string {
    const p = this.project;
    const source = !p.ok
      ? "구성 오류"
      : `${p.value.source === "project" ? catalog.PROJECT_FILE : "내장 default.json"}${
          p.value.config.catalogVersion ? ` (버전 ${p.value.config.catalogVersion})` : ""
        }`;
    return `${this.root.path} · 구성: ${source}`;
  }

  keys(): string {
    switch (this.step) {
      case "verify":
        return "↑↓ 스크롤  Enter·Esc·q 종료";
      case "agents":
        return "↑↓ 이동  Space 선택·배치 전환  Enter 버튼 실행·다음  Esc·q 종료";
      case "agentInstall":
        return "Enter 확인·실행  Esc 돌아가기  q 종료";
      case "preview":
        return "↑↓ PgUp PgDn 스크롤  Enter 적용  Esc 이전  q·Ctrl+C 취소";
      case "done":
        return "↑↓ 스크롤  Enter·q 종료";
      case "skills":
        return "↑↓ 이동  Space 선택  Enter 버튼 실행·다음  Esc 이전  Ctrl+C 취소";
      case "catalog":
        return this.rows() > 0
          ? "↑↓ 공개 스킬 선택  PgUp PgDn 스크롤  Enter 설치 확인 화면  Esc 돌아가기"
          : "PgUp PgDn 스크롤  Enter·Esc 돌아가기  q 종료";
      case "install":
        return "Enter 확인·실행  Esc 돌아가기  q 종료";
      default:
        return "↑↓ 이동  Space 선택  Enter 다음  Esc 이전  Ctrl+C 취소";
    }
  }

  /** Body lines of the current step, message included. */
  lines(): Line[] {
    const v = {
      verify: () => this.verifyLines(),
      agents: () => this.agentLines(),
      agentInstall: () => this.agentInstallLines(),
      paths: () => this.pathsLines(),
      skills: () => this.skillLines(),
      preview: () => this.previewLines(),
      done: () => this.resultLines(),
      catalog: () => this.catalogLines(),
      install: () => this.installLines(),
    }[this.step]();
    if (this.message) v.push(raw(""), { text: this.message, bold: true });
    return v;
  }

  private row(n: number, text: string): Line {
    return this.focus === n && this.rows() > 0
      ? { text: `▸ ${text}`, inverse: true }
      : raw(`  ${text}`);
  }

  private verifyLines(): Line[] {
    const v = [
      raw("현재 실행 디렉터리를 루트로 사용합니다. 상위 폴더를 찾거나 Git을 초기화하지 않습니다."),
      raw(""),
    ];
    if (!this.project.ok) {
      v.push(raw(`프로젝트 구성을 읽을 수 없습니다: ${this.project.error}`));
      v.push(raw("내장 default.json으로 대체하지 않습니다. 파일을 고친 뒤 다시 실행하세요."));
    }
    if (!this.loaded.ok) {
      v.push(raw(`기존 파일을 읽을 수 없습니다: ${this.loaded.error}`));
    }
    if (!this.project.ok || !this.loaded.ok) {
      v.push(raw("파일을 변경하지 않았습니다."));
      return v;
    }
    const { config: c, state, lock } = this.loaded.value;
    if (!c) return v;
    v.push(raw("이미 초기화된 프로젝트입니다. 요약과 무변경 검증만 제공합니다."));
    v.push(raw("변경(스킬 추가·갱신 등)은 2단계 업데이트 기능에서 다룹니다."));
    v.push(raw(""));
    v.push(raw(`참조 문서: ${c.paths.references}/   산출물: ${c.paths.output}/`));
    v.push(raw(`백로그: ${c.backlog.enabled ? "사용" : "사용 안 함"}`));
    for (const a of c.agents) {
      const e = this.agentEntries().find((x) => x.id === a.id);
      v.push(
        raw(
          e
            ? `에이전트: ${agentTitle(e)} (${placementName(a.placement)})`
            : `에이전트: ${a.id} (알 수 없음: 구성에 없어 배치하지 않음)`,
        ),
      );
    }
    for (const s of lock.skills) {
      v.push(raw(`스킬: ${s.id} (소스 ${s.source}, 파일 ${s.files.length}개)`));
    }
    v.push(
      raw(
        state.lastSuccess
          ? `마지막 적용: ${state.lastSuccess.at}`
          : "로컬 적용 기록이 없습니다 (clone 직후일 수 있음).",
      ),
    );
    v.push(raw(""));
    const p = this.prepared;
    if (p?.ok) {
      const n = conflicts(p.value.plan).length;
      if (!hasWrites(p.value.plan) && n === 0) {
        v.push(raw("검증: 설정과 파일이 일치합니다. 변경할 항목이 없습니다."));
      } else {
        v.push(
          raw(`검증: 설정과 다른 항목이 있습니다 (충돌 ${n}개). 이 화면에서는 적용하지 않습니다.`),
        );
        v.push(...textLines(preview(p.value.plan)));
      }
    } else if (p) {
      v.push(raw(`검증 오류: ${p.error}`));
    }
    return v;
  }

  private pathsLines(): Line[] {
    const f = this.focus;
    return [
      this.row(0, `참조 문서 경로: ${input(this.references, f === 0)}`),
      this.row(1, `산출물 경로:   ${input(this.output, f === 1)}  (이름은 점으로 시작)`),
      this.row(
        2,
        `지시서 경로:   ${input(this.instructions, f === 2)}  (.completed/·completed.log 포함)`,
      ),
      this.row(3, `백로그 agent.db: ${check(this.backlog)}`),
      raw(""),
      raw("고정 경로 (1단계에서 변경 불가):"),
      raw(`  ${skillsDir()}/  스킬 원본`),
      raw(
        `  ${this.instructions.replace(/\/+$/, "")}/agent.db·agent-db.sh·agent-db.ps1  백로그 (지시서 경로를 따름)`,
      ),
    ];
  }

  private agentLines(): Line[] {
    const v: Line[] = [];
    if (this.agents.length === 0) v.push(raw("구성에 에이전트가 없습니다."));
    this.agents.forEach((r, i) => {
      v.push(this.row(i * 2, `${check(r.selected)} ${agentTitle(r.entry)}  ${detectionText(r)}`));
      const target =
        r.placement === "copy"
          ? `복사 → ${r.entry.skillsDir}/<id>/`
          : "참조 (지침에서 .skills/ 안내)";
      v.push(this.row(i * 2 + 1, `    배치: ${target}   지침: ${r.entry.guidance}`));
    });
    const n = this.agents.length * 2;
    v.push(this.row(n, "[설치] 선택했지만 감지되지 않은 에이전트 설치"));
    v.push(this.row(n + 1, "[다시 감지]"));
    v.push(raw(""));
    for (const [m, why] of methods("")) {
      if (m === "symlink" && why) v.push(raw(`심볼릭 링크 배치: 선택 불가 (${why})`));
    }
    v.push(raw("선택한 에이전트 중 하나 이상이 설치되어 있어야 다음 단계로 갑니다."));
    v.push(raw("에이전트는 지침 경로만으로 .skills/를 자동으로 찾지 않습니다."));
    return v;
  }

  private agentInstallLines(): Line[] {
    const targets = this.installTargets();
    if (targets.length === 0) {
      return [raw("설치할 에이전트가 없습니다. 감지되지 않은 에이전트를 선택하세요.")];
    }
    const v = [raw("실행할 명령 (셸 없이, 프로젝트 루트에서, 하나씩 차례로):"), raw("")];
    let ready = 0;
    for (const r of targets) {
      const req = installRequest(r.entry, this.pathValue);
      if ("program" in req) {
        ready++;
        v.push(raw(`${agentTitle(r.entry)}:  ${install.display(req)}`));
      } else {
        v.push(raw(`${agentTitle(r.entry)}:  사용 불가 (${req.unavailable})`));
      }
      if (r.installed?.ok) {
        v.push(raw(`    지난 결과: ${exitText(r.installed.value)}`));
      } else if (r.installed) {
        v.push(raw(`    지난 결과: 실행하지 못함 (${r.installed.error})`));
      }
    }
    v.push(raw(""));
    v.push(
      raw(
        "npm 전역 설치입니다. npm prefix에 쓸 권한이 없으면 EACCES로 실패하며, sudo나 권한 상승은 쓰지 않습니다.",
      ),
    );
    v.push(
      raw(
        "Claude Code는 Node.js 22 이상이 필요합니다. 셸 설치 스크립트(curl | bash 등)는 지원하지 않습니다.",
      ),
    );
    if (ready > 0) {
      v.push(raw(""));
      v.push(raw("Enter를 누르면 TUI 화면을 잠시 닫고 터미널을 설치 명령에 넘깁니다."));
      v.push(raw("끝나면 Enter로 돌아오고, 종료 코드를 보여 준 뒤 다시 감지합니다."));
    }
    return v;
  }

  private skillLines(): Line[] {
    const f = this.focus;
    const kind = this.wiki ? "위키 게이트웨이 (md.zerotymer.net)" : "로컬 폴더 (프로젝트 안)";
    const location = this.wiki
      ? `토큰 환경변수 이름(선택): ${input(this.tokenEnv, f === SK_LOCATION)}`
      : this.localLocked
        ? `로컬 폴더: ${this.localLocked}  (고정됨)`
        : `로컬 폴더: ${input(this.localDir, f === SK_LOCATION)}`;
    let token = "게이트웨이 토큰: 해당 없음";
    if (this.wiki) {
      // The value itself is never drawn.
      const state =
        this.token.trim() !== ""
          ? "입력됨 (표시하지 않음, 적용 후 .harness/local.toml에 저장)"
          : !this.storedToken.ok
            ? "저장된 토큰을 읽지 못함"
            : this.storedToken.value
              ? "저장된 토큰 사용"
              : "없음 (토큰 없이 진행)";
      token = `게이트웨이 토큰(선택): ${state}${f === SK_TOKEN ? "▏" : ""}`;
    }
    const pageType = this.wiki
      ? `경로 접두어(type): ${input(this.pageType, f === SK_TYPE)}`
      : "경로 접두어(type): 해당 없음";
    const reference = this.wiki ? "페이지 UUID" : "스킬 폴더 이름";
    const v = [
      this.row(SK_KIND, `소스: ${kind}`),
      this.row(SK_LOCATION, location),
      this.row(SK_TOKEN, token),
      this.row(SK_TYPE, pageType),
      this.row(SK_REF, `${reference}: ${input(this.reference, f === SK_REF)}`),
      this.row(SK_FETCH, "[조회] 내용 가져오기 (네트워크는 이때만 사용)"),
      this.row(SK_NAME, `스킬 이름(.skills/<이름>): ${input(this.skillName, f === SK_NAME)}`),
      this.row(SK_PARTIAL, `${check(this.acceptPartial)} 한 언어만 있어도 부분 가져오기 수락`),
      this.row(SK_ADD, "[추가] 선택 목록에 넣기"),
      this.row(SK_LIST, this.wiki ? "[목록 불러오기] 로컬 폴더만" : "[목록 불러오기]"),
      this.row(SK_CATALOGS, "[카탈로그 보기] 위키·스킬·문서의 출처·버전·상태"),
    ];
    if (this.wiki && !this.storedToken.ok) {
      v.push(raw(`  토큰 파일 오류: ${this.storedToken.error}`));
    }
    this.catalog.forEach((e, i) => {
      v.push(this.row(SK_FIXED_ROWS + i, `  ${e.title}  ${e.reference}`));
    });
    const p = this.fetched;
    if (p) {
      v.push(raw(""), raw(`조회 결과: ${p.title ?? ""}`));
      for (const file of p.files) v.push(raw(`  ${file.path} (${file.bytes.length} bytes)`));
      if (p.missingLocales.length > 0) {
        v.push(raw(`  빠진 언어: ${p.missingLocales.join(", ")} (번역을 만들지 않습니다)`));
      }
      v.push(...p.warnings.map((w) => raw(`  경고: ${w}`)));
    }
    v.push(raw(""));
    if (this.imports.length === 0) {
      v.push(raw("선택한 스킬: 없음 (스킬 없이 진행할 수 있습니다)"));
    } else {
      v.push(raw("선택한 스킬:"));
      for (const i of this.imports) v.push(raw(`  ${i.skillId} ← ${i.sourceId} ${i.reference}`));
    }
    v.push(raw("버튼 밖에서 Enter를 누르면 미리보기로 이동합니다."));
    return v;
  }

  private previewLines(): Line[] {
    const p = this.prepared;
    if (!p) return [];
    if (!p.ok) return [raw(`계획 오류: ${p.error}`)];
    const n = conflicts(p.value.plan).length;
    return [
      raw(
        n === 0
          ? "아래 변경을 적용합니다. Enter를 눌러야만 파일을 씁니다."
          : `충돌 ${n}개: 해결 전에는 적용할 수 없습니다.`,
      ),
      raw(""),
      ...textLines(preview(p.value.plan)),
      ...p.value.warnings.map((w) => raw(`경고: ${w}`)),
      raw(""),
      ...gitLines(p.value.git),
    ];
  }

  private resultLines(): Line[] {
    const v: Line[] = [];
    const r = this.result;
    if (r?.ok) {
      const [report, hidden] = r.value;
      if (report.failed !== undefined) {
        v.push(raw(`적용 실패: ${report.failed}`), raw("성공 상태를 기록하지 않았습니다."));
        v.push(...report.restored.map((p) => raw(`복구: ${p}`)));
        v.push(...report.residual.map((p) => raw(`복구 못 함: ${p}`)));
      } else {
        v.push(raw("적용을 완료했습니다."));
        for (const [label, list] of [
          ["생성", report.created],
          ["갱신", report.updated],
          ["변경 없음", report.unchanged],
          ["보존", report.preserved],
        ] as const) {
          v.push(...list.map((p) => raw(`${label}: ${p}`)));
        }
        if (report.applyId) v.push(raw(`백업 위치: .harness/backups/${report.applyId}/`));
        if (hidden.kind === "failed") {
          v.push(raw(`산출물 폴더 숨김 속성 적용 실패: ${hidden.message}`));
        }
      }
    } else if (r) {
      v.push(raw(`적용하지 않았습니다: ${r.error}`));
    }
    const t = this.tokenSaved;
    if (t?.ok) v.push(raw(`게이트웨이 토큰을 ${LOCAL_FILE}에 저장했습니다 (Git 무시 경로).`));
    else if (t) v.push(raw(`게이트웨이 토큰 저장 실패: ${t.error}`));
    v.push(raw(""), raw("Git 커밋·푸시는 하지 않았습니다."));
    return v;
  }

  private catalogLines(): Line[] {
    const v = [raw("프로젝트 구성만 읽습니다. 네트워크로 내려받지 않습니다."), raw("")];
    if (!this.project.ok) {
      v.push({ text: `구성 오류: ${this.project.error}`, bold: true });
      return v;
    }
    const c = this.project.value.config;
    v.push({ text: `공개 스킬 ${c.skills.length}개`, bold: true });
    c.skills.forEach((e, i) => {
      const state =
        e.installer !== SKILLS_CLI
          ? "직접 설치"
          : !this.npx
            ? "사용 불가"
            : this.installConflict(e).kind === "with"
              ? "충돌"
              : "설치 가능";
      v.push(this.row(i, `[${state}] ${e.id}  ${display(e.title)}  ← ${e.installer} ${e.source}`));
      if (e.installer === SKILLS_CLI) v.push(raw(`      스킬: ${e.skill ?? "*"}`));
      else if (e.install) v.push(raw(`      직접 실행: ${e.install}`));
    });
    // Listed only for now: importing, downloading and hooks come later (0022 item 5).
    v.push(raw(""), { text: `위키 스킬 ${c.wikiSkills.length}개 (목록만)`, bold: true });
    for (const e of c.wikiSkills) {
      const archived = e.status === "archived" ? "  (보관됨)" : "";
      v.push(raw(`  ${e.id}  ${display(e.title)}  (type ${e.pageType})${archived}`));
      v.push(raw(`      ${e.uuid}`));
    }
    v.push(raw(""), { text: `문서 ${c.docs.length}개 (목록만)`, bold: true });
    for (const e of c.docs) {
      v.push(raw(`  ${e.id}  ${display(e.title)}  → ${e.filename}`));
      v.push(raw(`      ${e.url}`));
    }
    v.push(raw(""), { text: `훅 ${c.hooks.length}개 (참고용)`, bold: true });
    for (const e of c.hooks) {
      v.push(raw(`  ${e.id}  ${display(e.title)}${e.summary ? `  ${display(e.summary)}` : ""}`));
      if (e.url) v.push(raw(`      ${e.url}`));
    }
    return v;
  }

  private installLines(): Line[] {
    const e = this.installEntry();
    if (!e) return [raw("선택한 항목이 없습니다.")];
    const v: Line[] = [{ text: `${e.id}  ${display(e.title)}`, bold: true }];
    v.push(raw(`출처: ${e.source}`));
    if (e.summary) v.push(raw(`설명: ${display(e.summary)}`));
    if (e.version) v.push(raw(`버전(표시용): ${e.version}`));
    v.push(raw(""));
    const args = install.args(e);
    if (!args) {
      v.push(raw(`설치 도구 \`${e.installer}\` 항목은 TUI가 실행하지 않습니다. 직접 실행하세요:`));
      v.push(raw(`  ${e.install ?? ""}`));
      return v;
    }
    v.push(raw("실행할 명령 (셸 없이, 프로젝트 루트에서):"));
    v.push(raw(`  ${install.display({ program: this.npx ?? install.NPX, args })}`));
    v.push(raw(""));
    let ready = true;
    if (!this.npx) {
      ready = false;
      v.push(raw(`사용 불가: PATH에서 \`${install.NPX}\`를 찾을 수 없습니다.`));
    }
    const c = this.installConflict(e);
    if (c.kind === "none") {
      v.push(raw("이름 충돌: 없음"));
    } else if (c.kind === "with") {
      ready = false;
      v.push(raw(`이름 충돌: 하네스 스킬 \`${c.name}\`과 같은 이름이라 설치하지 않습니다.`));
    } else {
      v.push(
        raw(
          "경고: 모든 스킬을 설치하므로 이름 충돌 검사를 할 수 없습니다 (사용 불가). 하네스 스킬과 이름이 겹치면 CLI가 덮어쓸 수 있습니다.",
        ),
      );
    }
    v.push(raw(""));
    if (ready) {
      v.push(raw("Enter를 누르면 TUI 화면을 잠시 닫고 터미널을 skills CLI에 넘깁니다."));
      v.push(raw("대상 에이전트, project·user 범위, 스킬 선택은 CLI가 직접 묻습니다."));
      v.push(raw("설치 위치와 파일은 skills CLI가 관리하며, 하네스는 소유하지 않습니다."));
    }
    const r = this.installResult;
    if (r?.ok) {
      v.push(raw(`결과: ${exitText(r.value)}`));
    } else if (r) {
      v.push(raw(`결과: 실행하지 못함 (${r.error})`));
    }
    return v;
  }
}

async function settle<T>(p: Promise<T>): Promise<Result<T>> {
  try {
    return { ok: true, value: await p };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
}

const raw = (text: string): Line => ({ text });
const textLines = (s: string) =>
  s
    .split("\n")
    .filter((l) => l !== "")
    .map(raw);

function detectionText(r: AgentRow): string {
  const d = r.detection;
  if (!d) return "감지 중…";
  if (d.kind === "installed") return `설치됨${d.version ? ` (${d.version})` : ""}`;
  return `미설치 (${d.reason})`;
}

const exitText = (code: number | null) =>
  code === 0
    ? "성공 (종료 코드 0)"
    : code === null
      ? "실패 (신호로 종료됨)"
      : `실패 (종료 코드 ${code})`;

function gitLines(g: GitReport): Line[] {
  return [
    ...(g.unavailable ? [raw(`Git: ${g.unavailable}`)] : []),
    ...g.excludedShared.map((e) => raw(`Git 경고: 공유 경로가 제외됨 ${e}`)),
    ...g.trackedIgnored.map((e) => raw(`Git 경고: 이미 추적 중 ${e}`)),
    ...g.notes.map((e) => raw(`Git 안내: ${e}`)),
  ];
}

const check = (on: boolean) => (on ? "[x]" : "[ ]");
const input = (value: string, focused: boolean) => (focused ? `${value}▏` : value);
const placementName = (p: Placement) => (p === "copy" ? "복사" : "참조");
