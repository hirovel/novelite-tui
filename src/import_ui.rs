use crate::{Command, ImportEncoding, Outcome, Workspace, prompt::Purpose};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{io, path::PathBuf};

#[derive(Default)]
pub(crate) struct ImportUi {
    pub origin: Option<crate::search::Origin>,
    selected: usize,
    first: usize,
    list: Rect,
}
impl Workspace {
    pub(crate) fn cancel_import(&mut self) {
        if self.import_publishing() {
            return;
        }
        if let Some(crate::importing::ImportJob::Preview { cancelled, .. }) = &mut self.import_job {
            *cancelled = true;
        }
        self.import_revision = self.import_revision.wrapping_add(1);
        self.importing = None;
        if let Some(origin) = self.import_ui.origin.take() {
            origin.restore(self);
        }
        self.import_ui = ImportUi::default();
    }
    pub(crate) fn begin_import(&mut self) -> io::Result<Outcome> {
        self.require_export_idle()?;
        self.require_import_idle()?;
        if self.prompt.is_some() {
            return Err(io::Error::other("请先完成或取消当前输入"));
        }
        self.cancel_import();
        self.import_ui.origin = Some(crate::search::Origin::take(self));
        self.begin_prompt(Purpose::ImportSource)
    }
    pub(crate) fn import_source_response(
        &mut self,
        source: PathBuf,
        encoding: ImportEncoding,
    ) -> io::Result<Option<Outcome>> {
        self.prepare_import(&source, encoding)?;
        self.prompt = None;
        self.import_ui.selected = 0;
        Ok(Some(Outcome::Continue))
    }
    pub(crate) fn import_event(&mut self, event: Event) -> io::Result<Option<Outcome>> {
        if self.import_busy() {
            if matches!(event, Event::Key(k) if k.code == KeyCode::Esc && k.kind != KeyEventKind::Release)
            {
                self.cancel_import();
            }
            return Ok(Some(Outcome::Continue));
        }
        let count = self.import_preview().unwrap().chapters.len();
        let mut enter = false;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Up => self.import_ui.selected = self.import_ui.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.import_ui.selected = (self.import_ui.selected + 1).min(count - 1)
                }
                KeyCode::PageUp => {
                    self.import_ui.selected = self.import_ui.selected.saturating_sub(10)
                }
                KeyCode::PageDown => {
                    self.import_ui.selected = (self.import_ui.selected + 10).min(count - 1)
                }
                KeyCode::Enter => enter = true,
                KeyCode::Esc => self.cancel_import(),
                _ => {}
            },
            Event::Mouse(mouse) if self.mouse_enabled() => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.import_ui.selected = self.import_ui.selected.saturating_sub(1)
                }
                MouseEventKind::ScrollDown => {
                    self.import_ui.selected = (self.import_ui.selected + 1).min(count - 1)
                }
                MouseEventKind::Down(MouseButton::Left)
                    if self
                        .import_ui
                        .list
                        .contains((mouse.column, mouse.row).into()) =>
                {
                    self.import_ui.selected = (self.import_ui.first
                        + (mouse.row - self.import_ui.list.y) as usize)
                        .min(count - 1);
                }
                _ => {}
            },
            _ => {}
        }
        if enter {
            self.begin_prompt(Purpose::ImportDestination)?;
        }
        Ok(Some(Outcome::Continue))
    }
    pub(crate) fn guard_import_command(&self, id: &str) -> io::Result<()> {
        if self.import_publishing()
            || (self.import_busy() && self.import_active() && !matches!(id, "save" | "quit"))
            || (self.importing.is_some()
                && !matches!(
                    id,
                    "save"
                        | "quit"
                        | "import-encoding"
                        | "import-manuscript"
                        | "toggle-import-module"
                ))
        {
            return Err(io::Error::other("请先完成或取消导入；预览不会修改当前正文"));
        }
        Ok(())
    }
    pub(crate) fn import_captures(&self, event: &Event) -> bool {
        self.import_active()
            && self.commands.palette_selection().is_none()
            && self.prompt.is_none()
            && !matches!(event, Event::Key(k) if k.code == KeyCode::F(1) || (k.code == KeyCode::Char('p') && k.modifiers == KeyModifiers::CONTROL))
            && self.commands.global_command(event).is_none()
            && self.commands.bound_command(event).is_none()
    }
    pub(crate) fn render_import(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        if self.import_busy() && self.import_active() {
            let label = if self.import_publishing() {
                "正在创建新作品，请等待完成后退出"
            } else {
                "正在扫描导入来源 · Esc 取消"
            };
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(label)
                    .block(Block::default().borders(Borders::ALL).title("导入处理中")),
                area,
            );
            return;
        }
        let Some(plan) = &self.importing else {
            return;
        };
        let preview = &plan.preview;
        let block = Block::default()
            .style(self.theme().base)
            .borders(Borders::ALL)
            .title("导入预览 · ↑↓ 浏览 · Enter 新目录 · Esc 取消");
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let selected = self.import_ui.selected.min(preview.chapters.len() - 1);
        let capacity = inner.height.saturating_sub(4).div_ceil(2).max(1) as usize;
        let first = selected.saturating_sub(capacity - 1);
        self.import_ui.first = first;
        self.import_ui.list = Rect::new(
            inner.x,
            inner.y.saturating_add(2),
            inner.width,
            capacity.min(inner.height.saturating_sub(2) as usize) as u16,
        );
        let mut lines = vec![
            Line::from(crate::render::safe_label(&format!(
                "{} · {} · {}章 · 保留{}份原件",
                preview.title,
                preview.encoding.label(),
                preview.chapters.len(),
                plan.sources.len()
            ))),
            Line::from("仅迁移正文；附带文件归档，不接入旧大纲/历史"),
        ];
        for (i, chapter) in preview
            .chapters
            .iter()
            .enumerate()
            .skip(first)
            .take(capacity)
        {
            let line = Line::from(crate::render::safe_label(&format!(
                "{} / {}",
                chapter.volume, chapter.title
            )));
            lines.push(if i == selected {
                line.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                line
            });
        }
        lines.push(Line::from("── 所选章节原文片段 ──"));
        lines.extend(
            preview.chapters[selected]
                .text
                .lines()
                .take(inner.height as usize)
                .map(|s| {
                    Line::from(crate::render::safe_label(
                        &s.chars().take(inner.width as usize).collect::<String>(),
                    ))
                }),
        );
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
pub(crate) fn encoding(value: &str) -> io::Result<ImportEncoding> {
    match value.trim().to_ascii_lowercase().replace('-', "").as_str() {
        "utf8" => Ok(ImportEncoding::Utf8),
        "utf16le" => Ok(ImportEncoding::Utf16Le),
        "utf16be" => Ok(ImportEncoding::Utf16Be),
        "gb18030" | "gbk" => Ok(ImportEncoding::Gb18030),
        _ => Err(io::Error::other(
            "支持 utf8、utf16le、utf16be、gb18030；请选择来源编码",
        )),
    }
}
pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "import-manuscript".into(),
            title: "导入 TXT/Markdown 文件或目录（预览后新建作品）".into(),
            run: Workspace::begin_import,
        },
        Command {
            id: "import-encoding".into(),
            title: "重新选择导入编码并预览".into(),
            run: |w| {
                if w.prompt.is_some() {
                    return Err(io::Error::other("请先完成或取消当前输入"));
                }
                let source = w
                    .import_preview()
                    .ok_or_else(|| io::Error::other("请先选择导入来源"))?
                    .source
                    .clone();
                w.begin_prompt(Purpose::ImportEncoding(source))
            },
        },
    ]
}
