//! Six-step wizard (Stage 1 spec §7, instruction 0010). Korean UI text.
//!
//! `App` is a plain state machine: `handle_key` changes state and `render`
//! only draws it, so rendering never touches the network or disk. Network
//! access (skill fetch) happens only when the user activates a button.
//! Nothing is written before the explicit apply on the preview step.

use crate::contract::{
    AgentConfig, AgentId, DEFAULT_TOKEN_ENV, HarnessConfig, Placement, SkillConfig, SourceConfig,
    SourceKind,
};
use crate::init::{self, Loaded, PendingImport, Prepared};
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
const SK_TYPE: usize = 2;
const SK_REF: usize = 3;
const SK_FETCH: usize = 4;
const SK_NAME: usize = 5;
const SK_PARTIAL: usize = 6;
const SK_ADD: usize = 7;
const SK_LIST: usize = 8;
const SK_FIXED_ROWS: usize = 9;

pub struct App {
    pub root: ProjectRoot,
    loaded: Result<Loaded, String>,
    pub step: Step,
    focus: usize,
    scroll: u16,
    message: Option<String>,
    pub quit: bool,
    // step 2
    references: String,
    output: String,
    backlog: bool,
    // step 3
    agents: Vec<(AgentId, bool, Placement)>,
    // step 4
    wiki: bool,
    local_dir: String,
    token_env: String,
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
}

impl App {
    pub fn new(root: ProjectRoot) -> Self {
        let loaded = init::load(&root);
        let mut app = Self {
            root,
            loaded,
            step: Step::Root,
            focus: 0,
            scroll: 0,
            message: None,
            quit: false,
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
            Step::Paths => 3,
            Step::Agents => 4,
            Step::Skills => SK_FIXED_ROWS + self.catalog.len(),
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
            Step::Paths => self.focus <= 1,
            Step::Skills => matches!(self.focus, SK_LOCATION | SK_TYPE | SK_REF | SK_NAME),
            _ => false,
        }
    }

    fn input_mut(&mut self) -> Option<&mut String> {
        match (self.step, self.focus) {
            (Step::Paths, 0) => Some(&mut self.references),
            (Step::Paths, 1) => Some(&mut self.output),
            (Step::Skills, SK_LOCATION) if self.wiki => Some(&mut self.token_env),
            (Step::Skills, SK_LOCATION) if self.local_locked.is_none() => Some(&mut self.local_dir),
            (Step::Skills, SK_TYPE) if self.wiki => Some(&mut self.page_type),
            (Step::Skills, SK_REF) => Some(&mut self.reference),
            (Step::Skills, SK_NAME) => Some(&mut self.skill_name),
            _ => None,
        }
    }

    fn toggle(&mut self) {
        match (self.step, self.focus) {
            (Step::Paths, 2) => self.backlog = !self.backlog,
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
                n if n >= SK_FIXED_ROWS => self.pick_catalog(n - SK_FIXED_ROWS),
                _ => self.open_preview(),
            },
            Step::Preview => self.apply(),
            Step::Done => self.quit = true,
        }
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
        let layout = Layout {
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
        WikiGateway::new(self.token_env.trim())
    }

    fn page_type(&self) -> Option<String> {
        let t = self.page_type.trim();
        (self.wiki && !t.is_empty()).then(|| t.to_string())
    }

    fn load_catalog(&mut self) {
        let listed = if self.wiki {
            self.gateway().list()
        } else {
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
                } else if list.iter().any(|e| e.dummy) {
                    format!(
                        "{}개 항목 (더미 목록: 실제 위키 목록 조회는 아직 지원하지 않습니다)",
                        list.len()
                    )
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
            self.reference = e.reference.clone();
            if let Some(t) = &e.page_type {
                self.page_type = t.clone();
            }
            self.skill_name = slugify(&e.title);
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
        self.result = Some(
            init::apply_prepared(&self.root, prepared, &mut loaded.state)
                .map_err(|e| e.to_string()),
        );
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
            let i = STEPS.iter().position(|(s, _)| *s == self.step).unwrap_or(0);
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

    fn step_list(&self) -> Vec<Line<'static>> {
        STEPS
            .iter()
            .enumerate()
            .map(|(i, (s, name))| {
                let text = format!(
                    "{} {} {name}",
                    if *s == self.step { "▸" } else { " " },
                    i + 1
                );
                if *s == self.step {
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
            _ => "↑↓ 이동  Space 선택  Enter 다음  Esc 이전  Ctrl+C 취소".into(),
        }
    }

    fn render_main(&self, f: &mut Frame, area: Rect) {
        let i = STEPS.iter().position(|(s, _)| *s == self.step).unwrap_or(0);
        let mut lines = match self.step {
            Step::Root => self.root_lines(),
            Step::Paths => self.paths_lines(),
            Step::Agents => self.agent_lines(),
            Step::Skills => self.skill_lines(),
            Step::Preview => self.preview_lines(),
            Step::Done => self.result_lines(),
        };
        if let Some(m) = &self.message {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                m.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ));
        }
        let title = format!(" {}. {} ", i + 1, STEPS[i].1);
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
            self.row(2, format!("백로그 agent.db: {}", check(self.backlog))),
            Line::raw(""),
            Line::raw("고정 경로 (1단계에서 변경 불가):"),
            Line::raw(format!("  {}/  진행 중 지시서", Layout::instructions())),
            Line::raw(format!("  {}/  완료 지시서", Layout::completed())),
            Line::raw(format!("  {}  완료 로그", Layout::completed_log())),
            Line::raw(format!("  {}/  스킬 원본", Layout::skills())),
            Line::raw(format!(
                "  {}  백로그 경로 (사용 여부만 선택)",
                Layout::backlog()
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
                "토큰 환경변수 이름: {}",
                input(&self.token_env, f == SK_LOCATION)
            )
        } else if let Some(rel) = &self.local_locked {
            format!("로컬 폴더: {rel}  (고정됨)")
        } else {
            format!("로컬 폴더: {}", input(&self.local_dir, f == SK_LOCATION))
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
            self.row(SK_LIST, "[목록 불러오기]".into()),
        ];
        for (i, e) in self.catalog.iter().enumerate() {
            let dummy = if e.dummy { " (더미)" } else { "" };
            v.push(self.row(
                SK_FIXED_ROWS + i,
                format!("  {}  {}{dummy}", e.title, e.reference),
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
        v.push(Line::raw(""));
        v.push(Line::raw("Git 커밋·푸시는 하지 않았습니다."));
        v
    }
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

    #[test]
    fn wiki_catalog_is_marked_dummy() {
        let p = TempProject::new();
        let mut app = p.app();
        for _ in 0..3 {
            key(&mut app, KeyCode::Enter);
        }
        key(&mut app, KeyCode::Char(' ')); // switch to wiki
        app.focus = SK_LIST;
        key(&mut app, KeyCode::Enter);
        assert!(!app.catalog.is_empty());
        let text = screen(&app, 120, 40);
        assert!(text.contains("(더미)"));
        assert!(text.contains("더미 목록"));
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
