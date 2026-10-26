//! Navigation tools share one drawer and keep only geometry from the current frame.
use crate::{
    Workspace,
    navigator::{Kind, Target},
    render::{chrome_line, fit_label, safe_label, surface::Drawer},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Block, Paragraph},
};
use std::io;

#[derive(Default)]
pub(crate) struct NavigationUi {
    stamp: Option<(u64, u64)>,
    area: Option<Rect>,
    close: Option<Rect>,
    hits: Vec<(Rect, usize, Target, String)>,
    preview: Option<PreviewHits>,
}
struct PreviewHits {
    id: String,
    content: Rect,
    close: Rect,
    restore: Rect,
}
impl NavigationUi {
    pub(crate) fn clear_hits(&mut self) {
        self.stamp = None;
        self.area = None;
        self.close = None;
        self.hits.clear();
        self.preview = None;
    }
}
impl Workspace {
    fn navigation_tool_open(&self) -> bool {
        self.navigator
            .as_ref()
            .is_some_and(|n| n.panel_module().is_none())
            && self.history_preview.is_none()
    }
    pub(crate) fn bottom_tool_open(&self) -> bool {
        self.design_ui.open
            || self.commands.palette_selection().is_some()
            || self.navigation_tool_open()
    }
    fn navigation_topmost(&self) -> bool {
        self.palette().is_none()
            && !self.design_ui.open
            && self.prompt.is_none()
            && !self.import_active()
    }
    pub(crate) fn render_navigation_tool(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if !self.navigation_tool_open() || !self.navigation_topmost() {
            return;
        }
        let navigation = self.navigation().unwrap();
        let kind = &self.navigator.as_ref().unwrap().kind;
        let recent = matches!(kind, Kind::Recent);
        let theme = self.theme();
        self.design_effect_regions.push(area);
        let title = match kind {
            Kind::Recent => "最近文件与作品",
            Kind::History { .. } => "历史快照",
            Kind::Conflicts => "外部冲突",
            Kind::Structure(crate::structure_ui::Flow::RecycleChapter(_)) => "确认回收章节",
            Kind::Structure(crate::structure_ui::Flow::RecycleVolume(_)) => "确认回收卷",
            _ => navigation
                .title
                .split('·')
                .next()
                .unwrap_or(&navigation.title)
                .trim(),
        };
        let Some(drawer) = Drawer::paint(f, &theme, title, area) else {
            return;
        };
        self.navigation_ui.area = Some(area);
        self.navigation_ui.close = Some(drawer.close);
        let rows = self
            .navigator
            .as_ref()
            .unwrap()
            .visible_rows(navigation.entries.len(), drawer.content.height as usize);
        for (i, entry) in navigation
            .entries
            .iter()
            .enumerate()
            .skip(rows.start)
            .take(rows.len())
        {
            let row = Rect::new(
                drawer.content.x,
                drawer.content.y + (i - rows.start) as u16,
                drawer.content.width,
                1,
            );
            let selected = i == navigation.selected;
            let (title, detail) = self.navigation_row(kind, entry);
            chrome_line(
                f,
                row,
                &format!("  {title}"),
                if row.width >= 70 { &detail } else { "" },
                if selected {
                    theme.surface.add_modifier(Modifier::BOLD)
                } else {
                    theme.surface
                },
                theme.muted,
            );
            if selected {
                f.render_widget(
                    Paragraph::new("▎").style(theme.accent),
                    Rect::new(row.x, row.y, 1, 1),
                );
            }
            self.navigation_ui
                .hits
                .push((row, i, entry.target.clone(), entry.label.clone()));
        }
        if navigation.entries.is_empty() {
            let empty = if matches!(kind, Kind::Recent) {
                "没有匹配的文件或作品"
            } else {
                "没有匹配项"
            };
            f.render_widget(Paragraph::new(empty).style(theme.muted), drawer.content);
        }
        let entry = navigation.entries.get(navigation.selected);
        let removable = matches!(kind, Kind::Recent)
            && entry.is_some_and(|e| matches!(e.target, Target::Project(_)));
        let detail = match (kind, entry.map(|e| &e.target)) {
            (Kind::Recent, Some(Target::Project(root))) => safe_label(&root.to_string_lossy()),
            (
                Kind::History {
                    usage, capacity, ..
                },
                _,
            ) => format!("{usage}/{capacity} 字节"),
            _ => format!("{} 项", navigation.entries.len()),
        };
        let filterable = self.navigator.as_ref().unwrap().filterable();
        let hint = if removable {
            if drawer.hint.width >= 40 {
                "Enter 打开 · Ctrl+D 移除记录"
            } else {
                "Ctrl+D 移除记录"
            }
        } else {
            match kind {
                Kind::Recent => "Enter 打开",
                Kind::History { .. } => "Enter 预览",
                Kind::Conflicts => "默认取消 · 覆盖仍需输入确认",
                _ if !filterable => "默认取消 · 内容保留在回收区",
                _ => "Enter 选择",
            }
        };
        if (removable && drawer.hint.width < 80) || !filterable {
            f.render_widget(
                Paragraph::new(fit_label(hint, drawer.hint.width)).style(theme.muted),
                drawer.hint,
            );
        } else {
            chrome_line(f, drawer.hint, &detail, hint, theme.muted, theme.muted);
        }
        let n = self.navigator.as_mut().unwrap();
        if filterable {
            let (query, cursor) = n.query_view(drawer.entry.width.saturating_sub(2));
            let label = if navigation.query.is_empty() {
                fit_label(
                    if recent {
                        "筛选文件、作品或路径…"
                    } else {
                        "输入文字筛选…"
                    },
                    drawer.entry.width.saturating_sub(2),
                )
                .into_owned()
            } else {
                query
            };
            f.render_widget(
                Paragraph::new(format!("› {label}")).style(theme.surface),
                drawer.entry,
            );
            f.set_cursor_position((drawer.entry.x + 2 + cursor, drawer.entry.y));
        } else {
            f.render_widget(
                Paragraph::new(fit_label("上下选择 · Enter 确认", drawer.entry.width))
                    .style(theme.muted),
                drawer.entry,
            );
        }
        self.navigation_ui.stamp = Some(n.stamp());
    }
    fn navigation_row(&self, kind: &Kind, entry: &crate::NavigationEntry) -> (String, String) {
        match (&entry.target, kind) {
            (Target::Project(root), Kind::Recent) => {
                let title = self
                    .recent_projects()
                    .iter()
                    .find(|p| p.root == *root)
                    .map_or(entry.label.as_str(), |p| p.title.as_str());
                (safe_label(title), safe_label(&root.to_string_lossy()))
            }
            (Target::Snapshot(id), Kind::History { entries, .. }) => {
                let snapshot = entries.iter().find(|s| s.id == *id).unwrap();
                (
                    format!(
                        "{} · {}",
                        crate::history_ui::timestamp(snapshot.created),
                        if snapshot.marked {
                            "手动保留"
                        } else {
                            "自动恢复点"
                        }
                    ),
                    String::new(),
                )
            }
            (Target::Structure(value), Kind::Structure(flow)) if !value.is_empty() => {
                use crate::structure_ui::Flow;
                let label = match flow {
                    Flow::RecycleChapter(id) => self
                        .project()
                        .and_then(|p| {
                            p.volumes
                                .iter()
                                .flat_map(|v| &v.chapters)
                                .find(|c| c.id == *id)
                        })
                        .map(|c| format!("回收 1 章 ·「{}」", safe_label(&c.title))),
                    Flow::RecycleVolume(id) => self
                        .project()
                        .and_then(|p| p.volumes.iter().find(|v| v.id == *id))
                        .map(|v| {
                            format!(
                                "回收全部 {} 章 ·「{}」",
                                v.chapters.len(),
                                safe_label(&v.title)
                            )
                        }),
                    _ => None,
                };
                (
                    label.unwrap_or_else(|| safe_label(&entry.label)),
                    String::new(),
                )
            }
            (Target::Conflict(action), _) => (
                (match *action {
                    "load" => "载入磁盘 · 先备份本地",
                    "copy" => "另存当前本地稿",
                    "overwrite" => "覆盖磁盘 · 先备份双方",
                    _ => "取消，保留冲突",
                })
                .into(),
                String::new(),
            ),
            _ => (safe_label(&entry.label), String::new()),
        }
    }
    pub(crate) fn render_navigation_preview(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let Some(id) = self.history_preview_id().map(str::to_owned) else {
            return;
        };
        if area.is_empty() {
            return;
        }
        let theme = self.theme();
        f.render_widget(Block::default().style(theme.surface), area);
        let content = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(2),
        );
        let title = Rect::new(area.x, area.y, area.width, 1);
        f.render_widget(
            Paragraph::new("─".repeat(area.width as usize)).style(theme.border),
            title,
        );
        if area.width >= 24 {
            let close = Rect::new(area.right() - 8, area.y, 8, 1);
            let restore = Rect::new(area.right() - 15, area.y, 4, 1);
            f.render_widget(
                Paragraph::new(fit_label(" 历史预览 ", area.width - 16))
                    .style(theme.surface.patch(theme.muted)),
                Rect::new(area.x, area.y, area.width - 16, 1),
            );
            f.render_widget(
                Paragraph::new("恢复").style(theme.surface.patch(theme.accent)),
                restore,
            );
            f.render_widget(
                Paragraph::new("关闭 Esc").style(theme.surface.patch(theme.muted)),
                close,
            );
            if self.navigation_topmost() {
                self.navigation_ui.preview = Some(PreviewHits {
                    id,
                    content,
                    close,
                    restore,
                });
            }
        } else {
            f.render_widget(
                Paragraph::new("历史预览 · Esc 关闭").style(theme.muted),
                title,
            );
        }
        if let Some(view) = self.history_preview(content.width, content.height) {
            f.render_widget(
                Paragraph::new(
                    view.lines
                        .into_iter()
                        .map(ratatui::text::Line::from)
                        .collect::<Vec<_>>(),
                )
                .style(theme.surface),
                content,
            );
            if let Some((x, y)) = view.cursor {
                f.set_cursor_position((content.x + x, content.y + y));
            }
        }
        if area.height >= 2 {
            f.render_widget(
                Paragraph::new(fit_label("Enter 恢复 · 先备份当前稿", area.width))
                    .style(theme.muted),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        }
    }
    pub(crate) fn navigation_tool_mouse(&mut self, event: MouseEvent) -> io::Result<bool> {
        if !self.navigation_topmost() {
            return Ok(false);
        }
        let pos = Position::new(event.column, event.row);
        if self.history_preview.is_some() {
            let Some(hit) = self
                .navigation_ui
                .preview
                .as_ref()
                .filter(|h| self.history_preview_id() == Some(h.id.as_str()))
            else {
                return Ok(true);
            };
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) if hit.close.contains(pos) => {
                    self.history_preview_event(Event::Key(KeyEvent::new(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                    )))?;
                }
                MouseEventKind::Down(MouseButton::Left) if hit.restore.contains(pos) => {
                    self.history_preview_event(Event::Key(KeyEvent::new(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                    )))?;
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    if hit.content.contains(pos) =>
                {
                    let height = hit.content.height;
                    self.scroll_history_preview(
                        if event.kind == MouseEventKind::ScrollUp {
                            -3
                        } else {
                            3
                        },
                        height,
                    );
                }
                _ => {}
            }
            return Ok(true);
        }
        if !self.navigation_tool_open() {
            return Ok(false);
        }
        if self.navigator.as_ref().map(|n| n.stamp()) != self.navigation_ui.stamp {
            return Ok(true);
        }
        let code = match event.kind {
            MouseEventKind::Down(MouseButton::Left)
                if self.navigation_ui.close.is_some_and(|r| r.contains(pos)) =>
            {
                Some(KeyCode::Esc)
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self
                    .navigation_ui
                    .hits
                    .iter()
                    .find(|(r, _, _, _)| r.contains(pos));
                if let Some((_, index, target, label)) = hit {
                    let index = *index;
                    if self.navigation().is_some_and(|v| {
                        v.entries
                            .get(index)
                            .is_some_and(|e| &e.target == target && &e.label == label)
                    }) {
                        self.navigator.as_mut().unwrap().select(index);
                        Some(KeyCode::Enter)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            MouseEventKind::ScrollUp
                if self.navigation_ui.area.is_some_and(|r| r.contains(pos)) =>
            {
                Some(KeyCode::Up)
            }
            MouseEventKind::ScrollDown
                if self.navigation_ui.area.is_some_and(|r| r.contains(pos)) =>
            {
                Some(KeyCode::Down)
            }
            _ => None,
        };
        if let Some(code) = code {
            self.navigation_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))?;
        }
        Ok(true)
    }
}
