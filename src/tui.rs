//! Six-step wizard (Stage 1 spec §7, instruction 0010). Korean UI text.
//!
//! `App` is a plain state machine: `handle_key` changes state and `render`
//! only draws it, so rendering never touches the network or disk. Network
//! access (skill fetch) happens only when the user activates a button.
//! Nothing is written before the explicit apply on the preview step.

use crate::catalog::{
    Catalog, CatalogError, Catalogs, Origin, SKILLS_CLI, Status, Target, WikiEntry,
};
use crate::contract::{
    AgentConfig, AgentId, DEFAULT_TOKEN_ENV, HarnessConfig, Placement, SkillConfig, SourceConfig,
    SourceKind, WIKI_GATEWAY_URL,
};
use crate::init::{self, Loaded, PendingImport, Prepared};
use crate::install::{self, Conflict, NPX, RunRequest};
use crate::local::{self, Secret};
use crate::paths::{HiddenAttr, Layout, ProjectRoot, RelPath};
use crate::placement::{Method, agent_name, methods, skills_dir};
use crate::sources::{CatalogEntry, LocalDir, Package, SkillSource, WikiGateway, slugify};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout as Split, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Root,
    Paths,
    Agents,
    Skills,
    Preview,
    Done,
    /// View of the three catalogs, opened from the skills step. Public
    /// skill entries can be selected for install.
    Catalog,
    /// Confirmation screen for one public skill install.
    Install,
}

const STEPS: [(Step, &str); 6] = [
    (Step::Root, "루트·설정"),
    (Step::Paths, "경로·백로그"),
    (Step::Agents, "에이전트·배치"),
    (Step::Skills, "소스·스킬"),
    (Step::Preview, "미리보기"),
    (Step::Done, "적용·결과"),
];

/// Rows on the skills step.
const SK_KIND: usize = 0;
const SK_LOCATION: usize = 1;
const SK_TOKEN: usize = 2;
const SK_TYPE: usize = 3;
const SK_REF: usize = 4;
const SK_FETCH: usize = 5;
const SK_NAME: usize = 6;
const SK_PARTIAL: usize = 7;
const SK_ADD: usize = 8;
const SK_LIST: usize = 9;
const SK_CATALOGS: usize = 10;
const SK_FIXED_ROWS: usize = 11;

pub struct App {
    pub root: ProjectRoot,
    loaded: Result<Loaded, String>,
    pub step: Step,
    focus: usize,
    scroll: u16,
    message: Option<String>,
    pub quit: bool,
    // step 2
    instructions: String,
    references: String,
    output: String,
    backlog: bool,
    // step 3
    agents: Vec<(AgentId, bool, Placement)>,
    // step 4
    wiki: bool,
    local_dir: String,
    token_env: String,
    /// Token typed in this session; never rendered.
    token: String,
    /// Token from `.harness/local.toml`, or why it could not be read.
    stored_token: Result<Option<Secret>, String>,
    gateway_url: String,
    catalogs: Catalogs,
    page_type: String,
    reference: String,
    skill_name: String,
    accept_partial: bool,
    catalog: Vec<CatalogEntry>,
    fetched: Option<Package>,
    imports: Vec<PendingImport>,
    /// Source fields are fixed once a skill from that source was added.
    local_locked: Option<RelPath>,
    // steps 5-6 and the initialized summary
    prepared: Option<Result<Prepared, String>>,
    result: Option<Result<(crate::apply::ApplyReport, HiddenAttr), String>>,
    /// Outcome of saving the typed token after a successful apply.
    token_saved: Option<Result<(), String>>,
    /// `npx` found on `PATH`, if any.
    pub npx: Option<std::path::PathBuf>,
    /// Index of the skills catalog entry on the install screen.
    install_pick: usize,
    /// Confirmed install waiting for the terminal handover (see `take_run`).
    run_request: Option<RunRequest>,
    /// Exit code of the last install, or why it could not run.
    install_result: Option<Result<Option<i32>, String>>,
}

impl App {
    pub fn new(root: ProjectRoot) -> Self {
        let loaded = init::load(&root);
        let catalogs = Catalogs::load(root.path());
        let stored_token = local::load_token(root.path());
        let mut app = Self {
            root,
            loaded,
            step: Step::Root,
            focus: 0,
            scroll: 0,
            message: None,
            quit: false,
            instructions: Layout::default().instructions.to_string(),
            references: Layout::default().references.to_string(),
            output: Layout::default().output.to_string(),
            backlog: false,
            agents: vec![
                (AgentId::ClaudeCode, true, Placement::Copy),
                (AgentId::Codex, false, Placement::Reference),
            ],
            wiki: false,
            local_dir: String::new(),
            token_env: DEFAULT_TOKEN_ENV.into(),
            token: String::new(),
            stored_token,
            gateway_url: WIKI_GATEWAY_URL.into(),
            catalogs,
            page_type: "skill".into(),
            reference: String::new(),
            skill_name: String::new(),
            accept_partial: false,
            catalog: Vec::new(),
            fetched: None,
            imports: Vec::new(),
            local_locked: None,
            prepared: None,
            result: None,
            token_saved: None,
            npx: install::find_program(NPX, std::env::var_os("PATH")),
            install_pick: 0,
            run_request: None,
            install_result: None,
        };
        // Already initialized: summary and a read-only verification only.
        if let Ok(Loaded {
            config: Some(config),
            ..
        }) = &app.loaded
        {
            let config = config.clone();
            let loaded = app.loaded.as_ref().expect("checked");
            app.prepared = Some(init::prepare(&app.root, loaded, &config, &[]));
        }
        app
    }

    fn initialized(&self) -> bool {
        matches!(
            &self.loaded,
            Ok(Loaded {
                config: Some(_),
                ..
            })
        )
    }

