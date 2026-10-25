use super::{geometry::ReadingLayout, safe_label};
use crate::{SaveState, Workspace};
use ratatui::{layout::Rect, widgets::Paragraph};

pub(super) struct Notice {
    text: String,
    warning: bool,
}

/// Preserve actionable feedback while keeping routine statistics out of the notice row.
pub(super) fn notice(work: &Workspace, message: &str) -> Option<Notice> {
    let warning = if !message.is_empty() {
        Some(message)
    } else {
        work.external_conflict()
            .map(|c| c.message.as_str())
            .or_else(|| work.configuration_warning())
            .or_else(|| work.history_warning())
    };
    if let Some(text) = warning {
        return Some(Notice {
            text: text.to_owned(),
            warning: true,
        });
    }
    let save_warning = match work.save_state() {
        SaveState::Failed(_) => Some("保存失败，请从命令中重试或另存"),
        SaveState::ReadOnly => Some("只读：作品正在其他实例中打开"),
        SaveState::Conflict(_) => Some("外部修改冲突，请从命令中处理"),
        _ => None,
    };
    if let Some(text) = save_warning {
        return Some(Notice {
            text: text.into(),
            warning: true,
        });
    }
    let text = work
        .export_message()
        .or_else(|| work.clipboard_message())
        .or_else(|| work.pane_message())
        .map(str::to_owned)
        .or_else(|| {
            work.book_search().map(|s| {
                format!(
                    "全书搜索{} · {} 条结果",
                    if s.running { "中" } else { "完成" },
                    s.total
                )
            })
        });
    text.filter(|s| !s.is_empty()).map(|text| Notice {
        text,
        warning: false,
    })
}

pub(super) fn draw(
    work: &mut Workspace,
    frame: &mut ratatui::Frame,
    layout: &ReadingLayout,
    body: Rect,
    notice: Option<&Notice>,
) {
    let theme = work.theme();
    // Header, writing region and tool dock share one horizontal span.
    let line = |r: Rect| {
        if body.width >= 24 {
            Rect::new(body.x, r.y, body.width, r.height)
        } else {
            r
        }
    };
    let header = line(layout.header);
    if !header.is_empty() {
        let title = work.project().map(|p| p.title.clone()).unwrap_or_else(|| {
            work.path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        frame.render_widget(
            Paragraph::new(safe_label(&title)).style(theme.muted),
            header,
        );
    }
    // The dock replaces the footer. Painting a second footer underneath it
    // would leak stale status text outside narrow tool regions.
    if !work.bottom_tool_open() {
        let footer = line(layout.footer);
        let command_width = if footer.width >= 48 {
            19
        } else if footer.width >= 24 {
            11
        } else {
            footer.width.min(4)
        };
        let command = Rect::new(
            footer.right().saturating_sub(command_width),
            footer.y,
            command_width,
            1,
        );
        let status = match work.save_state() {
            SaveState::Unchanged => "",
            SaveState::Pending => " · 未保存",
            SaveState::Saving => " · 保存中",
            SaveState::Saved => " · 已保存",
            SaveState::Failed(_) => " · 保存失败",
            SaveState::ReadOnly => " · 只读",
            SaveState::Conflict(_) => " · 冲突",
        };
        let draft = work
            .prompt
            .as_ref()
            .and_then(|p| p.editor.as_ref())
            .is_some_and(|d| d.has_edits());
        let label = if draft {
            "大纲草稿未保存".to_owned()
        } else {
            format!("{} 字{}", work.document.words, status)
        };
        let style_width = if footer.width >= 48 {
            12
        } else if footer.width >= 24 {
            8
        } else {
            0
        };
        let style_area = Rect::new(
            command.x.saturating_sub(style_width + 1),
            footer.y,
            style_width,
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(if notice.is_some_and(|n| n.warning) {
                theme.warning
            } else {
                theme.muted
            }),
            Rect::new(
                footer.x,
                footer.y,
                footer.width.saturating_sub(command_width + style_width + 3),
                1,
            ),
        );
        if style_width > 0 {
            work.render_paragraph_entry(frame, style_area);
        }
        work.render_command_entry(frame, command);
    }
    if let Some(notice) = notice {
        frame.render_widget(
            Paragraph::new(safe_label(&notice.text)).style(if notice.warning {
                theme.warning
            } else {
                theme.muted
            }),
            line(layout.notice),
        );
    }
}
