use crate::{
    Command, NavigationEntry, Outcome, Workspace,
    navigator::{Kind, Target},
    prompt::Purpose,
};
use std::io;

#[derive(Clone)]
pub(crate) enum Flow {
    Move(String),
    Copy(String),
    Restore(String),
    RecycleBin,
    RecycleChapter(String),
    RecycleVolume(String),
}
fn entry(label: String, value: String) -> NavigationEntry {
    NavigationEntry {
        label,
        target: Target::Structure(value),
    }
}
impl Flow {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Move(_) => "移动章节 · 选择目标卷 · 放在卷末",
            Self::Copy(_) => "复制章节 · 选择目标卷 · 放在卷末",
            Self::Restore(_) => "恢复 · 选择目标卷 · 放在卷末 · 同名并存",
            Self::RecycleBin => "回收区 · Enter 选择恢复目标卷 · Esc 返回",
            Self::RecycleChapter(_) | Self::RecycleVolume(_) => {
                "确认删除范围 · 内容保留在回收区 · Esc 取消"
            }
        }
    }
    pub fn entries(&self, work: &Workspace) -> Vec<NavigationEntry> {
        let Some(project) = work.project() else {
            return vec![];
        };
        match self {
            Self::Move(_) | Self::Copy(_) | Self::Restore(_) => project
                .volumes
                .iter()
                .map(|v| {
                    entry(
                        format!("{}（{} 章）", v.title, v.chapters.len()),
                        v.id.clone(),
                    )
                })
                .collect(),
            Self::RecycleBin => project
                .recycle_bin
                .iter()
                .map(|r| {
                    entry(
                        format!("{} · {} 章", r.title, r.chapters.len()),
                        r.id.clone(),
                    )
                })
                .collect(),
            Self::RecycleChapter(id) => {
                let title = project
                    .volumes
                    .iter()
                    .flat_map(|v| &v.chapters)
                    .find(|c| c.id == *id)
                    .map(|c| &c.title);
                title.map_or_else(Vec::new, |title| {
                    vec![
                        entry("取消".into(), String::new()),
                        entry(format!("移入回收区：章节「{title}」· 1 章"), id.clone()),
                    ]
                })
            }
            Self::RecycleVolume(id) => {
                project
                    .volumes
                    .iter()
                    .find(|v| v.id == *id)
                    .map_or_else(Vec::new, |v| {
                        vec![
                            entry("取消".into(), String::new()),
                            entry(
                                format!(
                                    "移入回收区：卷「{}」· 全部 {} 章",
                                    v.title,
                                    v.chapters.len()
                                ),
                                id.clone(),
                            ),
                        ]
                    })
            }
        }
    }
    pub fn select(&self, work: &mut Workspace, value: &str) -> io::Result<()> {
        if value.is_empty() {
            work.navigator = None;
            return Ok(());
        }
        match self {
            Self::Move(id) => work.move_chapter(id, value, usize::MAX)?,
            Self::Copy(id) => {
                work.begin_prompt(Purpose::CopyChapter {
                    chapter: id.clone(),
                    volume: value.into(),
                })?;
            }
            Self::Restore(id) => work.restore_recycled(id, value, usize::MAX)?,
            Self::RecycleBin => {
                work.show_navigation(Kind::Structure(Self::Restore(value.into())))?;
                return Ok(());
            }
            Self::RecycleChapter(id) => {
                work.recycle_chapter(id)?;
            }
            Self::RecycleVolume(id) => {
                work.recycle_volume(id)?;
            }
        }
        work.navigator = None;
        Ok(())
    }
}
fn current(work: &Workspace) -> io::Result<String> {
    work.active_chapter()
        .map(|c| c.id.clone())
        .ok_or_else(|| io::Error::other("请先打开作品"))
}
fn reorder_chapter(work: &mut Workspace, down: bool) -> io::Result<Outcome> {
    let id = current(work)?;
    let volume = work
        .project()
        .unwrap()
        .volumes
        .iter()
        .find(|v| v.chapters.iter().any(|c| c.id == id))
        .unwrap();
    let at = volume.chapters.iter().position(|c| c.id == id).unwrap();
    let volume = volume.id.clone();
    work.move_chapter(
        &id,
        &volume,
        if down { at + 1 } else { at.saturating_sub(1) },
    )?;
    Ok(Outcome::Continue)
}
fn reorder_volume(work: &mut Workspace, down: bool) -> io::Result<Outcome> {
    let id = work.selected_volume_id()?;
    let at = work
        .project()
        .unwrap()
        .volumes
        .iter()
        .position(|v| v.id == id)
        .unwrap();
    work.move_volume(&id, if down { at + 1 } else { at.saturating_sub(1) })?;
    Ok(Outcome::Continue)
}
pub(crate) fn commands() -> Vec<Command> {
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    let entries: &[(&str, &str, Run)] = &[
        ("move-chapter", "移动当前章节到其他卷", |w| {
            w.show_navigation(Kind::Structure(Flow::Move(current(w)?)))
        }),
        ("copy-chapter", "复制当前章节", |w| {
            w.show_navigation(Kind::Structure(Flow::Copy(current(w)?)))
        }),
        ("chapter-up", "当前章节上移", |w| {
            reorder_chapter(w, false)
        }),
        ("chapter-down", "当前章节下移", |w| {
            reorder_chapter(w, true)
        }),
        ("volume-up", "选中卷上移", |w| reorder_volume(w, false)),
        ("volume-down", "选中卷下移", |w| {
            reorder_volume(w, true)
        }),
        (
            "number-chapters",
            "按目录顺序重新编号所有章节",
            |w| {
                w.number_chapters()?;
                Ok(Outcome::Continue)
            },
        ),
        (
            "recycle-chapter",
            "删除当前章节到回收区（确认范围）",
            |w| w.show_navigation(Kind::Structure(Flow::RecycleChapter(current(w)?))),
        ),
        (
            "recycle-volume",
            "删除选中卷到回收区（确认范围）",
            |w| {
                w.show_navigation(Kind::Structure(Flow::RecycleVolume(
                    w.selected_volume_id()?,
                )))
            },
        ),
        ("recycle-bin", "回收区 / 恢复章节", |w| {
            w.show_navigation(Kind::Structure(Flow::RecycleBin))
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
