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
    work.render_navigation_preview(frame, body);
    work.render_import(
        frame,
        Rect::new(0, 1, area.width, area.height.saturating_sub(3)),
    );
    work.render_navigation_tool(frame, dock);
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
        frame.render_widget(Paragraph::new(format!("{line}\n{}", prompt.hint())), inner);
        if inner.width > 0 && inner.height > 0 {
            frame.set_cursor_position((inner.x + cursor_column, inner.y));
        }
    }
}