    // ------------------------------------------------------------ keys

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true; // cancel: nothing has been written before apply
            return;
        }
        match key.code {
            // Esc on the first step and `q` outside text fields quit, like Ctrl+C.
            KeyCode::Esc if self.step == Step::Root => self.quit = true,
            KeyCode::Char('q') if !self.focus_is_input() => self.quit = true,
            KeyCode::Esc => self.back(),
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Char(' ') if !self.focus_is_input() => self.toggle(),
            KeyCode::Enter => self.enter(),
            KeyCode::Backspace => {
                if let Some(s) = self.input_mut() {
                    s.pop();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(s) = self.input_mut() {
                    s.push(c);
                }
            }
            _ => {}
        }
    }

    fn rows(&self) -> usize {
        match self.step {
            Step::Paths => 4,
            Step::Agents => 4,
            Step::Skills => SK_FIXED_ROWS + self.catalog.len(),
            Step::Catalog => self.catalogs.skills.as_ref().map_or(0, |c| c.entries.len()),
            _ => 0,
        }
    }

    fn move_focus(&mut self, delta: i32) {
        let rows = self.rows();
        if rows == 0 {
            // Scrollable text steps.
            self.scroll = if delta < 0 {
                self.scroll.saturating_sub(1)
            } else {
                self.scroll.saturating_add(1)
            };
            return;
        }
        self.focus = (self.focus as i32 + delta).rem_euclid(rows as i32) as usize;
    }

    fn focus_is_input(&self) -> bool {
        match self.step {
            Step::Paths => self.focus <= 2,
            Step::Skills => matches!(
                self.focus,
                SK_LOCATION | SK_TOKEN | SK_TYPE | SK_REF | SK_NAME
            ),
            _ => false,
        }
    }

    fn input_mut(&mut self) -> Option<&mut String> {
        match (self.step, self.focus) {
            (Step::Paths, 0) => Some(&mut self.references),
            (Step::Paths, 1) => Some(&mut self.output),
            (Step::Paths, 2) => Some(&mut self.instructions),
            (Step::Skills, SK_LOCATION) if self.wiki => Some(&mut self.token_env),
            (Step::Skills, SK_LOCATION) if self.local_locked.is_none() => Some(&mut self.local_dir),
            (Step::Skills, SK_TOKEN) if self.wiki => Some(&mut self.token),
            (Step::Skills, SK_TYPE) if self.wiki => Some(&mut self.page_type),
            (Step::Skills, SK_REF) => Some(&mut self.reference),
            (Step::Skills, SK_NAME) => Some(&mut self.skill_name),
            _ => None,
        }
    }

    fn toggle(&mut self) {
        match (self.step, self.focus) {
            (Step::Paths, 3) => self.backlog = !self.backlog,
            (Step::Agents, row) => {
                let a = &mut self.agents[row / 2];
                if row % 2 == 0 {
                    a.1 = !a.1;
                } else {
                    a.2 = match a.2 {
                        Placement::Copy => Placement::Reference,
                        Placement::Reference => Placement::Copy,
                    };
                }
            }
            (Step::Skills, SK_KIND) => {
                self.wiki = !self.wiki;
                self.catalog.clear();
                self.fetched = None;
            }
            (Step::Skills, SK_PARTIAL) => self.accept_partial = !self.accept_partial,
            _ => {}
        }
    }

    fn back(&mut self) {
        self.message = None;
        self.focus = 0;
        self.scroll = 0;
        self.step = match self.step {
            Step::Root | Step::Done => return,
            Step::Paths => Step::Root,
            Step::Agents => Step::Paths,
            Step::Skills => Step::Agents,
            Step::Preview => {
                self.prepared = None;
                Step::Skills
            }
            Step::Catalog => {
                self.focus = SK_CATALOGS;
                Step::Skills
            }
            Step::Install => {
                self.focus = self.install_pick;
                Step::Catalog
            }
        };
    }

    fn go(&mut self, step: Step) {
        self.step = step;
        self.focus = 0;
        self.scroll = 0;
        self.message = None;
    }

    fn enter(&mut self) {
        match self.step {
            Step::Root => {
                if self.initialized() || self.loaded.is_err() {
                    self.quit = true;
                } else {
                    self.go(Step::Paths);
                }
            }
            Step::Paths => match self.layout() {
                Ok(_) => self.go(Step::Agents),
                Err(e) => self.message = Some(e),
            },
            Step::Agents => {
                if self.agents.iter().any(|a| a.1) {
                    self.go(Step::Skills);
                } else {
                    self.message = Some("에이전트를 하나 이상 선택하세요.".into());
                }
            }
            Step::Skills => match self.focus {
                SK_FETCH => self.fetch(),
                SK_ADD => self.add_import(),
                SK_LIST => self.load_catalog(),
                SK_CATALOGS => self.go(Step::Catalog),
                n if n >= SK_FIXED_ROWS => self.pick_catalog(n - SK_FIXED_ROWS),
                _ => self.open_preview(),
            },
            Step::Preview => self.apply(),
            Step::Done => self.quit = true,
            Step::Catalog if self.rows() > 0 => {
                self.install_pick = self.focus;
                self.install_result = None;
                self.go(Step::Install);
            }
            Step::Catalog => self.back(),
            Step::Install => self.confirm_install(),
        }
    }

    /// Skill names the harness deployed (lock) or selected in this session.
    fn harness_skill_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.imports.iter().map(|i| i.skill_id.clone()).collect();
        if let Ok(l) = &self.loaded {
            names.extend(l.lock.skills.iter().map(|s| s.id.clone()));
        }
        names
    }

    fn install_entry(&self) -> Option<&crate::catalog::SkillsEntry> {
        self.catalogs
            .skills
            .as_ref()
            .ok()?
            .entries
            .get(self.install_pick)
    }

    fn install_conflict(&self, e: &crate::catalog::SkillsEntry) -> Conflict {
        let names = self.harness_skill_names();
        install::conflict(e, names.iter().map(String::as_str))
    }

    /// Queues the install; nothing runs without this explicit confirmation.
    fn confirm_install(&mut self) {
        let Some(e) = self.install_entry() else {
            return;
        };
        let Some(args) = install::args(e) else {
            self.message =
                Some("이 항목은 TUI가 실행하지 않습니다. 안내된 명령을 직접 실행하세요.".into());
            return;
        };
        if let Conflict::With(name) = self.install_conflict(e) {
            self.message = Some(format!(
                "하네스 스킬 `{name}`과 이름이 겹쳐 설치하지 않습니다."
            ));
            return;
        }
        let Some(program) = self.npx.clone() else {
            self.message = Some(format!("`{NPX}`를 찾을 수 없어 설치할 수 없습니다."));
            return;
        };
        self.run_request = Some(RunRequest { program, args });
        self.message = None;
    }

    /// The confirmed install, for the caller to run with the terminal
    /// handed over (`install::run_interactive`).
    pub fn take_run(&mut self) -> Option<RunRequest> {
        self.run_request.take()
    }

    /// Records the outcome of a run returned by `take_run`.
    pub fn finish_run(&mut self, result: Result<Option<i32>, String>) {
        self.install_result = Some(result);
    }

    // --------------------------------------------------------- actions

    fn layout(&self) -> Result<Layout, String> {
        let references = self
            .root
            .resolve(&self.references)
            .map_err(|e| format!("참조 문서 경로: {e}"))?;
        let output = self
            .root
            .resolve(&self.output)
            .map_err(|e| format!("산출물 경로: {e}"))?;
        let instructions = self
            .root
            .resolve(&self.instructions)
            .map_err(|e| format!("지시서 경로: {e}"))?;
        let layout = Layout {
            instructions,
            references,
            output,
            backlog_enabled: self.backlog,
        };
        layout.validate(&self.root).map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        Ok(layout)
    }

    fn local_source(&self) -> Result<(RelPath, LocalDir), String> {
        let rel = match &self.local_locked {
            Some(rel) => rel.clone(),
            None => self
                .root
                .resolve(&self.local_dir)
                .map_err(|e| format!("로컬 폴더는 프로젝트 안에 있어야 합니다: {e}"))?,
        };
        let dir = self.root.abs(&rel);
        Ok((rel, LocalDir { dir }))
    }

    fn gateway(&self) -> WikiGateway {
        let stored = self.stored_token.as_ref().ok().cloned().flatten();
        WikiGateway {
            base_url: self.gateway_url.clone(),
            stored_token: Secret::new(&self.token).or(stored),
            ..WikiGateway::new(self.token_env.trim())
        }
    }

    /// Import state of a wiki catalog entry.
    fn wiki_state(&self, e: &WikiEntry) -> &'static str {
        if e.status == Status::Archived {
            return "보관됨";
        }
        if e.target != Some(Target::Skill) {
            return "목록만";
        }
        let in_lock = self.loaded.as_ref().is_ok_and(|l| {
            l.lock
                .skills
                .iter()
                .any(|s| s.reference.eq_ignore_ascii_case(&e.uuid))
        });
        if in_lock
            || self
                .imports
                .iter()
                .any(|i| i.reference.eq_ignore_ascii_case(&e.uuid))
        {
            return "가져옴";
        }
        if self.root.abs(&Layout::skills().join(&e.id)).exists() {
            return "충돌";
        }
        "사용 가능"
    }

    fn page_type(&self) -> Option<String> {
        let t = self.page_type.trim();
        (self.wiki && !t.is_empty()).then(|| t.to_string())
    }

    fn load_catalog(&mut self) {
        if self.wiki {
            match &self.catalogs.wiki {
                Ok(c) => {
                    self.catalog = c
                        .entries
                        .iter()
                        .map(|e| CatalogEntry {
                            reference: e.uuid.to_lowercase(),
                            title: e.title.display().to_string(),
                            page_type: Some(e.page_type.clone()),
                            name: Some(e.id.clone()),
                            blocked: match self.wiki_state(e) {
                                "보관됨" => Some("보관된 항목은 가져올 수 없습니다".into()),
                                "목록만" => Some("목록 전용 항목입니다".into()),
                                _ => None,
                            },
                        })
                        .collect();
                    self.message = Some(format!(
                        "위키 카탈로그 {}개 항목 ({})",
                        c.entries.len(),
                        catalog_source(c)
                    ));
                }
                Err(e) => {
                    self.catalog.clear();
                    self.message = Some(format!("위키 카탈로그 오류: {}", error_text(e)));
                }
            }
            return;
        }
        let listed = {
            match self.local_source() {
                Ok((_, src)) => src.list(),
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            }
        };
        match listed {
            Ok(list) => {
                self.message = Some(if list.is_empty() {
                    "목록이 비어 있습니다.".into()
                } else {
                    format!("{}개 항목", list.len())
                });
                self.catalog = list;
            }
            Err(e) => self.message = Some(format!("목록 오류: {e}")),
        }
    }

    fn pick_catalog(&mut self, i: usize) {
        if let Some(e) = self.catalog.get(i) {
            if let Some(why) = &e.blocked {
                self.message = Some(format!("`{}`: {why}.", e.title));
                return;
            }
            self.reference = e.reference.clone();
            if let Some(t) = &e.page_type {
                self.page_type = t.clone();
            }
            self.skill_name = e.name.clone().unwrap_or_else(|| slugify(&e.title));
            self.fetched = None;
            self.focus = SK_FETCH;
            self.message = Some(format!("`{}` 선택됨. [조회]로 내용을 가져오세요.", e.title));
        }
    }

    fn fetch(&mut self) {
        let reference = self.reference.trim().to_string();
        let result = if self.wiki {
            self.gateway()
                .fetch(&reference, self.page_type().as_deref())
        } else {
            match self.local_source() {
                Ok((_, src)) => src.fetch(&reference, None),
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            }
        };
        match result {
            Ok(pkg) => {
                if self.skill_name.trim().is_empty() {
                    self.skill_name = slugify(pkg.title.as_deref().unwrap_or(&reference));
                }
                self.message = Some(format!("파일 {}개를 가져왔습니다.", pkg.files.len()));
                self.fetched = Some(pkg);
            }
            Err(e) => {
                self.fetched = None;
                self.message = Some(format!("조회 오류: {e}"));
            }
        }
    }

    fn add_import(&mut self) {
        let Some(pkg) = self.fetched.clone() else {
            self.message = Some("먼저 [조회]로 내용을 가져오세요.".into());
            return;
        };
        let name = self.skill_name.trim().to_string();
        if !crate::contract::is_slug(&name) {
            self.message = Some("스킬 이름은 영문·숫자·`.`·`_`·`-`만 쓸 수 있습니다.".into());
            return;
        }
        if self
            .imports
            .iter()
            .any(|i| i.skill_id.eq_ignore_ascii_case(&name))
        {
            self.message = Some(format!("`{name}`은 이미 선택 목록에 있습니다."));
            return;
        }
        if !pkg.missing_locales.is_empty() && !self.accept_partial {
            self.message = Some(format!(
                "빠진 언어: {}. 부분 가져오기를 수락해야 추가할 수 있습니다.",
                pkg.missing_locales.join(", ")
            ));
            return;
        }
        let source_id = if self.wiki {
            "wiki"
        } else {
            match self.local_source() {
                Ok((rel, _)) => {
                    self.local_locked = Some(rel);
                    "local"
                }
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            }
        };
        self.imports.push(PendingImport {
            skill_id: name.clone(),
            source_id: source_id.into(),
            reference: self.reference.trim().into(),
            page_type: self.page_type(),
            accept_partial: self.accept_partial,
            package: pkg,
        });
        self.fetched = None;
        self.reference.clear();
        self.skill_name.clear();
        self.message = Some(format!("`{name}`을 선택 목록에 추가했습니다."));
    }

    fn config(&self) -> Result<HarnessConfig, String> {
        let layout = self.layout()?;
        let mut config = HarnessConfig::default();
        config.backlog.path = layout.backlog();
        config.paths.instructions = layout.instructions;
        config.paths.references = layout.references;
        config.paths.output = layout.output;
        config.backlog.enabled = self.backlog;
        config.agents = self
            .agents
            .iter()
            .filter(|a| a.1)
            .map(|a| AgentConfig {
                id: a.0,
                placement: a.2,
            })
            .collect();
        if let Some(rel) = &self.local_locked {
            config.sources.push(SourceConfig {
                id: "local".into(),
                kind: SourceKind::LocalDir,
                path: Some(rel.clone()),
                token_env: None,
            });
        }
        if self.imports.iter().any(|i| i.source_id == "wiki") {
            config.sources.push(SourceConfig {
                id: "wiki".into(),
                kind: SourceKind::WikiGateway,
                path: None,
                token_env: Some(self.token_env.trim().into()),
            });
        }
        config.skills = self
            .imports
            .iter()
            .map(|i| SkillConfig {
                id: i.skill_id.clone(),
                source: i.source_id.clone(),
                reference: i.reference.clone(),
                page_type: i.page_type.clone(),
            })
            .collect();
        Ok(config)
    }

    fn open_preview(&mut self) {
        let prepared = match (self.config(), &self.loaded) {
            (Ok(config), Ok(loaded)) => init::prepare(&self.root, loaded, &config, &self.imports),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e.clone()),
        };
        self.prepared = Some(prepared);
        self.go(Step::Preview);
    }

    fn apply(&mut self) {
        let Some(Ok(prepared)) = &self.prepared else {
            self.message =
                Some("계획에 오류가 있어 적용할 수 없습니다. Esc로 돌아가 수정하세요.".into());
            return;
        };
        if prepared.plan.conflicts().count() > 0 {
            self.message =
                Some("충돌을 해결한 뒤 다시 시도하세요. 아무것도 쓰지 않았습니다.".into());
            return;
        }
        let Ok(loaded) = &mut self.loaded else {
            return;
        };
        let result = init::apply_prepared(&self.root, prepared, &mut loaded.state)
            .map_err(|e| e.to_string());
        // The typed token is kept only after a successful apply.
        let applied = matches!(&result, Ok((r, _)) if r.failed.is_none());
        let stored = self.stored_token.as_ref().ok().cloned().flatten();
        if let Some(token) =
            Secret::new(&self.token).filter(|t| applied && Some(t) != stored.as_ref())
        {
            self.token_saved = Some(local::save_token(self.root.path(), &token));
        }
        self.result = Some(result);
        self.go(Step::Done);
    }

    // ---------------------------------------------------------- render

    pub fn render(&self, f: &mut Frame) {
        let area = f.area();
        let [body, footer] =
            Split::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(area);
        let title = format!(" agent-harness-tui ─ {} ", self.root.path().display());
        if area.width >= 80 {
            let [side, main] =
                Split::horizontal([Constraint::Length(18), Constraint::Min(10)]).areas(body);
            f.render_widget(
                Paragraph::new(self.step_list()).block(Block::bordered().title(title)),
                side,
            );
            self.render_main(f, main);
        } else {
            let [head, main] =
                Split::vertical([Constraint::Length(1), Constraint::Min(2)]).areas(body);
            let i = self.step_index();
            f.render_widget(Paragraph::new(format!("{}/6 {}", i + 1, STEPS[i].1)), head);
            self.render_main(f, main);
        }
        f.render_widget(
            Paragraph::new(self.keys())
                .wrap(Wrap { trim: true })
                .block(Block::bordered()),
            footer,
        );
    }

    /// Sidebar position; the catalog view belongs to the skills step.
    fn step_index(&self) -> usize {
        let shown = match self.step {
            Step::Catalog | Step::Install => Step::Skills,
            s => s,
        };
        STEPS.iter().position(|(s, _)| *s == shown).unwrap_or(0)
    }

    fn step_list(&self) -> Vec<Line<'static>> {
        let current = self.step_index();
        STEPS
            .iter()
            .enumerate()
            .map(|(i, (_, name))| {
                let text = format!("{} {} {name}", if i == current { "▸" } else { " " }, i + 1);
                if i == current {
                    Line::styled(text, Style::new().add_modifier(Modifier::BOLD))
                } else {
                    Line::raw(text)
                }
            })
            .collect()
    }

    fn keys(&self) -> String {
        match self.step {
            Step::Root if self.initialized() || self.loaded.is_err() => {
                "↑↓ 스크롤  Enter·Esc·q 종료".into()
            }
            Step::Root => "Enter 다음  Esc·q 종료".into(),
            Step::Preview => "↑↓ PgUp PgDn 스크롤  Enter 적용  Esc 이전  q·Ctrl+C 취소".into(),
            Step::Done => "↑↓ 스크롤  Enter·q 종료".into(),
            Step::Skills => {
                "↑↓ 이동  Space 선택  Enter 버튼 실행·다음  Esc 이전  Ctrl+C 취소".into()
            }
            Step::Catalog if self.rows() > 0 => {
                "↑↓ 공개 스킬 선택  PgUp PgDn 스크롤  Enter 설치 확인 화면  Esc 돌아가기".into()
            }
            Step::Catalog => "PgUp PgDn 스크롤  Enter·Esc 돌아가기  q 종료".into(),
            Step::Install => "Enter 확인·실행  Esc 돌아가기  q 종료".into(),
            _ => "↑↓ 이동  Space 선택  Enter 다음  Esc 이전  Ctrl+C 취소".into(),
        }
    }

    fn render_main(&self, f: &mut Frame, area: Rect) {
        let i = self.step_index();
        let mut lines = match self.step {
            Step::Root => self.root_lines(),
            Step::Paths => self.paths_lines(),
            Step::Agents => self.agent_lines(),
            Step::Skills => self.skill_lines(),
            Step::Preview => self.preview_lines(),
            Step::Done => self.result_lines(),
            Step::Catalog => self.catalog_lines(),
            Step::Install => self.install_lines(),
        };
        if let Some(m) = &self.message {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                m.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ));
        }
        let title = if self.step == Step::Catalog {
            format!(" {}. {} ─ 카탈로그 ", i + 1, STEPS[i].1)
        } else if self.step == Step::Install {
            format!(" {}. {} ─ 공개 스킬 설치 ", i + 1, STEPS[i].1)
        } else {
            format!(" {}. {} ", i + 1, STEPS[i].1)
        };
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(Block::bordered().title(title)),
            area,
        );
    }

    fn row(&self, n: usize, text: String) -> Line<'static> {
        if self.focus == n && self.rows() > 0 {
            Line::styled(
                format!("▸ {text}"),
                Style::new().add_modifier(Modifier::REVERSED),
            )
        } else {
            Line::raw(format!("  {text}"))
        }
    }

    fn root_lines(&self) -> Vec<Line<'static>> {
        let mut v = vec![
            Line::raw(format!("프로젝트 루트: {}", self.root.path().display())),
            Line::raw(
                "현재 실행 디렉터리를 루트로 사용합니다. 상위 폴더를 찾거나 Git을 초기화하지 않습니다.",
            ),
        ];
        v.push(Line::raw(""));
        match &self.loaded {
            Err(e) => {
                v.push(Line::raw(format!("기존 파일을 읽을 수 없습니다: {e}")));
                v.push(Line::raw("파일을 변경하지 않았습니다."));
            }
            Ok(Loaded { config: None, .. }) => {
                v.push(Line::raw(
                    "하네스 설정이 없습니다. Enter로 새로 초기화를 시작합니다.",
                ));
            }
            Ok(Loaded {
                config: Some(c),
                state,
                lock,
            }) => {
                v.push(Line::raw(
                    "이미 초기화된 프로젝트입니다. 요약과 무변경 검증만 제공합니다.",
                ));
                v.push(Line::raw(
                    "변경(스킬 추가·갱신 등)은 2단계 업데이트 기능에서 다룹니다.",
                ));
                v.push(Line::raw(""));
                v.push(Line::raw(format!(
                    "참조 문서: {}/   산출물: {}/",
                    c.paths.references, c.paths.output
                )));
                v.push(Line::raw(format!(
                    "백로그: {}",
                    if c.backlog.enabled {
                        "사용"
                    } else {
                        "사용 안 함"
                    }
                )));
                for a in &c.agents {
                    v.push(Line::raw(format!(
                        "에이전트: {} ({})",
                        agent_name(a.id),
                        placement_name(a.placement)
                    )));
                }
                for s in &lock.skills {
                    v.push(Line::raw(format!(
                        "스킬: {} (소스 {}, 파일 {}개)",
                        s.id,
                        s.source,
                        s.files.len()
                    )));
                }
                match &state.last_success {
                    Some(r) => v.push(Line::raw(format!("마지막 적용: {} (UTC)", r.at))),
                    None => v.push(Line::raw(
                        "로컬 적용 기록이 없습니다 (clone 직후일 수 있음).",
                    )),
                }
                v.push(Line::raw(""));
                match &self.prepared {
                    Some(Ok(p)) => {
                        let conflicts = p.plan.conflicts().count();
                        if !p.plan.has_writes() && conflicts == 0 {
                            v.push(Line::raw(
                                "검증: 설정과 파일이 일치합니다. 변경할 항목이 없습니다.",
                            ));
                        } else {
                            v.push(Line::raw(format!(
                                "검증: 설정과 다른 항목이 있습니다 (충돌 {conflicts}개). 이 화면에서는 적용하지 않습니다."
                            )));
                            v.extend(p.plan.to_string().lines().map(|l| Line::raw(l.to_string())));
                        }
                    }
                    Some(Err(e)) => v.push(Line::raw(format!("검증 오류: {e}"))),
                    None => {}
                }
            }
        }
        v
    }

    fn paths_lines(&self) -> Vec<Line<'static>> {
        vec![
            self.row(
                0,
                format!(
                    "참조 문서 경로: {}",
                    input(&self.references, self.focus == 0)
                ),
            ),
            self.row(
                1,
                format!(
                    "산출물 경로:   {}  (이름은 점으로 시작)",
                    input(&self.output, self.focus == 1)
                ),
            ),
            self.row(
                2,
                format!(
                    "지시서 경로:   {}  (.completed/·completed.log 포함)",
                    input(&self.instructions, self.focus == 2)
                ),
            ),
            self.row(3, format!("백로그 agent.db: {}", check(self.backlog))),
            Line::raw(""),
            Line::raw("고정 경로 (1단계에서 변경 불가):"),
            Line::raw(format!("  {}/  스킬 원본", Layout::skills())),
            Line::raw(format!(
                "  {}/agent.db·agent-db.sh·agent-db.ps1  백로그 (지시서 경로를 따름)",
                self.instructions.trim_end_matches('/')
            )),
        ]
    }

    fn agent_lines(&self) -> Vec<Line<'static>> {
        let mut v = Vec::new();
        for (i, (id, on, placement)) in self.agents.iter().enumerate() {
            v.push(self.row(i * 2, format!("{} {}", check(*on), agent_name(*id))));
            let target = match placement {
                Placement::Copy => format!("복사 → {}/<id>/", skills_dir(*id)),
                Placement::Reference => "참조 (지침에서 .skills/ 안내)".into(),
            };
            v.push(self.row(i * 2 + 1, format!("    배치: {target}")));
        }
        v.push(Line::raw(""));
        for (m, avail) in methods(AgentId::ClaudeCode) {
            if let (Method::Symlink, Err(why)) = (m, avail) {
                v.push(Line::raw(format!("심볼릭 링크 배치: 선택 불가 ({why})")));
            }
        }
        v.push(Line::raw(
            "에이전트는 지침 경로만으로 .skills/를 자동으로 찾지 않습니다.",
        ));
        v
    }

    fn skill_lines(&self) -> Vec<Line<'static>> {
        let f = self.focus;
        let kind = if self.wiki {
            "위키 게이트웨이 (md.zerotymer.net)"
        } else {
            "로컬 폴더 (프로젝트 안)"
        };
        let location = if self.wiki {
            format!(
                "토큰 환경변수 이름(선택): {}",
                input(&self.token_env, f == SK_LOCATION)
            )
        } else if let Some(rel) = &self.local_locked {
            format!("로컬 폴더: {rel}  (고정됨)")
        } else {
            format!("로컬 폴더: {}", input(&self.local_dir, f == SK_LOCATION))
        };
        let token = if self.wiki {
            // The value itself is never drawn.
            let state = match (&self.stored_token, self.token.trim().is_empty()) {
                (_, false) => "입력됨 (표시하지 않음, 적용 후 .harness/local.toml에 저장)",
                (Ok(Some(_)), true) => "저장된 토큰 사용",
                (Ok(None), true) => "없음 (토큰 없이 진행)",
                (Err(_), true) => "저장된 토큰을 읽지 못함",
            };
            format!(
                "게이트웨이 토큰(선택): {state}{}",
                if f == SK_TOKEN { "▏" } else { "" }
            )
        } else {
            "게이트웨이 토큰: 해당 없음".into()
        };
        let page_type = if self.wiki {
            format!(
                "경로 접두어(type): {}",
                input(&self.page_type, f == SK_TYPE)
            )
        } else {
            "경로 접두어(type): 해당 없음".into()
        };
        let reference = if self.wiki {
            "페이지 UUID"
        } else {
            "스킬 폴더 이름"
        };
        let mut v = vec![
            self.row(SK_KIND, format!("소스: {kind}")),
            self.row(SK_LOCATION, location),
            self.row(SK_TOKEN, token),
            self.row(SK_TYPE, page_type),
            self.row(
                SK_REF,
                format!("{reference}: {}", input(&self.reference, f == SK_REF)),
            ),
            self.row(
                SK_FETCH,
                "[조회] 내용 가져오기 (네트워크는 이때만 사용)".into(),
            ),
            self.row(
                SK_NAME,
                format!(
                    "스킬 이름(.skills/<이름>): {}",
                    input(&self.skill_name, f == SK_NAME)
                ),
            ),
            self.row(
                SK_PARTIAL,
                format!(
                    "{} 한 언어만 있어도 부분 가져오기 수락",
                    check(self.accept_partial)
                ),
            ),
            self.row(SK_ADD, "[추가] 선택 목록에 넣기".into()),
            self.row(
                SK_LIST,
                if self.wiki {
                    "[목록 불러오기] 위키 카탈로그".into()
                } else {
                    "[목록 불러오기]".into()
                },
            ),
            self.row(
                SK_CATALOGS,
                "[카탈로그 보기] 위키·스킬·문서의 출처·버전·상태".into(),
            ),
        ];
        if let (true, Err(e)) = (self.wiki, &self.stored_token) {
            v.push(Line::raw(format!("  토큰 파일 오류: {e}")));
        }
        for (i, e) in self.catalog.iter().enumerate() {
            let note = e
                .blocked
                .as_ref()
                .map(|b| format!(" ({b})"))
                .unwrap_or_default();
            v.push(self.row(
                SK_FIXED_ROWS + i,
                format!("  {}  {}{note}", e.title, e.reference),
            ));
        }
        if let Some(p) = &self.fetched {
            v.push(Line::raw(""));
            v.push(Line::raw(format!(
                "조회 결과: {}",
                p.title.clone().unwrap_or_default()
            )));
            for file in &p.files {
                v.push(Line::raw(format!(
                    "  {} ({} bytes)",
                    file.path,
                    file.bytes.len()
                )));
            }
            if !p.missing_locales.is_empty() {
                v.push(Line::raw(format!(
                    "  빠진 언어: {} (번역을 만들지 않습니다)",
                    p.missing_locales.join(", ")
                )));
            }
            v.extend(p.warnings.iter().map(|w| Line::raw(format!("  경고: {w}"))));
        }
        v.push(Line::raw(""));
        if self.imports.is_empty() {
            v.push(Line::raw(
                "선택한 스킬: 없음 (스킬 없이 진행할 수 있습니다)",
            ));
        } else {
            v.push(Line::raw("선택한 스킬:"));
            for i in &self.imports {
                v.push(Line::raw(format!(
                    "  {} ← {} {}",
                    i.skill_id, i.source_id, i.reference
                )));
            }
        }
        v.push(Line::raw(
            "버튼 밖에서 Enter를 누르면 미리보기로 이동합니다.",
        ));
        v
    }

    fn preview_lines(&self) -> Vec<Line<'static>> {
        let mut v = Vec::new();
        match &self.prepared {
            Some(Ok(p)) => {
                let conflicts = p.plan.conflicts().count();
                v.push(Line::raw(if conflicts == 0 {
                    "아래 변경을 적용합니다. Enter를 눌러야만 파일을 씁니다.".to_string()
                } else {
                    format!("충돌 {conflicts}개: 해결 전에는 적용할 수 없습니다.")
                }));
                v.push(Line::raw(""));
                v.extend(p.plan.to_string().lines().map(|l| Line::raw(l.to_string())));
                v.extend(p.warnings.iter().map(|w| Line::raw(format!("경고: {w}"))));
                v.push(Line::raw(""));
                v.extend(git_lines(&p.git));
            }
            Some(Err(e)) => v.push(Line::raw(format!("계획 오류: {e}"))),
            None => {}
        }
        v
    }

    fn result_lines(&self) -> Vec<Line<'static>> {
        let mut v = Vec::new();
        match &self.result {
            Some(Ok((r, hidden))) => {
                if let Some(e) = &r.failed {
                    v.push(Line::raw(format!("적용 실패: {e}")));
                    v.push(Line::raw("성공 상태를 기록하지 않았습니다."));
                    v.extend(r.restored.iter().map(|p| Line::raw(format!("복구: {p}"))));
                    v.extend(
                        r.residual
                            .iter()
                            .map(|p| Line::raw(format!("복구 못 함: {p}"))),
                    );
                } else {
                    v.push(Line::raw("적용을 완료했습니다."));
                    for (label, list) in [
                        ("생성", &r.created),
                        ("갱신", &r.updated),
                        ("변경 없음", &r.unchanged),
                        ("보존", &r.preserved),
                    ] {
                        v.extend(list.iter().map(|p| Line::raw(format!("{label}: {p}"))));
                    }
                    if let Some(id) = &r.apply_id {
                        v.push(Line::raw(format!("백업 위치: .harness/backups/{id}/")));
                    }
                    if let HiddenAttr::Failed(why) = hidden {
                        v.push(Line::raw(format!("산출물 폴더 숨김 속성 적용 실패: {why}")));
                    }
                }
            }
            Some(Err(e)) => v.push(Line::raw(format!("적용하지 않았습니다: {e}"))),
            None => {}
        }
        match &self.token_saved {
            Some(Ok(())) => v.push(Line::raw(format!(
                "게이트웨이 토큰을 {}에 저장했습니다 (Git 무시 경로).",
                local::LOCAL_FILE
            ))),
            Some(Err(e)) => v.push(Line::raw(format!("게이트웨이 토큰 저장 실패: {e}"))),
            None => {}
        }
        v.push(Line::raw(""));
        v.push(Line::raw("Git 커밋·푸시는 하지 않았습니다."));
        v
    }

    fn catalog_lines(&self) -> Vec<Line<'static>> {
        let mut v = vec![
            Line::raw("카탈로그는 로컬 파일만 읽습니다. 네트워크로 내려받지 않습니다."),
            Line::raw(""),
        ];
        let c = &self.catalogs;
        v.push(catalog_head("위키", &c.wiki));
        if let Ok(w) = &c.wiki {
            for e in &w.entries {
                v.push(Line::raw(format!(
                    "  [{}] {}  {}  (type {})",
                    self.wiki_state(e),
                    e.id,
                    e.title.display(),
                    e.page_type
                )));
            }
        }
        v.push(Line::raw(""));
        v.push(catalog_head("공개 스킬", &c.skills));
        if let Ok(s) = &c.skills {
            for (i, e) in s.entries.iter().enumerate() {
                let state = if e.installer != SKILLS_CLI {
                    "직접 설치"
                } else if self.npx.is_none() {
                    "사용 불가"
                } else if matches!(self.install_conflict(e), Conflict::With(_)) {
                    "충돌"
                } else {
                    "설치 가능"
                };
                v.push(self.row(
                    i,
                    format!(
                        "[{state}] {}  {}  ← {} {}",
                        e.id,
                        e.title.display(),
                        e.installer,
                        e.source
                    ),
                ));
                if e.installer == SKILLS_CLI {
                    let skill = e.skill.as_deref().unwrap_or("*");
                    v.push(Line::raw(format!("      스킬: {skill}")));
                } else if let Some(cmd) = &e.install {
                    v.push(Line::raw(format!("      직접 실행: {cmd}")));
                }
            }
        }
        v.push(Line::raw(""));
        v.push(catalog_head("문서", &c.docs));
        if let Ok(d) = &c.docs {
            for e in &d.entries {
                v.push(Line::raw(format!(
                    "  [목록만] {}  {}  → {}",
                    e.id,
                    e.title.display(),
                    e.filename
                )));
                v.push(Line::raw(format!("      {}", e.url)));
            }
        }
        v
    }
}

