use super::safe_label;
use crate::Workspace;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};

pub(super) fn draw(work: &mut Workspace, frame: &mut ratatui::Frame, body: Rect) {
    let theme = work.theme();
    for pane in work.pane_views(body) {
        if let Some(heading) = pane.heading {
            work.design_effect_regions.push(heading);
            let title = format!(
                "{} {} · {}",
                if pane.active { ">" } else { " " },
                pane.index + 1,
                pane.title
            );
            frame.render_widget(
                Paragraph::new(safe_label(&title)).style(if pane.active {
                    theme.selection
                } else {
                    theme.muted
                }),
                heading,
            );
        }
        frame.render_widget(
            Paragraph::new(
                pane.view
                    .lines
                    .into_iter()
                    .zip(&pane.view.headings)
                    .map(|(line, heading)| {
                        if *heading {
                            Line::styled(line, work.heading_style())
                        } else {
                            Line::from(line)
                        }
                    })
                    .collect::<Vec<_>>(),
            ),
            pane.area,
        );
        for (x, y, width, active) in pane.view.focus {
            frame.buffer_mut().set_style(
                Rect::new(pane.area.x + x, pane.area.y + y, width, 1),
                Style::default().add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::DIM
                }),
            );
        }
        for (x, y, width) in pane.view.dialogue {
            if pane.view.headings.get(y as usize) == Some(&true) {
                continue;
            }
            frame.buffer_mut().set_style(
                Rect::new(pane.area.x + x, pane.area.y + y, width, 1),
                theme.dialogue,
            );
        }
        for (x, y, width) in pane.view.selection {
            frame.buffer_mut().set_style(
                Rect::new(pane.area.x + x, pane.area.y + y, width, 1),
                theme.selection,
            );
        }
        if pane.active
            && !work.design_ui.open
            && work.commands.palette_selection().is_none()
            && let Some((x, y)) = pane.view.cursor
        {
            frame.set_cursor_position((pane.area.x + x, pane.area.y + y));
        }
    }
}
