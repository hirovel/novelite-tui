use super::safe_label;
use crate::Workspace;
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub(super) fn draw(
    work: &mut Workspace,
    frame: &mut ratatui::Frame,
    body: Rect,
    dock: Rect,
    message: &str,
) {
    let area = frame.area();
    let theme = work.theme();
    if let Some(preview) =
        work.history_preview(body.width.saturating_sub(2), body.height.saturating_sub(2))
    {
        let block = Block::default()
            .style(theme.surface)
            .border_style(theme.border)
            .border_type(BorderType::Rounded)
            .borders(Borders::ALL)
            .title("历史预览 · 方向键浏览 · Enter 备份当前稿并恢复 · Esc 返回");
        let inner = block.inner(body);
        frame.render_widget(Clear, body);
        frame.render_widget(block, body);
        frame.render_widget(
            Paragraph::new(
                preview
                    .lines
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            ),
            inner,
        );
        if let Some((x, y)) = preview.cursor {
            frame.set_cursor_position((inner.x + x, inner.y + y));
        }
    }
    if work.navigator.as_ref().is_some_and(|n| {
        n.panel_module().is_none() && !matches!(n.kind, crate::navigator::Kind::Recent)
    }) && let Some(navigation) = work.navigation()
    {
        let panel = Rect::new(0, 1, area.width.min(76), area.height.saturating_sub(3));
        let block = Block::default()
            .style(theme.surface)
            .border_style(theme.border)
            .border_type(BorderType::Rounded)
            .borders(Borders::ALL)
            .title(navigation.title);
        let inner = block.inner(panel);
        frame.render_widget(Clear, panel);
        frame.render_widget(block, panel);
        let mut lines = vec![Line::from(safe_label(&format!("> {}", navigation.query)))];
        let capacity = inner.height.saturating_sub(1) as usize;
        let first = navigation
            .selected
            .saturating_sub(capacity.saturating_sub(1));
        for (index, entry) in navigation
            .entries
            .iter()
            .enumerate()
            .skip(first)
            .take(capacity)
        {
            let line = Line::from(safe_label(&entry.label));
            lines.push(if index == navigation.selected {
                line.style(theme.selection)
            } else {
                line
            });
        }
        frame.render_widget(Paragraph::new(lines), inner);
        if inner.width > 0 && inner.height > 0 {
            let x = (unicode_width::UnicodeWidthStr::width(navigation.query.as_str()) + 2)
                .min(inner.width.saturating_sub(1) as usize) as u16;
            frame.set_cursor_position((inner.x + x, inner.y));
        }
    }
    work.render_import(
        frame,
        Rect::new(0, 1, area.width, area.height.saturating_sub(3)),
    );
    work.render_recent(frame, dock);
    super::commands::draw(work, frame, dock);
    work.render_design_settings(frame, dock, message);
    if let Some(prompt) = &mut work.prompt {
        let height = if prompt.editor.is_some() {
            area.height.saturating_sub(3)
        } else {
            area.height.saturating_sub(3).min(5)
        };
        let panel = Rect::new(
            0,
            1 + (area.height.saturating_sub(3) - height) / 2,
            area.width,
            height,
        );
        let block = Block::default()
            .style(theme.surface)
            .border_style(theme.border)
            .border_type(BorderType::Rounded)
            .borders(Borders::ALL)
            .title(safe_label(&prompt.title));
        let inner = block.inner(panel);
        frame.render_widget(Clear, panel);
        frame.render_widget(block, panel);
        if let Some(editor) = &mut prompt.editor {
            prompt.editor_area = inner;
            let view = editor.view(inner.width, inner.height);
            frame.render_widget(
                Paragraph::new(view.lines.into_iter().map(Line::from).collect::<Vec<_>>()),
                inner,
            );
            for (x, y, width) in view.selection {
                frame.buffer_mut().set_style(
                    Rect::new(inner.x + x, inner.y + y, width, 1),
                    theme.selection,
                );
            }
            if let Some((x, y)) = view.cursor {
                frame.set_cursor_position((inner.x + x, inner.y + y));
            }
            return;
        }
        let (line, cursor_column) = prompt.line_view(inner.width);
        frame.render_widget(
            Paragraph::new(format!("{line}\nEnter 确认 · Esc 取消")),
            inner,
        );
        if inner.width > 0 && inner.height > 0 {
            frame.set_cursor_position((inner.x + cursor_column, inner.y));
        }
    }
}
