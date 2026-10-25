use super::safe_label;
use crate::Workspace;
use ratatui::{
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub(super) fn draw(
    work: &mut Workspace,
    frame: &mut ratatui::Frame,
    panels: &[crate::workspace_layout::PanelPlacement],
) {
    let theme = work.theme();
    // Built-in modules use the same host layout and renderer.
    for panel in panels {
        let panel_style = if panel.overlay {
            theme.surface
        } else {
            theme.base
        };
        let block = Block::default()
            .style(panel_style)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let inner = crate::workspace_layout::panel_content_rect(panel.area);
        work.design_effect_regions.push(panel.area);
        let content = work.module_panel(panel.id, inner.height as usize);
        let block = block
            .borders(if panel.overlay {
                Borders::ALL
            } else {
                Borders::NONE
            })
            .title_style(theme.accent)
            .title(format!(
                " {} ",
                safe_label(
                    content
                        .title
                        .split('·')
                        .next()
                        .unwrap_or(&content.title)
                        .trim()
                )
            ))
            .border_style(if panel.focused {
                theme.accent
            } else {
                theme.border
            });
        frame.render_widget(Block::default().style(theme.base), panel.area);
        frame.render_widget(block, panel.area);
        let mut lines: Vec<Line> = content
            .headers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Line::from(safe_label(s)).style(
                    if panel.id == crate::ModuleId::Planning && i == 1 {
                        theme.accent
                    } else {
                        theme.muted
                    },
                )
            })
            .collect();
        for (offset, text) in content.entries.iter().enumerate() {
            let line = Line::from(safe_label(text));
            lines.push(
                if (panel.focused && content.first + offset == content.selected)
                    || (!panel.focused
                        && panel.id == crate::ModuleId::Chapters
                        && text.trim_start().starts_with('•'))
                {
                    line.style(theme.selection)
                } else {
                    line
                },
            );
        }
        frame.render_widget(Paragraph::new(lines), inner);
        if panel.focused && inner.width > 0 && inner.height > 0 {
            frame.set_cursor_position((inner.x, inner.y));
        }
    }
}
