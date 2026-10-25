use crate::{Command, Workspace, navigator::Kind, prompt::Purpose};
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleId {
    Chapters,
    Search,
    Planning,
    Typography,
    Immersion,
    Import,
    Export,
}
impl ModuleId {
    pub(crate) const ALL: [Self; 7] = [
        Self::Chapters,
        Self::Search,
        Self::Planning,
        Self::Typography,
        Self::Immersion,
        Self::Import,
        Self::Export,
    ];
}

pub(crate) struct Contribution {
    pub id: ModuleId,
    pub commands: Vec<Command>,
    pub behavior: Option<ModuleBehavior>,
    pub deactivate: fn(&mut Workspace),
}

#[derive(Clone, Copy)]
pub(crate) struct ModuleBehavior {
    panel: fn(&Workspace, usize) -> ModulePanel,
    focused: fn(&Workspace) -> bool,
    focus: fn(&mut Workspace) -> io::Result<crate::Outcome>,
    select: fn(&mut Workspace, usize),
    event: fn(&mut Workspace, crossterm::event::Event) -> io::Result<Option<crate::Outcome>>,
}

pub(crate) fn contributions() -> Vec<Contribution> {
    let mut chapters = crate::navigator::commands();
    chapters.retain(|c| !is_library_command(&c.id));
    chapters.extend(crate::structure_ui::commands());
    let mut search = crate::search::commands();
    search.extend(crate::book_search::commands());
    vec![
        Contribution {
            id: ModuleId::Export,
            commands: crate::exporting::commands(),
            behavior: None,
            deactivate: Workspace::finish_export_prompt,
        },
        Contribution {
            id: ModuleId::Import,
            commands: crate::import_ui::commands(),
            behavior: None,
            deactivate: Workspace::cancel_import,
        },
        Contribution {
            id: ModuleId::Immersion,
            commands: crate::immersion::commands(),
            deactivate: Workspace::leave_zen,
            behavior: None,
        },
        Contribution {
            id: ModuleId::Typography,
            commands: crate::typography::commands(),
            deactivate: |_| {},
            behavior: None,
        },
        Contribution {
            id: ModuleId::Chapters,
            commands: chapters,
            deactivate: |_| {},
            behavior: Some(ModuleBehavior {
                panel: chapter_panel,
                focused: |w| {
                    w.navigator
                        .as_ref()
                        .is_some_and(|n| matches!(n.kind, Kind::Chapters))
                },
                focus: |w| w.show_navigation(Kind::Chapters),
                select: |w, index| {
                    if let Some(n) = &mut w.navigator {
                        n.selected = index;
                    }
                },
                event: Workspace::navigation_event,
            }),
        },
        Contribution {
            id: ModuleId::Planning,
            commands: crate::planning::commands(),
            deactivate: |w| w.refresh_word_counts(),
            behavior: Some(ModuleBehavior {
                panel: crate::planning::panel,
                focused: |w| {
                    w.navigator
                        .as_ref()
                        .is_some_and(|n| matches!(n.kind, Kind::Planning))
                },
                focus: |w| w.show_navigation(Kind::Planning),
                select: |w, index| {
                    if let Some(n) = &mut w.navigator {
                        n.selected = index;
                    }
                },
                event: Workspace::navigation_event,
            }),
        },
        Contribution {
            id: ModuleId::Search,
            commands: search,
            deactivate: |w| {
                w.close_find();
                w.close_book_search();
            },
            behavior: Some(ModuleBehavior {
                panel: search_panel,
                focused: Workspace::book_search_focused,
                focus: |w| {
                    if w.book_search().is_none() {
                        w.begin_prompt(Purpose::SearchBook)
                    } else {
                        w.toggle_book_focus()
                    }
                },
                select: Workspace::select_book_result,
                event: Workspace::book_search_event,
            }),
        },
    ]
}

pub(crate) fn is_library_command(id: &str) -> bool {
    matches!(
        id,
        "recent-projects" | "open-project" | "new-project" | "forget-project"
    )
}

