use ratatui::layout::Rect;

/// Shell rows yield to writing space. The editor owns its reading-width padding.
pub(super) struct ReadingLayout {
    pub header: Rect,
    pub workspace: Rect,
    pub search: Rect,
    pub notice: Rect,
    pub footer: Rect,
    pub dock: Rect,
}

impl ReadingLayout {
    pub fn new(area: Rect, searching: bool, has_notice: bool, dock_open: bool) -> Self {
        let header = u16::from(area.height >= 6);
        let notice = u16::from(has_notice && area.height >= 3);
        // A shared height prevents the manuscript jumping when switching modes.
        // Short terminals retain writing space and show a compact resize hint.
        let dock = if dock_open {
            (area.height / 2).clamp(1, if area.width >= 100 { 8 } else { 9 })
        } else {
            1
        };
        let available = area.height.saturating_sub(header + notice + dock);
        let search = if searching {
            available.saturating_sub(1).min(4)
        } else {
            0
        };
        let workspace = available.saturating_sub(search);
        let row = |y, height| Rect::new(area.x, y, area.width, height);
        Self {
            header: row(area.y, header),
            workspace: row(area.y + header, workspace),
            search: row(area.y + header + workspace, search),
            notice: row(area.bottom().saturating_sub(dock + notice), notice),
            footer: row(area.bottom().saturating_sub(1), 1),
            dock: row(area.bottom().saturating_sub(dock), dock),
        }
    }
}
