//! Bottom drawer chrome shared by commands and settings; no document mutations.
use crate::appearance::Theme;
use ratatui::{
    layout::Rect,
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

/// Commands and settings share one bounded workbench. The returned rectangles
/// are also the input targets; the inspector never owns focus or writes state.
pub(crate) struct Inspector {
    pub content: Rect,
    pub footer: Rect,
    pub close: Rect,
    pub heading_note: Rect,
}

impl Inspector {
    pub fn paint(f: &mut ratatui::Frame, theme: &Theme, title: &str, area: Rect) -> Option<Self> {
        f.render_widget(Block::default().style(theme.surface), area);
        if area.width < 24 || area.height < 4 {
            f.render_widget(
                Paragraph::new("请扩大窗口 · Esc 返回").style(theme.muted),
                area,
            );
            return None;
        }
        let heading = format!(" {title} ");
        let title_width = (heading.width() as u16).min(area.width);
        f.render_widget(
            Paragraph::new("─".repeat(area.width as usize)).style(theme.border),
            Rect::new(area.x, area.y, area.width, 1),
        );
        f.render_widget(
            Paragraph::new(heading).style(theme.surface.patch(theme.accent)),
            Rect::new(area.x, area.y, title_width, 1),
        );
        let back = "Esc 返回正文";
        let close = Rect::new(
            area.right() - back.width() as u16,
            area.bottom() - 1,
            back.width() as u16,
            1,
        );
        f.render_widget(Paragraph::new(back).style(theme.muted), close);
        Some(Self {
            content: Rect::new(area.x, area.y + 1, area.width, area.height - 2),
            footer: Rect::new(
                area.x,
                area.bottom() - 1,
                area.width.saturating_sub(close.width + 1),
                1,
            ),
            close,
            heading_note: Rect::new(area.x + title_width, area.y, area.width - title_width, 1),
        })
    }

    /// Split only when both content columns fit, irrespective of terminal width.
    /// Painting and all hit targets use these same column rectangles.
    pub fn columns(
        f: &mut ratatui::Frame,
        theme: &Theme,
        area: Rect,
        left_width: u16,
        right_minimum: u16,
    ) -> Option<(Rect, Rect)> {
        if area.width < left_width + 3 + right_minimum {
            return None;
        }
        let left = Rect::new(area.x, area.y, left_width, area.height);
        let right = Rect::new(
            left.right() + 3,
            area.y,
            area.width - left_width - 3,
            area.height,
        );
        f.render_widget(
            Paragraph::new("│\n".repeat(area.height as usize)).style(theme.border),
            Rect::new(left.right() + 1, area.y, 1, area.height),
        );
        Some((left, right))
    }
}

pub(crate) fn selection_row(
    f: &mut ratatui::Frame,
    theme: &Theme,
    area: Rect,
    label: &str,
    value: &str,
) {
    let selected = theme.selection.add_modifier(ratatui::style::Modifier::BOLD);
    super::chrome_line(f, area, &format!("  {label}"), value, selected, selected);
    // The accent sits outside the neutral selection band, so reverse video
    // cannot turn it into a colored block on a terminal's own background.
    f.render_widget(
        Paragraph::new("▎").style(theme.surface.patch(theme.accent)),
        Rect::new(area.x, area.y, 1, 1),
    );
}

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
