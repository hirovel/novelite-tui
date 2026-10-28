use super::{CATEGORIES, Hit};
use crate::{
    Workspace,
    render::{
        chrome_line, fit_label,
        surface::{self, Inspector},
    },
};
use ratatui::{
    layout::{Alignment, Rect},
    style::Modifier,
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

impl Workspace {
    pub(crate) fn render_design_settings(
        &mut self,
        f: &mut ratatui::Frame,
        area: Rect,
        message: &str,
    ) {
        if !self.design_ui.open {
            return;
        }
        let theme = self.theme();
        self.design_effect_regions.push(area);
        let Some(dock) = Inspector::paint(f, &theme, "设置", area) else {
            return;
        };
        self.design_ui.hits.push((dock.close, Hit::Close));
        let terminal_note = "字体、光标沿用终端";
        if dock.heading_note.width >= terminal_note.width() as u16 + 2 {
            f.render_widget(
                Paragraph::new(terminal_note)
                    .style(theme.surface.patch(theme.muted))
                    .alignment(Alignment::Right),
                dock.heading_note,
            );
        }
        // Twelve cells fit every category. The property side keeps enough space
        // for labels, values and a useful explanation; narrow writing columns
        // switch to tabs even when the outer terminal itself is wide.
        let columns = Inspector::columns(f, &theme, dock.content, 12, 64);
        let content = columns.map_or(
            Rect::new(
                dock.content.x,
                dock.content.y + 1,
                dock.content.width,
                dock.content.height.saturating_sub(1),
            ),
            |(_, content)| content,
        );
        for (i, title) in CATEGORIES.iter().enumerate() {
            let r = if let Some((rail, _)) = columns {
                Rect::new(rail.x, rail.y + i as u16, rail.width, 1)
            } else {
                let width = dock.content.width / CATEGORIES.len() as u16;
                Rect::new(dock.content.x + i as u16 * width, dock.content.y, width, 1)
            };
            if !dock.content.contains((r.x, r.y).into()) {
                continue;
            }
            f.render_widget(
                Paragraph::new(format!(" {title}")).style(if i == self.design_ui.category {
                    theme.accent
                } else {
                    theme.muted
                }),
                r,
            );
            self.design_ui.hits.push((r, Hit::Category(i)));
        }
        let rows = self.settings_rows();
        let selected = self.design_ui.selected.min(rows.len().saturating_sub(1));
        // The detail yields before the last usable property row. Long categories
        // scroll within the same viewport; selecting never changes dock height.
        let detail_height = if content.height > 1 {
            content.height.saturating_sub(rows.len() as u16).clamp(1, 2)
        } else {
            0
        };
        let properties = Rect::new(
            content.x,
            content.y,
            content.width,
            content.height - detail_height,
        );
        let visible = surface::visible_rows(
            rows.len(),
            selected,
            properties.height as usize,
            &mut self.design_ui.scroll,
        );
        for i in visible.clone() {
            let row = &rows[i];
            let r = Rect::new(
                properties.x,
                properties.y + (i - visible.start) as u16,
                properties.width,
                1,
            );
            if i == selected {
                surface::selection_row(f, &theme, r, row.title, &row.value);
                self.design_ui.can_apply = true;
            } else {
                chrome_line(
                    f,
                    r,
                    &format!("  {}", row.title),
                    &row.value,
                    theme.surface,
                    theme.muted,
                );
            }
            self.design_ui.hits.push((r, Hit::Option(i)));
        }
        let note = if !message.is_empty() {
            message
        } else if !self.design_ui.can_apply {
            "请扩大窗口查看选项"
        } else {
            rows[selected].detail
        };
        if detail_height > 0 {
            let detail = Rect::new(
                content.x + 2,
                content.bottom() - detail_height,
                content.width.saturating_sub(2),
                detail_height,
            );
            f.render_widget(
                Paragraph::new(note)
                    .style(if message.is_empty() {
                        theme.muted
                    } else {
                        theme.warning
                    })
                    .wrap(Wrap { trim: false }),
                detail,
            );
        }
        let command_width = 6.min(dock.footer.width);
        let help_area = Rect::new(
            dock.footer.x + command_width + 1,
            dock.footer.y,
            dock.footer.width.saturating_sub(command_width + 1),
            1,
        );
        let help = [
            "Tab 分类  ←→ 调整",
            "Tab 分类  Enter 切换",
            "Tab  ←→ 调整",
            "Enter 切换",
            "Enter",
        ]
        .into_iter()
        .find(|text| text.width() <= usize::from(help_area.width))
        .unwrap_or("");
        f.render_widget(
            Paragraph::new(fit_label(help, help_area.width))
                .style(theme.muted.remove_modifier(Modifier::REVERSED)),
            help_area,
        );
        let command = Rect::new(dock.footer.x, dock.footer.y, command_width, 1);
        f.render_widget(Paragraph::new("› 命令").style(theme.muted), command);
        self.design_ui.hits.push((command, Hit::Commands));
    }
}
