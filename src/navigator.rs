use crate::line_editor::LineEditor;
use crate::{Command, Outcome, Workspace, prompt::Purpose};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::{
    cell::Cell,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_NAVIGATOR: AtomicU64 = AtomicU64::new(1);
use std::{io, path::PathBuf};

#[derive(Clone)]
pub(crate) enum Kind {
    Conflicts,
    Chapters,
    Planning,
    Recent,
    Structure(crate::structure_ui::Flow),
    History {
        entries: Vec<crate::Snapshot>,
        usage: u64,
        capacity: u64,
    },
}
#[derive(Clone)]
pub(crate) struct Navigator {
    pub(crate) kind: Kind,
    instance: u64,
    revision: u64,
    query: String,
    editor: LineEditor,
    selected: usize,
    // First visible result and last rendered row capacity.
    viewport: Cell<(usize, usize)>,
}
enum NavigationAction {
    None,
    Cancel,
    Activate,
}
impl Navigator {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            instance: NEXT_NAVIGATOR.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            query: String::new(),
            editor: LineEditor::default(),
            selected: 0,
            viewport: Cell::new((0, 1)),
        }
    }
    pub(crate) fn is_history(&self) -> bool {
        matches!(self.kind, Kind::History { .. })
    }
    pub(crate) fn stamp(&self) -> (u64, u64) {
        (self.instance, self.revision)
    }
    pub(crate) fn select(&mut self, index: usize) {
        if self.selected != index {
            self.revision += 1;
        }
        self.selected = index;
    }
    pub(crate) fn filterable(&self) -> bool {
        !matches!(
            self.kind,
            Kind::Conflicts
                | Kind::Structure(
                    crate::structure_ui::Flow::RecycleChapter(_)
                        | crate::structure_ui::Flow::RecycleVolume(_)
                )
        )
    }
    pub(crate) fn query_view(&mut self, width: u16) -> (String, u16) {
        self.editor.view(&self.query, width)
    }
    pub(crate) fn visible_rows(&self, count: usize, capacity: usize) -> Range<usize> {
        let (mut first, _) = self.viewport.get();
        let rows = crate::render::surface::visible_rows(count, self.selected, capacity, &mut first);
        self.viewport.set((first, capacity));
        rows
    }
    fn edit_query(&mut self, event: &Event) {
        if self.filterable() && self.editor.event(&mut self.query, event) {
            self.selected = 0;
            self.revision += 1;
            self.viewport.set((0, self.viewport.get().1));
        }
    }
    fn event(&mut self, event: &Event, count: usize) -> NavigationAction {
        self.select(self.selected.min(count.saturating_sub(1)));
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                return NavigationAction::None;
            }
            let page = self.viewport.get().1.max(1);
            match key.code {
                KeyCode::Esc => return NavigationAction::Cancel,
                KeyCode::Enter => return NavigationAction::Activate,
                KeyCode::Up => self.select(self.selected.saturating_sub(1)),
                KeyCode::Down => {
                    self.select(self.selected.saturating_add(1).min(count.saturating_sub(1)))
                }
                KeyCode::PageUp => self.select(self.selected.saturating_sub(page)),
                KeyCode::PageDown => self.select(
                    self.selected
                        .saturating_add(page)
                        .min(count.saturating_sub(1)),
                ),
                _ => self.edit_query(event),
            }
        } else {
            self.edit_query(event);
        }
        NavigationAction::None
    }
}
pub struct Navigation {
    pub title: String,
    pub query: String,
    pub entries: Vec<NavigationEntry>,
    pub selected: usize,
}
pub struct NavigationEntry {
    pub label: String,
    pub(crate) target: Target,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Conflict(&'static str),
    Planning(&'static str),
    Chapter(String),
    Volume(String),
    Project(PathBuf),
    New,
    Open,
    Structure(String),
    Snapshot(String),
}

impl Workspace {
    pub(crate) fn recent_record_action(&self, event: &Event) -> bool {
        self.navigator
            .as_ref()
            .is_some_and(|n| matches!(n.kind, Kind::Recent))
            && matches!(event, Event::Key(k) if k.kind != KeyEventKind::Release
                && k.code == KeyCode::Char('d') && k.modifiers == KeyModifiers::CONTROL)
    }
    pub fn navigation(&self) -> Option<Navigation> {
        let navigator = self.navigator.as_ref()?;
        Some(self.navigation_for(navigator))
    }
    pub(crate) fn navigation_for(&self, navigator: &Navigator) -> Navigation {
        let mut entries = vec![];
        match &navigator.kind {
            Kind::Planning => {
                entries = [
                    ("编辑本章大纲", "edit-outline"),
                    ("设置本章目标", "chapter-goal"),
                    ("设置全书目标", "book-goal"),
                    ("刷新全书字数", "refresh-word-counts"),
                ]
                .into_iter()
                .map(|(label, id)| NavigationEntry {
                    label: label.into(),
                    target: Target::Planning(id),
                })
                .collect();
            }
            Kind::Conflicts => {
                entries = [
                    ("取消，保留冲突", "cancel"),
                    ("载入磁盘版本（先备份本地）", "load"),
                    ("另存当前本地稿到新文件", "copy"),
                    ("明确覆盖磁盘（先备份双方）", "overwrite"),
                ]
                .into_iter()
                .map(|(label, action)| NavigationEntry {
                    label: label.into(),
                    target: Target::Conflict(action),
                })
                .collect();
            }
            Kind::History {
                entries: snapshots, ..
            } => {
                entries = snapshots
                    .iter()
                    .map(|s| NavigationEntry {
                        label: format!(
                            "{} · {} · {}",
                            if s.marked {
                                "★ 手动保留"
                            } else {
                                "自动"
                            },
                            crate::history_ui::timestamp(s.created),
                            s.id
                        ),
                        target: Target::Snapshot(s.id.clone()),
                    })
                    .collect();
            }
            Kind::Structure(flow) => entries = flow.entries(self),
            Kind::Chapters => {
                if let Some(project) = self.project() {
                    for volume in &project.volumes {
                        entries.push(NavigationEntry {
                            label: format!("▸ {}（{} 章）", volume.title, volume.chapters.len()),
                            target: Target::Volume(volume.id.clone()),
                        });
                        for chapter in &volume.chapters {
                            entries.push(NavigationEntry {
                                label: format!(
                                    "  {}{}",
                                    if self.active_chapter().is_some_and(|c| c.id == chapter.id) {
                                        "• "
                                    } else {
                                        ""
                                    },
                                    chapter.title
                                ),
                                target: Target::Chapter(chapter.id.clone()),
                            });
                        }
                    }
                }
            }
            Kind::Recent => {
                entries.push(NavigationEntry {
                    label: "＋ 新建作品".into(),
                    target: Target::New,
                });
                entries.push(NavigationEntry {
                    label: "打开文件或作品…".into(),
                    target: Target::Open,
                });
                entries.extend(
                    self.recent_projects()
                        .iter()
                        .map(|project| NavigationEntry {
                            label: format!("{} · {}", project.title, project.root.display()),
                            target: Target::Project(project.root.clone()),
                        }),
                );
            }
        }
        let query = navigator.query.to_lowercase();
        entries.retain(|entry| entry.label.to_lowercase().contains(&query));
        let selected = navigator.selected.min(entries.len().saturating_sub(1));
        Navigation {
            title: match &navigator.kind {
                Kind::Planning => "大纲与目标".into(),
                Kind::Conflicts => "外部冲突 · 上下选择 · Enter 执行 · Esc 返回".into(),
                Kind::Chapters => "章节 · 输入标题筛选 · Enter 选择 · Esc 返回 · F1 操作".into(),
                Kind::Recent => "最近文件与作品 · Enter 打开 · Ctrl+D 移除记录 · Esc 返回".into(),
                Kind::Structure(flow) => flow.title().into(),
                Kind::History {
                    usage, capacity, ..
                } => format!("历史 · {usage}/{capacity} 字节 · Enter 预览 · Esc 返回"),
            },
            query: navigator.query.clone(),
            entries,
            selected,
        }
    }
    pub(crate) fn show_navigation(&mut self, kind: Kind) -> io::Result<Outcome> {
        if matches!(kind, Kind::Chapters | Kind::Planning) && self.project.is_none() {
            return Err(io::Error::other("请先打开作品"));
        }
        if !matches!(kind, Kind::Chapters)
            && self.modules.popup_origin.is_none()
            && (self.module_focused(crate::ModuleId::Chapters)
                || self.module_focused(crate::ModuleId::Planning)
                || self.book_search_focused())
        {
            self.modules.popup_origin = Some(crate::search::Origin::take(self));
        }
        self.navigation_ui.clear_hits();
        self.navigator = Some(Navigator::new(kind));
        Ok(Outcome::Continue)
    }
    pub(crate) fn navigation_event(&mut self, event: Event) -> io::Result<Option<Outcome>> {
        let view = self.navigation().unwrap();
        // Keep text deletion separate from the explicit recent-record action.
        if self.recent_record_action(&event) {
            if let Some(NavigationEntry {
                target: Target::Project(root),
                ..
            }) = view.entries.get(view.selected)
            {
                self.forget_project(root)?;
            }
            return Ok(Some(Outcome::Continue));
        }
        let navigator = self.navigator.as_mut().unwrap();
        match navigator.event(&event, view.entries.len()) {
            NavigationAction::Cancel => {
                self.navigator = None;
                self.restore_popup_if_closed();
            }
            NavigationAction::Activate => {
                if let Some(entry) = view.entries.get(view.selected) {
                    if let Target::Structure(value) = &entry.target {
                        let Kind::Structure(flow) = navigator.kind.clone() else {
                            unreachable!()
                        };
                        flow.select(self, value)?;
                        self.restore_popup_if_closed();
                        return Ok(Some(Outcome::Continue));
                    }
                    if let Target::Planning(id) = &entry.target {
                        return self.execute_command(id).map(Some);
                    }
                    match &entry.target {
                        Target::Conflict(action) => match *action {
                            "load" => self.load_external()?,
                            "copy" => {
                                self.begin_prompt(Purpose::ConflictCopy)?;
                            }
                            "overwrite" => {
                                self.begin_prompt(Purpose::ConflictOverwrite)?;
                            }
                            _ => {}
                        },
                        Target::Snapshot(id) => self.open_history_preview(id)?,
                        Target::Structure(_) | Target::Planning(_) => unreachable!(),
                        Target::Chapter(id) => self.switch_chapter(id)?,
                        Target::Volume(id) => self.selected_volume = Some(id.clone()),
                        Target::Project(root) => self.switch_recent(root)?,
                        Target::New => {
                            self.begin_prompt(Purpose::NewProject)?;
                        }
                        Target::Open => {
                            self.begin_prompt(Purpose::OpenProject)?;
                        }
                    }
                    self.navigator = None;
                    self.restore_popup_if_closed();
                }
            }
            NavigationAction::None => {}
        }
        Ok(Some(Outcome::Continue))
    }
    pub(crate) fn selected_volume_id(&self) -> io::Result<String> {
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        if let Some(id) = &self.selected_volume
            && store.manifest.volumes.iter().any(|volume| volume.id == *id)
        {
            return Ok(id.clone());
        }
        store
            .manifest
            .volumes
            .iter()
            .find(|volume| volume.chapters.iter().any(|c| c.id == store.active))
            .map(|v| v.id.clone())
            .ok_or_else(|| io::Error::other("当前章节没有所属卷"))
    }
}
pub(crate) fn commands() -> Vec<Command> {
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    let entries: &[(&str, &str, Run)] = &[
        ("chapters", "章节目录 / 按标题跳章", |w| {
            w.show_navigation(Kind::Chapters)
        }),
        ("recent-projects", "最近文件与作品", |w| {
            w.show_navigation(Kind::Recent)
        }),
        ("open-project", "打开文件或作品", |w| {
            w.begin_prompt(Purpose::OpenProject)
        }),
        ("new-project", "新建作品", |w| {
            w.begin_prompt(Purpose::NewProject)
        }),
        ("new-volume", "新建卷", |w| {
            w.begin_prompt(Purpose::NewVolume)
        }),
        ("new-chapter", "在选中卷新建章节", |w| {
            w.begin_prompt(Purpose::NewChapter)
        }),
        ("rename-volume", "修改选中卷标题", |w| {
            w.begin_prompt(Purpose::RenameVolume)
        }),
        ("rename-chapter", "修改当前章节标题", |w| {
            w.begin_prompt(Purpose::RenameChapter)
        }),
        ("next-chapter", "下一章", |w| {
            w.next_chapter()?;
            Ok(Outcome::Continue)
        }),
        ("previous-chapter", "上一章", |w| {
            w.previous_chapter()?;
            Ok(Outcome::Continue)
        }),
        (
            "forget-project",
            "移除当前文件或作品的最近记录（保留手稿）",
            |w| {
                let root = w
                    .project
                    .as_ref()
                    .map_or_else(|| w.path.clone(), |store| store.root.clone());
                w.forget_project(root)?;
                Ok(Outcome::Continue)
            },
        ),
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