impl App {
    fn install_lines(&self) -> Vec<Line<'static>> {
        let Some(e) = self.install_entry() else {
            return vec![Line::raw("선택한 항목이 없습니다.")];
        };
        let mut v = vec![
            Line::styled(
                format!("{}  {}", e.id, e.title.display()),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!("출처: {}", e.source)),
        ];
        if let Some(s) = &e.summary {
            v.push(Line::raw(format!("설명: {}", s.display())));
        }
        if let Some(ver) = &e.version {
            v.push(Line::raw(format!("버전(표시용): {ver}")));
        }
        v.push(Line::raw(""));
        let Some(args) = install::args(e) else {
            v.push(Line::raw(format!(
                "설치 도구 `{}` 항목은 TUI가 실행하지 않습니다. 직접 실행하세요:",
                e.installer
            )));
            v.push(Line::raw(format!(
                "  {}",
                e.install.clone().unwrap_or_default()
            )));
            return v;
        };
        let program = self.npx.clone().unwrap_or_else(|| NPX.into());
        v.push(Line::raw("실행할 명령 (셸 없이, 프로젝트 루트에서):"));
        v.push(Line::raw(format!(
            "  {}",
            RunRequest { program, args }.display()
        )));
        v.push(Line::raw(""));
        let mut ready = true;
        if self.npx.is_none() {
            ready = false;
            v.push(Line::raw(format!(
                "사용 불가: PATH에서 `{NPX}`를 찾을 수 없습니다."
            )));
        }
        match self.install_conflict(e) {
            Conflict::None => v.push(Line::raw("이름 충돌: 없음")),
            Conflict::With(name) => {
                ready = false;
                v.push(Line::raw(format!(
                    "이름 충돌: 하네스 스킬 `{name}`과 같은 이름이라 설치하지 않습니다."
                )));
            }
            Conflict::Unknown => v.push(Line::raw(
                "경고: 모든 스킬을 설치하므로 이름 충돌 검사를 할 수 없습니다 (사용 불가). 하네스 스킬과 이름이 겹치면 CLI가 덮어쓸 수 있습니다.",
            )),
        }
        v.push(Line::raw(""));
        if ready {
            v.push(Line::raw(
                "Enter를 누르면 TUI 화면을 잠시 닫고 터미널을 skills CLI에 넘깁니다.",
            ));
            v.push(Line::raw(
                "대상 에이전트, project·user 범위, 스킬 선택은 CLI가 직접 묻습니다.",
            ));
            v.push(Line::raw(
                "설치 위치와 파일은 skills CLI가 관리하며, 하네스는 소유하지 않습니다.",
            ));
        }
        match &self.install_result {
            Some(Ok(Some(0))) => v.push(Line::raw("결과: 설치 성공 (종료 코드 0)")),
            Some(Ok(Some(code))) => v.push(Line::raw(format!("결과: 실패 (종료 코드 {code})"))),
            Some(Ok(None)) => v.push(Line::raw("결과: 실패 (신호로 종료됨)")),
            Some(Err(err)) => v.push(Line::raw(format!("결과: 실행하지 못함 ({err})"))),
            None => {}
        }
        v
    }
}