pub(crate) struct Modules {
    pub config: crate::workspace_layout::LayoutFile,
    pub path: Option<std::path::PathBuf>,
    pub layout: crate::WorkspaceLayout,
    pub popup_origin: Option<crate::search::Origin>,
    epochs: [u64; 7],
    deactivators: std::collections::HashMap<ModuleId, fn(&mut Workspace)>,
    behaviors: std::collections::HashMap<ModuleId, ModuleBehavior>,
}
impl Default for Modules {
    fn default() -> Self {
        Self {
            config: Default::default(),
            path: None,
            layout: Default::default(),
            popup_origin: None,
            epochs: [0; 7],
            deactivators: contributions()
                .into_iter()
                .map(|c| (c.id, c.deactivate))
                .collect(),
            behaviors: contributions()
                .into_iter()
                .filter_map(|c| c.behavior.map(|behavior| (c.id, behavior)))
                .collect(),
        }
    }
}

pub struct ModulePanel {
    pub title: String,
    pub headers: Vec<String>,
    pub entries: Vec<String>,
    pub selected: usize,
    pub first: usize,
}

fn chapter_panel(work: &Workspace, height: usize) -> ModulePanel {
    let default = crate::navigator::Navigator {
        kind: Kind::Chapters,
        query: String::new(),
        selected: 0,
    };
    let nav = work.navigation_for(
        work.navigator
            .as_ref()
            .filter(|n| matches!(n.kind, Kind::Chapters))
            .unwrap_or(&default),
    );
    let capacity = height.saturating_sub(1);
    let first = nav.selected.saturating_sub(capacity.saturating_sub(1));
    ModulePanel {
        title: "章节 · Enter 打开 · Esc 返回".into(),
        headers: vec![if nav.query.is_empty() {
            "卷与章节".into()
        } else {
            format!("筛选  {}", nav.query)
        }],
        entries: nav
            .entries
            .into_iter()
            .skip(first)
            .take(capacity)
            .map(|e| e.label)
            .collect(),
        selected: nav.selected,
        first,
    }
}
fn search_panel(work: &Workspace, height: usize) -> ModulePanel {
    let mut panel = ModulePanel {
        title: "全书搜索 · Enter 跳转 · Esc 关闭".into(),
        headers: vec!["F1 → search-book 开始搜索".into()],
        entries: vec![],
        selected: 0,
        first: 0,
    };
    if let Some(search) = work.book_search() {
        panel.headers = vec![
            format!(
                "查询：{} · 区分大小写 · 代次 {}",
                search.query, search.generation
            ),
            format!(
                "{} · {} 章 / {} 条 · 显示前 {} 条",
                if search.running {
                    "搜索中"
                } else {
                    "搜索完成"
                },
                search.chapters.len(),
                search.total,
                search.entries.len()
            ),
            search.errors.first().map_or_else(String::new, |e| {
                format!("{} 章读取失败：{e}", search.errors.len())
            }),
        ];
        panel.selected = search.selected;
        let capacity = height.saturating_sub(panel.headers.len());
        panel.first = panel.selected.saturating_sub(capacity.saturating_sub(1));
        panel.entries = search
            .entries
            .iter()
            .skip(panel.first)
            .take(capacity)
            .map(|e| {
                let count = search
                    .chapters
                    .iter()
                    .find(|c| c.chapter == e.chapter)
                    .map_or(0, |c| c.count);
                format!(
                    "{}（{} 条）· {} · {} · {}",
                    e.title,
                    count,
                    if e.is_title { "标题" } else { "正文" },
                    e.context,
                    e.revision
                )
            })
            .collect();
    }
    panel
}

impl Modules {
    pub fn enabled(&self, id: ModuleId) -> bool {
        self.config.enabled(id)
    }
    pub fn token(&self, id: ModuleId) -> (ModuleId, u64) {
        (id, self.epochs[id as usize])
    }
    pub fn live(&self, token: (ModuleId, u64)) -> bool {
        self.enabled(token.0) && self.epochs[token.0 as usize] == token.1
    }
}

