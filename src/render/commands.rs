use super::{
    chrome_line, safe_label,
    surface::{self, Drawer},
};
use crate::Workspace;
use ratatui::{layout::Rect, widgets::Paragraph};

pub(super) fn draw(work: &mut Workspace, f: &mut ratatui::Frame, area: Rect) {
    let Some(palette) = work.palette() else {
        return;
    };
    let theme = work.theme();
    work.design_effect_regions.push(area);
    let paragraph_style = palette.query == "当前段落";
    let title = if paragraph_style {
        "段落样式"
    } else {
        "命令中心"
    };
    let Some(drawer) = Drawer::paint(f, &theme, title, area) else {
        return;
    };
    work.design_ui.palette_close = Some(drawer.close);
    let results = drawer.content;
    let capacity = results.height as usize;
    let visible = surface::visible_rows(
        palette.entries.len(),
        palette.selected,
        capacity,
        &mut work.design_ui.palette_scroll,
    );
    let first = visible.start;
    for (i, entry) in palette
        .entries
        .iter()
        .enumerate()
        .skip(first)
        .take(visible.len())
    {
        let r = Rect::new(results.x, results.y + (i - first) as u16, results.width, 1);
        let selected = i == palette.selected;
        let style = if selected {
            theme.surface.add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            theme.surface
        };
        chrome_line(
            f,
            r,
            &format!("  {}", safe_label(&entry.title)),
            if selected { &entry.shortcuts } else { "" },
            style,
            theme.muted,
        );
        if selected {
            f.render_widget(
                Paragraph::new("▎").style(theme.accent),
                Rect::new(r.x, r.y, 1, 1),
            );
        }
        work.design_ui.palette_hits.push((r, entry.id.clone()));
    }
    if palette.entries.is_empty() {
        f.render_widget(Paragraph::new("没有匹配的操作").style(theme.muted), results);
    }
    let count = palette.entries.len();
    let scope = if !palette.query.is_empty() {
        "搜索"
    } else if palette
        .entries
        .iter()
        .any(|entry| entry.id == "browse-commands")
    {
        "常用"
    } else {
        "全部"
    };
    let status = if paragraph_style {
        "可撤销".into()
    } else if count > visible.len() {
        format!("{scope} · {}–{} / {count}", first + 1, visible.end)
    } else {
        format!("{scope} · {count} 项操作")
    };
    chrome_line(
        f,
        drawer.hint,
        &status,
        if count == 0 {
            "换个关键词"
        } else if drawer.hint.width >= 70 {
            "↑↓ 选择 · Enter 执行"
        } else {
            "Enter 执行"
        },
        theme.muted,
        theme.muted,
    );
    let (line, cursor) = work
        .commands
        .query_view(drawer.entry.width.saturating_sub(2));
    f.render_widget(
        Paragraph::new(format!(
            "› {}",
            if palette.query.is_empty() {
                "搜索全部操作…"
            } else {
                &line
            }
        ))
        .style(theme.surface),
        drawer.entry,
    );
    f.set_cursor_position((drawer.entry.x + 2 + cursor, drawer.entry.y));
}