fn origin_text(o: &Origin) -> String {
    match o {
        Origin::Env(p) => format!("환경변수 경로 {}", p.display()),
        Origin::BesideExe(p) => format!("실행 파일 옆 {}", p.display()),
        Origin::Builtin => "내장 샘플".into(),
    }
}

fn catalog_source<E>(c: &Catalog<E>) -> String {
    match &c.catalog_version {
        Some(ver) => format!("출처: {}, 버전 {ver}", origin_text(&c.origin)),
        None => format!("출처: {}", origin_text(&c.origin)),
    }
}

fn error_text(e: &CatalogError) -> String {
    format!(
        "{} — {} (이 카탈로그는 사용하지 않습니다)",
        origin_text(&e.origin),
        e.message
    )
}

fn catalog_head<E>(name: &str, c: &Result<Catalog<E>, CatalogError>) -> Line<'static> {
    let text = match c {
        Ok(c) => format!(
            "{name} 카탈로그 ({}) {}개",
            catalog_source(c),
            c.entries.len()
        ),
        Err(e) => format!("{name} 카탈로그 오류: {}", error_text(e)),
    };
    Line::styled(text, Style::new().add_modifier(Modifier::BOLD))
}

fn git_lines(g: &crate::gitignore::GitReport) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    if let Some(why) = &g.unavailable {
        v.push(Line::raw(format!("Git: {why}")));
    }
    v.extend(
        g.excluded_shared
            .iter()
            .map(|e| Line::raw(format!("Git 경고: 공유 경로가 제외됨 {e}"))),
    );
    v.extend(
        g.tracked_ignored
            .iter()
            .map(|e| Line::raw(format!("Git 경고: 이미 추적 중 {e}"))),
    );
    v.extend(g.notes.iter().map(|e| Line::raw(format!("Git 안내: {e}"))));
    v
}

