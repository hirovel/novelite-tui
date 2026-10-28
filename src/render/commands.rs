use super::{
    chrome_line, fit_label, safe_label,
    surface::{self, Inspector},
};
use crate::Workspace;
use ratatui::{
    layout::{Alignment, Rect},
    style::Modifier,
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(super) fn draw(work: &mut Workspace, f: &mut ratatui::Frame, area: Rect) {
    if work.commands.palette_selection().is_none() {
        return;
    }
    let theme = work.theme();
    work.design_effect_regions.push(area);
    let (query_line, cursor) = work.commands.query_view(area.width.saturating_sub(2));
    let palette = work.commands.palette().unwrap();
    let paragraph_style = palette.query == "当前段落";
    let title = if paragraph_style {
        "段落样式"
    } else {
        "命令中心"
    };
    let Some(dock) = Inspector::paint(f, &theme, title, area) else {
        return;
    };
    work.design_ui.palette_close = Some(dock.close);
    let query = Rect::new(dock.content.x, dock.content.y, dock.content.width, 1);
    f.render_widget(
        Paragraph::new("› ").style(theme.accent),
        Rect::new(query.x, query.y, 2, 1),
    );
    f.render_widget(
        Paragraph::new(if palette.query.is_empty() {
            "搜索全部操作…"
        } else {
            &query_line
        })
        .style(if palette.query.is_empty() {
            theme.muted
        } else {
            theme.surface
        }),
        Rect::new(query.x + 2, query.y, query.width - 2, 1),
    );
    f.set_cursor_position((query.x + 2 + cursor, query.y));
    let results = Rect::new(
        dock.content.x,
        query.bottom(),
        dock.content.width,
        dock.content.height - 1,
    );
    let list_width = palette
        .entries
        .iter()
        .map(|c| c.title.width() as u16 + 4)
        .max()
        .unwrap_or(32)
        .clamp(32, 44);
    let columns = Inspector::columns(f, &theme, results, list_width, 32);
    let list = columns.map_or(results, |(list, _)| list);
    let visible = surface::visible_rows(
        palette.entries.len(),
        palette.selected,
        list.height as usize,
        &mut work.design_ui.palette_scroll,
    );
    for i in visible.clone() {
        let entry = palette.entries[i];
        let r = Rect::new(list.x, list.y + (i - visible.start) as u16, list.width, 1);
        let label = safe_label(&entry.title);
        if i == palette.selected {
            surface::selection_row(f, &theme, r, &label, &work.commands.shortcuts(&entry.id));
        } else {
            chrome_line(f, r, &format!("  {label}"), "", theme.surface, theme.muted);
        }
        work.design_ui.palette_hits.push((r, entry.id.clone()));
    }
    if palette.entries.is_empty() {
        f.render_widget(Paragraph::new("没有匹配的操作").style(theme.muted), list);
    }
    if let Some((_, detail)) = columns {
        if let Some(entry) = palette.entries.get(palette.selected) {
            f.render_widget(
                Paragraph::new(fit_label(&safe_label(&entry.title), detail.width))
                    .style(theme.accent),
                Rect::new(detail.x, detail.y, detail.width, 1),
            );
            let description = Rect::new(
                detail.x,
                detail.y + 1,
                detail.width,
                detail.height.saturating_sub(1),
            );
            f.render_widget(
                Paragraph::new(work.commands.description(&entry.id))
                    .style(theme.surface)
                    .wrap(Wrap { trim: false }),
                description,
            );
        } else {
            f.render_widget(
                Paragraph::new("输入操作名称或命令 ID，缩短关键词后重试。")
                    .style(theme.muted)
                    .wrap(Wrap { trim: false }),
                detail,
            );
        }
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
        "可撤销".to_owned()
    } else if count > visible.len() {
        format!("{scope} {}–{} / {count}", visible.start + 1, visible.end)
    } else {
        format!("{scope} {count} 项")
    };
    f.render_widget(
        Paragraph::new(fit_label(&status, dock.heading_note.width))
            .style(theme.surface.patch(theme.muted))
            .alignment(Alignment::Right),
        dock.heading_note,
    );
    let help = if count == 0 {
        "换个关键词"
    } else if dock.footer.width >= "↑↓ 选择  Enter 执行".width() as u16 {
        "↑↓ 选择  Enter 执行"
    } else {
        "Enter 执行"
    };
    f.render_widget(
        Paragraph::new(fit_label(help, dock.footer.width))
            .style(theme.muted.remove_modifier(Modifier::REVERSED)),
        dock.footer,
    );
}
