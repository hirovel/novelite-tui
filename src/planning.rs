use crate::Workspace;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterPlan {
    pub outline: String,
    pub target_words: Option<u64>,
}
impl ChapterPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.outline.is_empty() && self.target_words.is_none()
    }
}
impl Workspace {
    pub fn chapter_plan(&self) -> Option<&ChapterPlan> {
        self.active_chapter().map(|c| &c.plan)
    }
    pub fn update_chapter_plan(&mut self, plan: ChapterPlan) -> io::Result<()> {
        self.require_writable()?;
        let store = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        let chapter = store
            .manifest
            .volumes
            .iter_mut()
            .flat_map(|v| &mut v.chapters)
            .find(|c| c.id == store.active)
            .ok_or_else(|| io::Error::other("当前章节不存在"))?;
        if chapter.plan != plan {
            chapter.plan = plan;
            store.changed();
            self.saving.edited();
        }
        Ok(())
    }
    pub fn set_book_goal(&mut self, target: Option<u64>) -> io::Result<()> {
        self.require_writable()?;
        let store = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        if store.manifest.target_words != target {
            store.manifest.target_words = target;
            store.changed();
            self.saving.edited();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordProgress {
    pub chapter_words: u64,
    pub chapter_goal: Option<u64>,
    pub book_words: Option<u64>,
    pub book_goal: Option<u64>,
    pub running: bool,
    pub errors: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct PlanEdit {
    root: std::path::PathBuf,
    chapter: String,
    original: ChapterPlan,
}
impl Workspace {
    pub(crate) fn begin_plan_edit(&mut self, outline: bool) -> io::Result<crate::Outcome> {
        self.require_writable()?;
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        let original = self.chapter_plan().unwrap().clone();
        let edit = PlanEdit {
            root: store.root.clone(),
            chapter: store.active.clone(),
            original,
        };
        let purpose = if outline {
            crate::prompt::Purpose::Outline(edit.clone())
        } else {
            crate::prompt::Purpose::ChapterGoal(edit.clone())
        };
        let mut prompt = crate::Prompt::new(purpose);
        if outline {
            prompt.editor = Some(crate::document::Document::new(
                edit.original.outline.clone(),
                false,
            ));
        }
        self.prompt = Some(prompt);
        Ok(crate::Outcome::Continue)
    }
    pub(crate) fn apply_plan_edit(
        &mut self,
        edit: PlanEdit,
        text: &str,
        outline: bool,
    ) -> io::Result<()> {
        if self
            .project
            .as_ref()
            .is_none_or(|s| s.root != edit.root || s.active != edit.chapter)
            || self.chapter_plan() != Some(&edit.original)
        {
            return Err(io::Error::other("章节或大纲已变化；取消后重新编辑"));
        }
        let mut plan = edit.original;
        if outline {
            plan.outline = text.into();
        } else {
            plan.target_words = parse_goal(text)?;
        }
        self.update_chapter_plan(plan)
    }
    pub(crate) fn commit_outline(&mut self) -> io::Result<()> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };
        let crate::prompt::Purpose::Outline(edit) = &prompt.purpose else {
            return Ok(());
        };
        let edit = edit.clone();
        let text = prompt.editor.as_ref().unwrap().text.clone();
        self.apply_plan_edit(edit, &text, true)?;
        self.prompt = None;
        self.restore_popup_if_closed();
        Ok(())
    }
    pub(crate) fn apply_book_goal(
        &mut self,
        root: std::path::PathBuf,
        original: Option<u64>,
        text: &str,
    ) -> io::Result<()> {
        if self
            .project
            .as_ref()
            .is_none_or(|s| s.root != root || s.manifest.target_words != original)
        {
            return Err(io::Error::other("作品或目标已变化；取消后重新编辑"));
        }
        self.set_book_goal(parse_goal(text)?)
    }
}
fn parse_goal(text: &str) -> io::Result<Option<u64>> {
    let goal = text
        .trim()
        .parse::<u64>()
        .map_err(|_| io::Error::other("目标应为非负整数，0 表示清除"))?;
    Ok((goal != 0).then_some(goal))
}
pub(crate) fn commands() -> Vec<crate::Command> {
    use crate::{Outcome, navigator::Kind};
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    let entries: &[(&str, &str, Run)] = &[
        ("planning", "大纲与写作目标", |w| {
            w.show_navigation(Kind::Planning)
        }),
        ("edit-outline", "编辑本章大纲", |w| {
            w.begin_plan_edit(true)
        }),
        (
            "chapter-goal",
            "设置本章字数目标（0 清除）",
            |w| w.begin_plan_edit(false),
        ),
        ("book-goal", "设置全书字数目标（0 清除）", |w| {
            w.require_writable()?;
            let store = w
                .project
                .as_ref()
                .ok_or_else(|| io::Error::other("请先打开作品"))?;
            w.begin_prompt(crate::prompt::Purpose::BookGoal {
                root: store.root.clone(),
                original: store.manifest.target_words,
            })
        }),
        ("refresh-word-counts", "刷新全书字数", |w| {
            w.refresh_word_counts();
            Ok(Outcome::Continue)
        }),
    ];
    entries
        .iter()
        .map(|(id, title, run)| crate::Command {
            id: (*id).into(),
            title: (*title).into(),
            run: *run,
        })
        .collect()
}
pub(crate) fn panel(work: &Workspace, height: usize) -> crate::ModulePanel {
    use crate::navigator::{Kind, Navigator};
    let default = Navigator::new(Kind::Planning);
    let navigator = work
        .navigator
        .as_ref()
        .filter(|n| matches!(n.kind, Kind::Planning))
        .unwrap_or(&default);
    let nav = work.navigation_for(navigator);
    let p = work.word_progress();
    let mut headers = vec![
        "本章目标".into(),
        format!(
            "{} / {} 字",
            p.chapter_words,
            p.chapter_goal.map_or("未设定".into(), |n| n.to_string())
        ),
    ];
    if let Some(goal) = p.chapter_goal.filter(|n| *n > 0) {
        let ratio = ((p.chapter_words as f64 / goal as f64) * 100.0).min(100.0) as usize;
        headers.push(format!(
            "{}{}  {}%",
            "━".repeat(ratio * 12 / 100),
            "─".repeat(12 - ratio * 12 / 100),
            ratio
        ));
    }
    // Reuse the existing separator row so focusing the query cannot move actions.
    let query_row = work
        .navigator
        .as_ref()
        .filter(|n| matches!(n.kind, Kind::Planning))
        .map(|_| headers.len());
    headers.extend([
        if query_row.is_some() {
            format!("筛选  {}", nav.query)
        } else {
            String::new()
        },
        "章节大纲".into(),
    ]);
    let width = work
        .modules
        .layout
        .panels
        .iter()
        .find(|panel| panel.id == crate::ModuleId::Planning)
        .map(|panel| crate::workspace_layout::panel_content_rect(panel.area).width as usize)
        .unwrap_or(28);
    let remaining = height.saturating_sub(headers.len() + 7);
    if remaining > 0
        && let Some(plan) = work.chapter_plan()
    {
        let lines = wrap_outline(&plan.outline, width, remaining + 1);
        if lines.len() > remaining && remaining > 0 {
            headers.extend(lines.into_iter().take(remaining - 1));
            headers.push("… 编辑大纲查看全文".into());
        } else {
            headers.extend(lines.into_iter().take(remaining));
        }
    }
    headers.extend([
        String::new(),
        format!(
            "全书 {}{} 字",
            p.book_words.map_or("待统计".into(), |n| n.to_string()),
            p.book_goal.map_or(String::new(), |n| format!(" / {n}"))
        ),
        if !p.errors.is_empty() {
            "统计失败 · 请刷新字数".into()
        } else if p.running {
            "统计中".into()
        } else {
            String::new()
        },
    ]);
    let capacity = height.saturating_sub(headers.len());
    let first = navigator.visible_rows(nav.entries.len(), capacity).start;
    crate::ModulePanel {
        title: "大纲与目标 · Enter 编辑 · Esc 返回".into(),
        query_row,
        headers,
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

fn wrap_outline(text: &str, width: usize, limit: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let mut out = Vec::new();
    if limit == 0 || width == 0 {
        return out;
    }
    // Bound invisible/combining runs too: terminal columns alone cannot bound work.
    let budget = width.saturating_mul(limit).saturating_mul(16);
    let mut end = text.len().min(budget);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = end < text.len();
    let text = &text[..end];
    let mut part = String::new();
    let mut columns = 0;
    // Read only the visible rows plus one overflow row, even for a long single line.
    for g in text.graphemes(true) {
        if g == "\n" || g == "\r\n" {
            out.push(std::mem::take(&mut part));
            if out.len() == limit {
                return out;
            }
            columns = 0;
            continue;
        }
        let w = g.width();
        if columns + w > width && !part.is_empty() {
            out.push(std::mem::take(&mut part));
            if out.len() == limit {
                return out;
            }
            columns = 0;
        }
        part.push_str(g);
        columns += w;
    }
    if !part.is_empty() {
        out.push(part);
    }
    if truncated && out.len() < limit {
        out.push("… 编辑大纲查看全文".into());
    }
    out
}