fn check(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

fn input(value: &str, focused: bool) -> String {
    if focused {
        format!("{value}▏")
    } else {
        value.to_string()
    }
}

fn placement_name(p: Placement) -> &'static str {
    match p {
        Placement::Copy => "복사",
        Placement::Reference => "참조",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aht-tui-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn app(&self) -> App {
            App::new(ProjectRoot::new(&self.0).unwrap())
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    fn screen(app: &App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| app.render(f)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..h {
            let mut x = 0;
            while x < w {
                let sym = buf[(x, y)].symbol();
                s.push_str(sym);
                // Wide (e.g. Hangul) symbols occupy the following cell too.
                x += ratatui::text::Span::raw(sym).width().max(1) as u16;
            }
            s.push('\n');
        }
        s
    }

    fn files_in(dir: &PathBuf) -> usize {
        fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn walks_all_steps_and_applies_only_on_enter() {
        let p = TempProject::new();
        let mut app = p.app();
        assert_eq!(app.step, Step::Root);
        for expect in [Step::Paths, Step::Agents, Step::Skills, Step::Preview] {
            key(&mut app, KeyCode::Enter);
            assert_eq!(app.step, expect);
            assert_eq!(files_in(&p.0), 0, "nothing written before apply");
        }
        assert!(screen(&app, 100, 40).contains("create"));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Done);
        assert!(p.0.join("CLAUDE.md").is_file());
        assert!(p.0.join("harness.toml").is_file());
        assert!(screen(&app, 100, 40).contains("적용을 완료했습니다"));
        key(&mut app, KeyCode::Enter);
        assert!(app.quit);
    }

    #[test]
    fn cancel_and_back_never_write() {
        let p = TempProject::new();
        let mut app = p.app();
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.step, Step::Paths);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
        assert_eq!(files_in(&p.0), 0);
    }

    #[test]
    fn esc_on_first_step_and_q_quit_without_writing() {
        let p = TempProject::new();
        let mut app = p.app();
        key(&mut app, KeyCode::Esc);
        assert!(app.quit, "Esc on the first step quits");

        let mut app = p.app();
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit, "q quits outside text fields");

        // On the paths step the first field is a text input: `q` is typed.
        let mut app = p.app();
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Char('q'));
        assert!(!app.quit);
        assert_eq!(app.references, "referencesq");
        // The last row (backlog toggle) is not an input: `q` quits.
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
        assert_eq!(files_in(&p.0), 0);
    }

    #[test]
    fn korean_path_input_and_validation() {
        let p = TempProject::new();
        let mut app = p.app();
        key(&mut app, KeyCode::Enter);
        // Replace "references" with a Korean path.
        for _ in 0.."references".len() {
            key(&mut app, KeyCode::Backspace);
        }
        typed(&mut app, "참조 문서");
        assert!(screen(&app, 100, 30).contains("참조 문서"));
        // Output without a leading dot is rejected and the step stays.
        key(&mut app, KeyCode::Down);
        for _ in 0..".output".len() {
            key(&mut app, KeyCode::Backspace);
        }
        typed(&mut app, "out");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Paths);
        assert!(app.message.as_deref().unwrap().contains("dot-prefixed"));
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Backspace);
        typed(&mut app, ".산출물");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Agents);
    }

    #[test]
    fn custom_instructions_dir_is_validated_and_applied() {
        let p = TempProject::new();
        let mut app = p.app();
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        let clear = |app: &mut App| {
            for _ in 0..app.instructions.chars().count() {
                key(app, KeyCode::Backspace);
            }
        };
        // Nested inside the references directory: rejected.
        clear(&mut app);
        typed(&mut app, "references/tasks");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Paths);
        assert!(app.message.as_deref().unwrap().contains("nested"));
        // Outside the project root: rejected.
        clear(&mut app);
        typed(&mut app, "../tasks");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Paths);
        assert!(app.message.as_deref().unwrap().contains("지시서 경로"));

        clear(&mut app);
        typed(&mut app, "docs/tasks");
        for _ in 0..4 {
            key(&mut app, KeyCode::Enter);
        }
        assert_eq!(app.step, Step::Done);
        assert!(p.0.join("docs/tasks/.completed").is_dir());
        assert!(p.0.join("docs/tasks/completed.log").is_file());
        assert!(!p.0.join("instructions").exists());
        let config = fs::read_to_string(p.0.join("harness.toml")).unwrap();
        assert!(config.contains(r#"instructions = "docs/tasks""#));
        let guide = fs::read_to_string(p.0.join("CLAUDE.md")).unwrap();
        assert!(guide.contains("`docs/tasks/completed.log`"));
    }

    #[test]
    fn backlog_creates_db_and_scripts_and_keeps_existing_db() {
        let p = TempProject::new();
        let run = |p: &TempProject| {
            let mut app = p.app();
            key(&mut app, KeyCode::Enter);
            key(&mut app, KeyCode::Up); // last row: backlog toggle
            key(&mut app, KeyCode::Char(' '));
            assert!(screen(&app, 100, 30).contains("instructions/agent.db"));
            for _ in 0..4 {
                key(&mut app, KeyCode::Enter);
            }
            assert_eq!(app.step, Step::Done);
        };
        run(&p);
        for f in ["agent.db", "agent-db.sh", "agent-db.ps1"] {
            assert!(p.0.join("instructions").join(f).is_file(), "{f}");
        }
        assert!(!p.0.join("agent.db").exists());
        let config = fs::read_to_string(p.0.join("harness.toml")).unwrap();
        assert!(config.contains(r#"path = "instructions/agent.db""#));

        // A fresh project that already has a database keeps it untouched.
        let q = TempProject::new();
        fs::create_dir(q.0.join("instructions")).unwrap();
        fs::write(q.0.join("instructions/agent.db"), b"user data").unwrap();
        run(&q);
        assert_eq!(
            fs::read(q.0.join("instructions/agent.db")).unwrap(),
            b"user data"
        );
        assert!(q.0.join("instructions/agent-db.sh").is_file());
    }

    #[test]
    fn agents_toggle_and_require_one() {
        let p = TempProject::new();
        let mut app = p.app();
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Enter);
        let text = screen(&app, 100, 30);
        assert!(text.contains("심볼릭 링크 배치: 선택 불가"));
        key(&mut app, KeyCode::Char(' ')); // deselect Claude Code
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Agents);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char(' ')); // select Codex
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Skills);
    }

    #[test]
    fn local_skill_import_end_to_end() {
        let p = TempProject::new();
        fs::create_dir_all(p.0.join("vendor/review")).unwrap();
        fs::write(
            p.0.join("vendor/review/SKILL.md"),
            "---\nname: review\n---\n",
        )
        .unwrap();
        let mut app = p.app();
        for _ in 0..3 {
            key(&mut app, KeyCode::Enter);
        }
        assert_eq!(app.step, Step::Skills);
        key(&mut app, KeyCode::Down); // location
        typed(&mut app, "vendor");
        app.focus = SK_LIST;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.catalog.len(), 1);
        app.focus = SK_FIXED_ROWS;
        key(&mut app, KeyCode::Enter); // pick → fills reference and name
        assert_eq!(app.focus, SK_FETCH);
        key(&mut app, KeyCode::Enter); // fetch
        assert!(app.fetched.is_some());
        app.focus = SK_ADD;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.imports.len(), 1);
        app.focus = SK_KIND;
        key(&mut app, KeyCode::Enter); // to preview
        assert_eq!(app.step, Step::Preview);
        key(&mut app, KeyCode::Enter); // apply
        assert!(p.0.join(".skills/review/SKILL.md").is_file());
        assert!(p.0.join(".claude/skills/review/SKILL.md").is_file());
        let toml = fs::read_to_string(p.0.join("harness.toml")).unwrap();
        assert!(toml.contains("path = \"vendor\"") && toml.contains("id = \"review\""));
    }

    fn to_wiki_list(app: &mut App) {
        for _ in 0..3 {
            key(app, KeyCode::Enter);
        }
        assert_eq!(app.step, Step::Skills);
        key(app, KeyCode::Char(' ')); // switch to wiki
        app.focus = SK_LIST;
        key(app, KeyCode::Enter);
    }

    #[test]
    fn wiki_catalog_lists_and_blocks_archived_and_doc_entries() {
        let p = TempProject::new();
        let mut app = p.app();
        let parsed = crate::catalog::parse::<WikiEntry>(
            r#"{"schema_version":1,"kind":"wiki","catalog_version":"t1","entries":[
              {"id":"live","uuid":"14CEA96F-F1E2-461E-B9A7-96B5A6F63B67","type":"claude/hook","target":"skill","title":{"ko":"라이브"}},
              {"id":"old","uuid":"e03f48fb-3e00-41d7-b99d-c32854567d67","type":"skill","target":"skill","status":"archived","title":{"en":"Old"}},
              {"id":"page","uuid":"5c502e34-1daf-44d7-b740-1baa03e583e5","type":"doc","title":{"en":"Page"}}]}"#,
        )
        .unwrap();
        app.catalogs.wiki = Ok(Catalog {
            origin: Origin::Builtin,
            catalog_version: parsed.catalog_version,
            entries: parsed.entries,
        });
        to_wiki_list(&mut app);
        assert_eq!(app.catalog.len(), 3);
        let text = screen(&app, 120, 40);
        assert!(
            text.contains("내장 샘플") && text.contains("버전 t1"),
            "{text}"
        );
        for i in [1, 2] {
            app.focus = SK_FIXED_ROWS + i;
            key(&mut app, KeyCode::Enter);
            assert!(app.reference.is_empty(), "entry {i} must not be picked");
        }
        app.focus = SK_FIXED_ROWS;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.reference, "14cea96f-f1e2-461e-b9a7-96b5a6f63b67");
        assert_eq!(app.page_type, "claude/hook");
        assert_eq!(app.skill_name, "live");
        assert_eq!(app.focus, SK_FETCH);
    }

    #[test]
    fn broken_wiki_catalog_is_shown_not_replaced() {
        let p = TempProject::new();
        let mut app = p.app();
        app.catalogs.wiki = Err(crate::catalog::CatalogError {
            origin: Origin::Env(p.0.join("bad.json")),
            message: "unsupported schema_version 2".into(),
        });
        to_wiki_list(&mut app);
        assert!(app.catalog.is_empty());
        let text = screen(&app, 120, 40);
        assert!(text.contains("unsupported schema_version 2"), "{text}");
    }

    /// Serves one canned 200 response and returns the raw request.
    fn serve_once(body: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let mut req = Vec::new();
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });
        (url, handle)
    }

    #[test]
    fn wiki_catalog_import_through_mock_gateway_saves_token() {
        const TOKEN: &str = "tui-token-3b9d";
        let p = TempProject::new();
        let mut app = p.app();
        let body = r##"{"uuid":"14cea96f-f1e2-461e-b9a7-96b5a6f63b67","type":"skill","pages":{
            "en":{"title":"wiki-skill-import","content":"# en\n","contentHash":"sha256:e1"},
            "ko":{"title":"wiki-skill-import","content":"# ko\n","contentHash":"sha256:k1"}}}"##;
        let (url, req) = serve_once(body.into());
        app.gateway_url = url;
        to_wiki_list(&mut app);
        app.focus = SK_TOKEN;
        typed(&mut app, TOKEN);
        assert!(!screen(&app, 120, 40).contains(TOKEN));
        // First built-in entry: wiki-skill-import.
        app.focus = SK_FIXED_ROWS;
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Enter); // fetch
        let req = req.join().unwrap().to_lowercase();
        assert!(
            req.starts_with(
                "get /api/v2/documents/14cea96f-f1e2-461e-b9a7-96b5a6f63b67?type=skill "
            )
        );
        assert!(req.contains(&format!("authorization: bearer {TOKEN}")));
        assert!(app.fetched.is_some(), "{:?}", app.message);
        app.focus = SK_ADD;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.imports.len(), 1);
        app.focus = SK_KIND;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Preview);
        assert!(!screen(&app, 120, 60).contains(TOKEN));
        key(&mut app, KeyCode::Enter); // apply
        assert_eq!(app.step, Step::Done);
        assert_eq!(
            fs::read_to_string(p.0.join(".skills/wiki-skill-import/SKILL_ko.md")).unwrap(),
            "# ko\n"
        );
        let lock = fs::read_to_string(p.0.join("harness.lock.json")).unwrap();
        assert!(lock.contains("sha256:e1") && lock.contains("sha256:k1"));
        let done = screen(&app, 120, 60);
        assert!(done.contains(".harness/local.toml") && !done.contains(TOKEN));
        let stored = crate::local::load_token(&p.0).unwrap().unwrap();
        assert_eq!(stored.expose(), TOKEN);
        for tracked in ["harness.toml", "harness.lock.json", ".gitignore"] {
            let text = fs::read_to_string(p.0.join(tracked)).unwrap();
            assert!(!text.contains(TOKEN), "{tracked}");
        }
        assert!(
            fs::read_to_string(p.0.join(".gitignore"))
                .unwrap()
                .contains("/.harness/")
        );
    }

    #[test]
    fn catalog_view_shows_three_catalogs_and_returns() {
        let p = TempProject::new();
        let mut app = p.app();
        for _ in 0..3 {
            key(&mut app, KeyCode::Enter);
        }
        app.focus = SK_CATALOGS;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.step, Step::Catalog);
        let text = screen(&app, 140, 60);
        for want in [
            "위키 카탈로그",
            "공개 스킬 카탈로그",
            "문서 카탈로그",
            "내장 샘플",
            "버전 2026.11.23",
            "[사용 가능] wiki-skill-import",
            "직접 실행: claude plugin",
            "anthropics-skills-README.md",
            "4 소스·스킬",
        ] {
            assert!(text.contains(want), "{want}\n{text}");
        }
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.step, Step::Skills);
        assert_eq!(app.focus, SK_CATALOGS);
        assert_eq!(files_in(&p.0), 0, "viewing catalogs writes nothing");
    }

    fn to_install(app: &mut App, i: usize) {
        for _ in 0..3 {
            key(app, KeyCode::Enter);
        }
        app.focus = SK_CATALOGS;
        key(app, KeyCode::Enter);
        assert_eq!(app.step, Step::Catalog);
        app.focus = i;
        key(app, KeyCode::Enter);
        assert_eq!(app.step, Step::Install);
    }

    #[test]
    fn install_runs_only_after_confirmation_and_reports_exit_code() {
        let p = TempProject::new();
        let mut app = p.app();
        app.npx = Some(PathBuf::from("/opt/node/bin").join(NPX));
        to_install(&mut app, 0); // built-in `anthropic-pdf`
        assert_eq!(app.take_run(), None, "opening the screen runs nothing");
        let text = screen(&app, 160, 40);
        let cmd = format!(
            "{NPX} skills add anthropics/skills@e4bb5d960d87126ff3b3a8af1a104b7db67560ba -s pdf"
        );
        assert!(text.contains(&cmd), "{text}");
        assert!(text.contains("이름 충돌: 없음"));
        key(&mut app, KeyCode::Enter); // confirm
        let req = app.take_run().expect("queued after confirmation");
        assert_eq!(
            req.args,
            [
                "skills",
                "add",
                "anthropics/skills@e4bb5d960d87126ff3b3a8af1a104b7db67560ba",
                "-s",
                "pdf"
            ]
        );
        assert_eq!(app.take_run(), None, "runs once");
        app.finish_run(Ok(Some(3)));
        assert!(screen(&app, 160, 40).contains("실패 (종료 코드 3)"));
        app.finish_run(Ok(Some(0)));
        assert!(screen(&app, 160, 40).contains("설치 성공"));
        key(&mut app, KeyCode::Esc);
        assert_eq!((app.step, app.focus), (Step::Catalog, 0));
        assert_eq!(files_in(&p.0), 0, "the TUI itself wrote nothing");
    }

    #[test]
    fn install_is_blocked_without_npx_on_conflict_and_for_other_installers() {
        let p = TempProject::new();
        let mut app = p.app();
        app.npx = None;
        to_install(&mut app, 0);
        assert!(screen(&app, 160, 40).contains("사용 불가"));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.take_run(), None);

        let mut app = p.app();
        app.npx = Some(PathBuf::from(NPX));
        app.imports.push(PendingImport {
            skill_id: "PDF".into(),
            source_id: "local".into(),
            reference: "pdf".into(),
            page_type: None,
            accept_partial: false,
            package: Package::default(),
        });
        to_install(&mut app, 0);
        assert!(screen(&app, 160, 40).contains("하네스 스킬 `PDF`"));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.take_run(), None, "name conflict blocks the run");

        // `claude-code-cli` entry: text only.
        let mut app = p.app();
        app.npx = Some(PathBuf::from(NPX));
        to_install(&mut app, 2);
        assert!(screen(&app, 160, 40).contains("claude plugin marketplace add"));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.take_run(), None);
    }

    #[test]
    fn installing_every_skill_warns_that_the_check_is_unavailable() {
        let p = TempProject::new();
        let mut app = p.app();
        app.npx = Some(PathBuf::from(NPX));
        let parsed = crate::catalog::parse::<crate::catalog::SkillsEntry>(
            r#"{"schema_version":1,"kind":"skills","entries":[
              {"id":"all","installer":"skills-cli","source":"owner/repo","title":{"en":"All"}}]}"#,
        )
        .unwrap();
        app.catalogs.skills = Ok(Catalog {
            origin: Origin::Builtin,
            catalog_version: None,
            entries: parsed.entries,
        });
        to_install(&mut app, 0);
        let text = screen(&app, 160, 40);
        assert!(text.contains("이름 충돌 검사를 할 수 없습니다"), "{text}");
        assert!(
            text.contains(&format!("{NPX} skills add owner/repo\n"))
                || text.contains(&format!("{NPX} skills add owner/repo "))
        );
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.take_run().unwrap().args,
            ["skills", "add", "owner/repo"]
        );
    }

    #[test]
    fn initialized_project_is_summary_and_verify_only() {
        let p = TempProject::new();
        let mut app = p.app();
        for _ in 0..5 {
            key(&mut app, KeyCode::Enter);
        }
        assert_eq!(app.step, Step::Done);
        let mut again = p.app();
        let text = screen(&again, 120, 40);
        assert!(text.contains("이미 초기화된 프로젝트"), "{text}");
        assert!(text.contains("변경할 항목이 없습니다"), "{text}");
        key(&mut again, KeyCode::Enter);
        assert!(again.quit, "no re-init path");
        assert_eq!(again.step, Step::Root);
    }

    #[test]
    fn renders_at_small_and_large_sizes() {
        let p = TempProject::new();
        let mut app = p.app();
        for step in 0..4 {
            for (w, h) in [(20, 6), (60, 20), (140, 50)] {
                let text = screen(&app, w, h);
                assert!(!text.is_empty(), "step {step} {w}x{h}");
            }
            key(&mut app, KeyCode::Enter);
        }
        let narrow = screen(&app, 60, 20);
        assert!(narrow.contains("5/6"), "{narrow}");
    }
}
