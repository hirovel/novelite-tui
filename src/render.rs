//! Paint order is explicit: background → manuscript → modules → chrome → overlays.
//! Layout rectangles are also passed to pane/module input mapping; never infer mouse
//! coordinates separately from the rectangles used to draw the text.
mod chrome;
mod commands;
mod geometry;
mod manuscript;
mod overlays;
mod panels;
mod search;
pub(crate) mod surface;
use crate::Workspace;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Paragraph},
};

impl Workspace {
    pub fn render(&mut self, frame: &mut ratatui::Frame<'_>, message: &str) {
        self.design_effect_regions.clear();
        self.begin_design_frame();
        let area = frame.area();
        if area.is_empty() {
            return;
        }
        frame.render_widget(Block::default().style(self.theme().base), area);
        let notice = chrome::notice(self, message);
        let geometry = geometry::ReadingLayout::new(
            area,
            self.chapter_search().is_some(),
            notice.is_some(),
            self.bottom_tool_open(),
        );
        let layout = self.workspace_layout(geometry.workspace);
        let body = self.editor_area(layout.editor);
        manuscript::draw(self, frame, body);
        search::draw(self, frame, geometry.search);
        panels::draw(self, frame, &layout.panels);
        chrome::draw(self, frame, &geometry, body, notice.as_ref());
        // Use the full writing region, including both panes, as the tool axis.
        // The frame background already covers the margins outside this span.
        let dock = Rect::new(body.x, geometry.dock.y, body.width, geometry.dock.height);
        overlays::draw(self, frame, body, dock, message);
    }
}

pub(crate) fn safe_label(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '�' } else { ch })
        .collect()
}

// Column-aware chrome: long CJK titles yield to status rather than hiding it.
pub(crate) fn chrome_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    left: &str,
    right: &str,
    base: Style,
    detail: Style,
) {
    use unicode_width::UnicodeWidthStr;
    frame.render_widget(Block::default().style(base), area);
    let rw = right.width().min(usize::from(area.width / 2)) as u16;
    let lw = area.width.saturating_sub(rw + u16::from(rw > 0));
    frame.render_widget(
        Paragraph::new(fit_label(left, lw)).style(base),
        Rect::new(area.x, area.y, lw, 1),
    );
    frame.render_widget(
        Paragraph::new(fit_label(right, rw))
            .style(detail)
            .alignment(ratatui::layout::Alignment::Right),
        Rect::new(area.right() - rw, area.y, rw, 1),
    );
}

/// Truncate chrome at grapheme boundaries; prose uses the document layout instead.
pub(crate) fn fit_label(text: &str, width: u16) -> std::borrow::Cow<'_, str> {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let width = usize::from(width);
    if text.width() <= width {
        return text.into();
    }
    if width == 0 {
        return "".into();
    }
    let mut used = 0;
    let mut end = 0;
    for (offset, glyph) in text.grapheme_indices(true) {
        let columns = glyph.width();
        if used + columns > width - 1 {
            break;
        }
        used += columns;
        end = offset + glyph.len();
    }
    format!("{}…", &text[..end]).into()
}
