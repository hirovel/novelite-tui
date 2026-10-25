//! Recent-project drawer. Existing navigation owns targets and execution; this
//! Module keeps viewport/input state and hit rectangles local to its rendering.
use crate::{
    Workspace,
    line_editor::LineEditor,
    navigator::{Kind, Target},
    render::{
        chrome_line, fit_label, safe_label,
        surface::{Drawer, visible_rows},
    },
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    widgets::Paragraph,
};
use std::io;
#[derive(Default)]
pub(crate) struct RecentUi {
    pub query: LineEditor,
    scroll: usize,
    area: Option<Rect>,
    close: Option<Rect>,
    hits: Vec<(Rect, usize)>,
}
impl RecentUi {
    pub fn clear_hits(&mut self) {
        self.area = None;
        self.close = None;
        self.hits.clear();
    }
}
impl Workspace {
    pub(crate) fn recent_open(&self) -> bool {
        self.navigator
            .as_ref()
            .is_some_and(|n| matches!(n.kind, Kind::Recent))
    }
    pub(crate) fn recent_record_action(&self, event: &Event) -> bool {
        self.recent_open()
            && matches!(event, Event::Key(k) if k.kind != KeyEventKind::Release
                && k.code == KeyCode::Char('d') && k.modifiers == KeyModifiers::CONTROL)
    }
    pub(crate) fn bottom_tool_open(&self) -> bool {
        self.design_ui.open || self.commands.palette_selection().is_some() || self.recent_open()
    }
    fn recent_visible(&self) -> bool {
        self.recent_open()
            && self.palette().is_none()
            && !self.design_ui.open
            && self.prompt.is_none()
            && self.history_preview.is_none()
            && !self.import_active()
    }
    pub(crate) fn render_recent(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if !self.recent_visible() {
            return;
        }
        let navigation = self.navigation().unwrap();
        let theme = self.theme();
        self.design_effect_regions.push(area);
        let Some(drawer) = Drawer::paint(f, &theme, "最近作品", area) else {
            return;
        };
        self.recent_ui.area = Some(area);
        self.recent_ui.close = Some(drawer.close);
        let rows = visible_rows(
            navigation.entries.len(),
            navigation.selected,
            drawer.content.height as usize,
            &mut self.recent_ui.scroll,
        );
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
            let (title, path) = if let Target::Project(root) = &entry.target {
                let title = self
                    .recent_projects()
                    .iter()
                    .find(|p| p.root == *root)
                    .map_or(entry.label.as_str(), |p| p.title.as_str());
                (safe_label(title), safe_label(&root.to_string_lossy()))
            } else {
                (safe_label(&entry.label), String::new())
            };
            chrome_line(
                f,
                row,
                &format!("  {title}"),
                if row.width >= 70 { &path } else { "" },
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
            self.recent_ui.hits.push((row, i));
        }
        if navigation.entries.is_empty() {
            f.render_widget(
                Paragraph::new("没有匹配的作品").style(theme.muted),
                drawer.content,
            );
        }
        let selected = navigation.entries.get(navigation.selected);
        let detail = match selected.map(|entry| &entry.target) {
            Some(Target::Project(root)) => safe_label(&root.to_string_lossy()),
            _ => format!("{} 项", navigation.entries.len()),
        };
        let can_remove = matches!(
            selected.map(|entry| &entry.target),
            Some(Target::Project(_))
        );
        let hint = if can_remove {
            if drawer.hint.width >= 40 {
                "Enter 打开 · Ctrl+D 移除记录"
            } else {
                "Ctrl+D 移除记录"
            }
        } else {
            "Enter 打开"
        };
        if can_remove && drawer.hint.width < 80 {
            // Prioritize the complete action over a duplicate path in narrow drawers.
            f.render_widget(Paragraph::new(hint).style(theme.muted), drawer.hint);
        } else {
            chrome_line(f, drawer.hint, &detail, hint, theme.muted, theme.muted);
        }
        let n = self.navigator.as_ref().unwrap();
        let (query, cursor) = self
            .recent_ui
            .query
            .view(&n.query, drawer.entry.width.saturating_sub(2));
        let label = if n.query.is_empty() {
            fit_label("筛选作品或路径…", drawer.entry.width.saturating_sub(2)).into_owned()
        } else {
            query
        };
        f.render_widget(
            Paragraph::new(format!("› {label}")).style(theme.surface),
            drawer.entry,
        );
        f.set_cursor_position((drawer.entry.x + 2 + cursor, drawer.entry.y));
    }
    pub(crate) fn recent_mouse(&mut self, event: MouseEvent) -> io::Result<bool> {
        if !self.recent_visible() {
            return Ok(false);
        }
        let pos = Position::new(event.column, event.row);
        let key = match event.kind {
            MouseEventKind::Down(MouseButton::Left)
                if self.recent_ui.close.is_some_and(|r| r.contains(pos)) =>
            {
                Some(KeyCode::Esc)
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, selected)) =
                    self.recent_ui.hits.iter().find(|(r, _)| r.contains(pos))
                {
                    self.navigator.as_mut().unwrap().selected = *selected;
                    Some(KeyCode::Enter)
                } else {
                    None
                }
            }
            MouseEventKind::ScrollUp if self.recent_ui.area.is_some_and(|r| r.contains(pos)) => {
                Some(KeyCode::Up)
            }
            MouseEventKind::ScrollDown if self.recent_ui.area.is_some_and(|r| r.contains(pos)) => {
                Some(KeyCode::Down)
            }
            _ => None,
        };
        if let Some(code) = key {
            self.navigation_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))?;
        }
        // This is modal: clicks outside the drawer never edit the manuscript.
        Ok(true)
    }
}