impl Workspace {
    pub(crate) fn restore_popup_if_closed(&mut self) {
        if self.navigator.is_none()
            && self.history_preview.is_none()
            && self.prompt.is_none()
            && let Some(origin) = self.modules.popup_origin.take()
        {
            origin.restore(self);
        }
    }
    pub fn module_panel(&self, id: ModuleId, height: usize) -> ModulePanel {
        self.modules
            .behaviors
            .get(&id)
            .map(|b| (b.panel)(self, height))
            .unwrap_or_else(|| ModulePanel {
                title: String::new(),
                headers: vec![],
                entries: vec![],
                selected: 0,
                first: 0,
            })
    }
    pub(crate) fn focus_editor(&mut self) {
        if self
            .navigator
            .as_ref()
            .is_some_and(|n| n.panel_module().is_some())
        {
            self.navigator = None;
        }
        self.set_book_focus(false);
    }
    pub(crate) fn module_mouse(&mut self, event: crossterm::event::MouseEvent) -> io::Result<bool> {
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind,
        };
        use ratatui::layout::Position;
        let point = Position::new(event.column, event.row);
        let panel = self
            .modules
            .layout
            .panels
            .iter()
            .rev()
            .find(|p| p.area.contains(point) && self.module_enabled(p.id))
            .cloned();
        let Some(panel) = panel else {
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
                && self.modules.layout.editor.contains(point)
            {
                self.focus_editor();
            }
            return Ok(false);
        };
        if !matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
        ) {
            return Ok(true);
        }
        let behavior = self.modules.behaviors[&panel.id];
        if !(behavior.focused)(self) {
            (behavior.focus)(self)?;
        }
        if self.prompt.is_some() {
            return Ok(true);
        }
        let content_area = crate::workspace_layout::panel_content_rect(panel.area);
        if !content_area.contains(point) {
            return Ok(true);
        }
        let content = self.module_panel(panel.id, content_area.height as usize);
        let row = event.row.saturating_sub(content_area.y) as usize;
        let action = match event.kind {
            MouseEventKind::ScrollUp => Some(KeyCode::Up),
            MouseEventKind::ScrollDown => Some(KeyCode::Down),
            _ if row >= content.headers.len()
                && row - content.headers.len() < content.entries.len() =>
            {
                let index = content.first + row - content.headers.len();
                (behavior.select)(self, index);
                Some(KeyCode::Enter)
            }
            _ => None,
        };
        if let Some(code) = action {
            let event = Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
            (behavior.event)(self, event)?;
        }
        Ok(true)
    }
    pub fn module_focused(&self, id: ModuleId) -> bool {
        self.modules
            .behaviors
            .get(&id)
            .is_some_and(|b| (b.focused)(self))
    }
    pub fn module_enabled(&self, id: ModuleId) -> bool {
        self.modules.enabled(id)
    }
    pub fn set_module_enabled(&mut self, id: ModuleId, enabled: bool) -> io::Result<()> {
        if id == ModuleId::Export && !enabled {
            self.require_export_idle()?;
        }
        if self.import_publishing() {
            return Err(io::Error::other("导入正在写入，请等待完成后更改模块"));
        }
        if self.module_enabled(id) == enabled {
            return Ok(());
        }
        if id == ModuleId::Planning && !enabled {
            self.commit_outline()?;
        }
        let mut candidate = self.modules.config.clone();
        candidate.set_enabled(id, enabled);
        candidate.persist(self.modules.path.as_deref())?;
        self.modules.epochs[id as usize] += 1;
        self.modules.config = candidate;
        self.commands.enable_module(id, enabled);
        if !enabled {
            if self.prompt.as_ref().and_then(|p| p.purpose.module()) == Some(id) {
                self.prompt = None;
            }
            if self.navigator.as_ref().and_then(|n| n.module()) == Some(id) {
                self.navigator = None;
            }
            let deactivate = self.modules.deactivators[&id];
            deactivate(self);
        }
        Ok(())
    }
}

