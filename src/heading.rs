//! Paragraph meaning belongs to logical lines; widgets only display that meaning.
use crate::{Command, Outcome, Workspace};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};
use std::{io, ops::Range};

#[derive(Clone)]
pub(crate) struct Syntax {
    pub bytes: usize,
    pub prefix: Option<Range<usize>>,
    pub fence_line: Option<String>,
}
impl Syntax {
    pub fn new(line: &str) -> Self {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let hashes = trimmed.bytes().take_while(|c| *c == b'#').count();
        let prefix = if indent <= 3
            && (1..=6).contains(&hashes)
            && (trimmed.len() == hashes || matches!(trimmed.as_bytes()[hashes], b' ' | b'\t'))
        {
            Some(indent..indent + hashes + usize::from(trimmed.len() > hashes))
        } else {
            None
        };
        let fence_line = (line.trim().starts_with("```") || line.trim().starts_with("~~~"))
            .then(|| line.to_owned());
        Self {
            bytes: line.len(),
            prefix,
            fence_line,
        }
    }
}
pub(crate) struct LineInfo {
    pub range: Range<usize>,
    pub prefix: Option<Range<usize>>,
    pub protected: bool,
}
pub(crate) fn matches_title(line: &str, title: &str) -> bool {
    !title.trim().is_empty() && line.split_whitespace().eq(title.split_whitespace())
}
impl Workspace {
    fn is_chapter_title(&self, line: &LineInfo) -> bool {
        let start = line
            .prefix
            .as_ref()
            .map_or(line.range.start, |prefix| prefix.end);
        !line.protected
            && self.active_chapter().is_some_and(|chapter| {
                matches_title(&self.document.text[start..line.range.end], &chapter.title)
            })
    }
    pub(crate) fn heading_style(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.effective_color_mode() == crate::ColorMode::Monochrome {
            style
        } else if self.modules.config.appearance.background == crate::Background::Light {
            style.fg(Color::Blue)
        } else {
            style.fg(Color::LightBlue)
        }
    }
    pub(crate) fn render_paragraph_entry(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if self.prompt.is_some()
            || self.history_preview.is_some()
            || self.navigator.is_some()
            || self.import_active()
        {
            return;
        }
        let line = self.document.current_line();
        if line.protected {
            f.render_widget(Paragraph::new("代码块").style(self.theme().muted), area);
            return;
        }
        let label = if self.is_chapter_title(&line) {
            if area.width >= 10 {
                "章节标题 ▾"
            } else {
                "章名 ▾"
            }
        } else if line.prefix.is_some() {
            "标题 ▾"
        } else {
            "正文 ▾"
        };
        f.render_widget(Paragraph::new(label).style(self.theme().accent), area);
        self.design_ui.heading_hit = Some(area);
    }
    pub(crate) fn open_paragraph_style(&mut self) -> io::Result<Outcome> {
        if self.prompt.is_some()
            || self.history_preview.is_some()
            || self.navigator.is_some()
            || self.import_active()
        {
            return Err(io::Error::other("请先完成当前操作，再设置正文标题"));
        }
        let line = self.document.current_line();
        if self.is_chapter_title(&line) {
            return self.execute_command("rename-chapter");
        }
        self.commands.search_palette("当前段落");
        Ok(Outcome::Continue)
    }
    fn set_paragraph_heading(&mut self, heading: bool) -> io::Result<Outcome> {
        self.require_writable()?;
        if self.prompt.is_some()
            || self.history_preview.is_some()
            || self.navigator.is_some()
            || self.import_active()
        {
            return Err(io::Error::other("请先关闭当前面板，再设置正文标题"));
        }
        let line = self.document.current_line();
        if line.protected {
            return Err(io::Error::other(
                "代码块保留原样；请将光标移到正文再设置标题",
            ));
        }
        let (range, replacement) = if heading {
            if line.prefix.is_some() || self.is_chapter_title(&line) {
                return Ok(Outcome::Continue);
            }
            (line.range.start..line.range.start, "# ")
        } else if let Some(prefix) = line.prefix {
            let mut plain = self.document.text[line.range.clone()].to_owned();
            plain.replace_range(
                prefix.start - line.range.start..prefix.end - line.range.start,
                "",
            );
            if self
                .active_chapter()
                .is_some_and(|chapter| matches_title(&plain, &chapter.title))
            {
                return Err(io::Error::other(
                    "这行与章节名称一致，会作为章节标题；请通过底部章节标题入口改名，或修改这行文字",
                ));
            }
            (prefix, "")
        } else if self.is_chapter_title(&line) {
            return Err(io::Error::other(
                "这行沿用章节名称；请点击底部章节标题改名，或修改这行文字",
            ));
        } else {
            return Ok(Outcome::Continue);
        };
        if self
            .document
            .replace_preserving_selection(range, replacement)
        {
            self.map_other_pane();
            self.saving.edited();
        }
        Ok(Outcome::Continue)
    }
}
pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "paragraph-heading".into(),
            title: "设为标题 · 当前段落".into(),
            run: |w| w.set_paragraph_heading(true),
        },
        Command {
            id: "paragraph-body".into(),
            title: "改回正文 · 当前段落标题".into(),
            run: |w| w.set_paragraph_heading(false),
        },
    ]
}
