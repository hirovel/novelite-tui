use super::{CATEGORIES, Hit};
use crate::{
    Workspace,
    render::{
        chrome_line, fit_label,
        surface::{self, Drawer},
    },
};
use ratatui::{layout::Rect, style::Modifier, widgets::Paragraph};

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
        let Some(drawer) = Drawer::paint(f, &theme, "设置", area) else {
            return;
        };
        self.design_ui.hits.push((drawer.close, Hit::Close));
        let wide = drawer.content.width >= 80 && drawer.content.height >= 3;
        let content = if wide {
            Rect::new(
                drawer.content.x + 16,
                drawer.content.y,
                drawer.content.width - 16,
                drawer.content.height,
            )
        } else {
            Rect::new(
                drawer.content.x,
                drawer.content.y + 1,
                drawer.content.width,
                drawer.content.height.saturating_sub(1),
            )
        };
        for (i, title) in CATEGORIES.iter().enumerate() {
            let r = if wide {
                Rect::new(drawer.content.x, drawer.content.y + i as u16, 12, 1)
            } else {
                let width = drawer.content.width / CATEGORIES.len() as u16;
                Rect::new(
                    drawer.content.x + i as u16 * width,
                    drawer.content.y,
                    width,
                    1,
                )
            };
            if !drawer.content.contains((r.x, r.y).into()) {
                continue;
            }
            f.render_widget(
                Paragraph::new(format!(" {title}")).style(if i == self.design_ui.category {
                    theme.surface.add_modifier(Modifier::BOLD)
                } else {
                    theme.muted
                }),
                r,
            );
            self.design_ui.hits.push((r, Hit::Category(i)));
        }
        let rows = self.settings_rows();
        let selected = self.design_ui.selected.min(rows.len().saturating_sub(1));
        let visible = surface::visible_rows(
            rows.len(),
            selected,
            content.height as usize,
            &mut self.design_ui.scroll,
        );
        for i in visible.clone() {
            let row = &rows[i];
            let r = Rect::new(
                content.x,
                content.y + (i - visible.start) as u16,
                content.width,
                1,
            );
            let active = i == selected;
            // The same row rectangle paints the property and owns its click target.
            let value_offset = 14.min(r.width.saturating_sub(8));
            let label = Rect::new(r.x + 2, r.y, value_offset.saturating_sub(2), 1);
            f.render_widget(
                Paragraph::new(fit_label(row.title, label.width)).style(theme.surface),
                label,
            );
            let value = format!(" {} ", row.value);
            let value_width = unicode_width::UnicodeWidthStr::width(value.as_str()) as u16;
            let value_area = Rect::new(
                r.x + value_offset,
                r.y,
                value_width.min(r.width - value_offset),
                1,
            );
            f.render_widget(
                Paragraph::new(fit_label(&value, value_area.width)).style(if active {
                    theme
                        .surface
                        .patch(theme.accent)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    theme.surface
                }),
                value_area,
            );
            if active {
                f.render_widget(
                    Paragraph::new("▎").style(theme.accent),
                    Rect::new(r.x, r.y, 1, 1),
                );
                self.design_ui.can_apply = true;
            }
            self.design_ui.hits.push((r, Hit::Option(i)));
        }
        if self.design_ui.category == 0 && content.height > rows.len() as u16 + 1 {
            f.render_widget(
                Paragraph::new("字体与光标外观沿用终端").style(theme.muted),
                Rect::new(
                    content.x + 2,
                    content.y + rows.len() as u16 + 1,
                    content.width.saturating_sub(2),
                    1,
                ),
            );
        }
        let note = if !message.is_empty() {
            message
        } else if !self.design_ui.can_apply {
            "请扩大窗口查看选项"
        } else {
            rows[selected].detail
        };
        chrome_line(
            f,
            drawer.hint,
            note,
            "",
            if message.is_empty() {
                theme.muted
            } else {
                theme.warning
            },
            theme.muted,
        );
        // The command button has a fixed readable width; help uses all remaining
        // columns. Pick a complete phrase instead of clipping a key name in half.
        use unicode_width::UnicodeWidthStr;
        let command_width = 6.min(drawer.entry.width);
        let help_width = drawer.entry.width.saturating_sub(command_width + 1);
        let help = [
            "Tab 分类 · ←→ 调整 · Enter 切换",
            "Tab 分类 · ←→/Enter 调整",
            "Tab · ←→ · Enter",
            "←→ · Enter",
            "Enter",
        ]
        .into_iter()
        .find(|text| text.width() <= usize::from(help_width))
        .unwrap_or("");
        f.render_widget(
            Paragraph::new("› 命令").style(theme.muted),
            Rect::new(drawer.entry.x, drawer.entry.y, command_width, 1),
        );
        f.render_widget(
            Paragraph::new(help)
                .style(theme.muted)
                .alignment(ratatui::layout::Alignment::Right),
            Rect::new(
                drawer.entry.right() - help_width,
                drawer.entry.y,
                help_width,
                1,
            ),
        );
        self.design_ui.hits.push((
            Rect::new(drawer.entry.x, drawer.entry.y, 6.min(drawer.entry.width), 1),
            Hit::Commands,
        ));
    }
}