pub(crate) fn commands() -> Vec<Command> {
    use crate::{Outcome, PanelSide};
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    fn change(
        w: &mut Workspace,
        id: ModuleId,
        f: fn(&mut crate::PanelPreferences),
    ) -> io::Result<Outcome> {
        let mut p = w
            .panel_preferences(id)
            .ok_or_else(|| io::Error::other("该模块没有面板"))?;
        f(&mut p);
        w.configure_panel(id, p)?;
        Ok(Outcome::Continue)
    }
    fn toggle(w: &mut Workspace, id: ModuleId) -> io::Result<Outcome> {
        w.set_module_enabled(id, !w.module_enabled(id))?;
        Ok(Outcome::Continue)
    }
    let entries: &[(&str, &str, Run)] = &[
        ("toggle-export-module", "开关手稿导出模块", |w| {
            toggle(w, ModuleId::Export)
        }),
        ("toggle-import-module", "开关手稿导入模块", |w| {
            toggle(w, ModuleId::Import)
        }),
        ("toggle-immersion-module", "开关专注写作模块", |w| {
            toggle(w, ModuleId::Immersion)
        }),
        (
            "toggle-typography-module",
            "开关中文排版模块",
            |w| toggle(w, ModuleId::Typography),
        ),
        (
            "toggle-planning-module",
            "开关大纲与目标模块",
            |w| toggle(w, ModuleId::Planning),
        ),
        ("pin-planning", "固定/收起大纲面板", |w| {
            change(w, ModuleId::Planning, |p| p.pinned = !p.pinned)
        }),
        ("planning-left", "大纲面板放左侧", |w| {
            change(w, ModuleId::Planning, |p| p.side = PanelSide::Left)
        }),
        ("planning-right", "大纲面板放右侧", |w| {
            change(w, ModuleId::Planning, |p| p.side = PanelSide::Right)
        }),
        ("planning-wider", "大纲面板加宽", |w| {
            change(w, ModuleId::Planning, |p| p.width = (p.width + 4).min(60))
        }),
        ("planning-narrower", "大纲面板收窄", |w| {
            change(w, ModuleId::Planning, |p| {
                p.width = p.width.saturating_sub(4).max(18)
            })
        }),
        ("toggle-chapters-module", "开关章节模块", |w| {
            toggle(w, ModuleId::Chapters)
        }),
        ("toggle-search-module", "开关搜索模块", |w| {
            toggle(w, ModuleId::Search)
        }),
        ("pin-chapters", "固定/收起章节面板", |w| {
            change(w, ModuleId::Chapters, |p| p.pinned = !p.pinned)
        }),
        ("pin-search", "固定/收起搜索面板", |w| {
            change(w, ModuleId::Search, |p| p.pinned = !p.pinned)
        }),
        ("chapters-left", "章节面板放左侧", |w| {
            change(w, ModuleId::Chapters, |p| p.side = PanelSide::Left)
        }),
        ("chapters-right", "章节面板放右侧", |w| {
            change(w, ModuleId::Chapters, |p| p.side = PanelSide::Right)
        }),
        ("search-left", "搜索面板放左侧", |w| {
            change(w, ModuleId::Search, |p| p.side = PanelSide::Left)
        }),
        ("search-right", "搜索面板放右侧", |w| {
            change(w, ModuleId::Search, |p| p.side = PanelSide::Right)
        }),
        ("chapters-wider", "章节面板加宽", |w| {
            change(w, ModuleId::Chapters, |p| p.width = (p.width + 4).min(60))
        }),
        ("chapters-narrower", "章节面板收窄", |w| {
            change(w, ModuleId::Chapters, |p| {
                p.width = p.width.saturating_sub(4).max(18)
            })
        }),
        ("search-wider", "搜索面板加宽", |w| {
            change(w, ModuleId::Search, |p| p.width = (p.width + 4).min(60))
        }),
        ("search-narrower", "搜索面板收窄", |w| {
            change(w, ModuleId::Search, |p| {
                p.width = p.width.saturating_sub(4).max(18)
            })
        }),
        ("focus-editor", "返回正文", |w| {
            w.focus_editor();
            Ok(Outcome::Continue)
        }),
    ];
    entries
        .iter()
        .map(|(id, title, run)| Command {
            id: (*id).into(),
            title: (*title).into(),
            run: *run,
        })
        .collect()
}

impl crate::navigator::Navigator {
    /// Panels use the module host; structure/history remain independent popups.
    pub(crate) fn panel_module(&self) -> Option<ModuleId> {
        match self.kind {
            Kind::Planning => Some(ModuleId::Planning),
            Kind::Chapters => Some(ModuleId::Chapters),
            _ => None,
        }
    }
    pub(crate) fn module(&self) -> Option<ModuleId> {
        self.panel_module()
            .or_else(|| matches!(self.kind, Kind::Structure(_)).then_some(ModuleId::Chapters))
    }
}
impl Purpose {
    pub(crate) fn module(&self) -> Option<ModuleId> {
        match self {
            Self::ExportAuthor | Self::ExportDestination(_) => Some(ModuleId::Export),
            Self::ImportSource | Self::ImportEncoding(_) | Self::ImportDestination => {
                Some(ModuleId::Import)
            }
            Self::Format(_) => Some(ModuleId::Typography),
            Self::Outline(_) | Self::ChapterGoal(_) | Self::BookGoal { .. } => {
                Some(ModuleId::Planning)
            }
            Self::Find | Self::SearchBook | Self::Replace { .. } => Some(ModuleId::Search),
            Self::NewVolume
            | Self::NewChapter
            | Self::RenameVolume
            | Self::RenameChapter
            | Self::CopyChapter { .. } => Some(ModuleId::Chapters),
            _ => None,
        }
    }
}
