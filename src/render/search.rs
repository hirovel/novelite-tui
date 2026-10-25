use super::safe_label;
use crate::Workspace;
use ratatui::{layout::Rect, text::Line, widgets::Paragraph};

pub(super) fn draw(work: &mut Workspace, frame: &mut ratatui::Frame, area: Rect) {
    if let Some(search) = work.chapter_search() {
        let status = if search.stale {
            "结果已过期".into()
        } else if search.total == 0 {
            "无结果".into()
        } else {
            format!(
                "{} / {}",
                search.current.map_or("—".into(), |i| i.to_string()),
                search.total
            )
        };
        let lines = vec![
            Line::from(safe_label(&format!(
                "查找 · {} · 修订 {} · {} · 文字匹配{}",
                search.chapter,
                search.revision,
                status,
                if search.case_sensitive {
                    " · 区分大小写"
                } else {
                    " · 忽略大小写"
                }
            ))),
            Line::from(safe_label(&format!("查询：{}", search.query))),
            Line::from(safe_label(&search.context)),
            Line::from(format!(
                "{} 下一个 · {} 上一个 · F1 替换/大小写 · Esc 关闭",
                work.command_shortcuts("find-next"),
                work.command_shortcuts("find-previous")
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}
