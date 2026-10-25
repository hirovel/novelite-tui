//! Bottom drawer chrome shared by commands and settings; no document mutations.
use crate::appearance::Theme;
use ratatui::{
    layout::Rect,
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(crate) struct Drawer {
    pub content: Rect,
    pub hint: Rect,
    pub entry: Rect,
    pub close: Rect,
}
impl Drawer {
    /// The shell reserves this area before laying out the manuscript. Geometry is
    /// final on the first frame; motion changes colors, never input coordinates.
    pub fn paint(f: &mut ratatui::Frame, theme: &Theme, title: &str, area: Rect) -> Option<Self> {
        f.render_widget(Clear, area);
        f.render_widget(Block::default().style(theme.surface), area);
        if area.width < 24 || area.height < 4 {
            f.render_widget(
                Paragraph::new("请扩大窗口 · Esc 返回").style(theme.muted),
                area,
            );
            return None;
        }
        // The shell supplies the manuscript's horizontal span. Do not pad it again.
        let inner = area;
        let title_y = area.y;
        let content_y = area.y + 1;
        f.render_widget(
            Paragraph::new("─".repeat(inner.width as usize)).style(theme.border),
            Rect::new(inner.x, title_y, inner.width, 1),
        );
        let close = Rect::new(inner.right() - 8, title_y, 8, 1);
        let heading = format!(" {title} ");
        let heading_width = (heading.width() as u16).min(inner.width - 10);
        f.render_widget(
            Paragraph::new(heading).style(theme.surface.patch(theme.muted)),
            Rect::new(inner.x, title_y, heading_width, 1),
        );
        f.render_widget(
            Paragraph::new("关闭 Esc").style(theme.surface.patch(theme.muted)),
            close,
        );
        Some(Self {
            content: Rect::new(
                inner.x,
                content_y,
                inner.width,
                area.bottom() - 2 - content_y,
            ),
            hint: Rect::new(inner.x, area.bottom() - 2, inner.width, 1),
            entry: Rect::new(inner.x, area.bottom() - 1, inner.width, 1),
            close,
        })
    }
}
/// Keep a stable list viewport; scrolling only follows selection out of bounds.
pub(crate) fn visible_rows(
    count: usize,
    selected: usize,
    capacity: usize,
    first: &mut usize,
) -> std::ops::Range<usize> {
    if capacity == 0 || count == 0 {
        *first = 0;
        return 0..0;
    }
    let selected = selected.min(count - 1);
    *first = (*first)
        .min(count.saturating_sub(capacity))
        .min(selected)
        .max(selected.saturating_sub(capacity - 1));
    *first..(*first + capacity).min(count)
}
