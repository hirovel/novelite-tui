use crate::{Command, Outcome, Workspace, prompt::Purpose};
use serde::{Deserialize, Serialize};
use std::io;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub pairing: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            pairing: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FormatKind {
    Paragraphs,
    Unindent,
    Punctuation,
    Spacing,
}
impl FormatKind {
    fn label(self) -> &'static str {
        match self {
            Self::Paragraphs => "规范段落",
            Self::Unindent => "去除缩进",
            Self::Punctuation => "修复中文标点",
            Self::Spacing => "中英文间距",
        }
    }
}
#[derive(Clone)]
pub(crate) struct FormatRequest {
    instance: u64,
    revision: u64,
    kind: FormatKind,
}
impl Workspace {
    pub fn pairing_enabled(&self) -> bool {
        self.modules.config.typography.enabled && self.modules.config.typography.pairing
    }
    pub fn set_pairing_enabled(&mut self, enabled: bool) -> io::Result<()> {
        let mut candidate = self.modules.config.clone();
        candidate.typography.pairing = enabled;
        candidate.persist(self.modules.path.as_deref())?;
        self.modules.config = candidate;
        Ok(())
    }
    pub(crate) fn typing_event(
        &mut self,
        event: crossterm::event::Event,
    ) -> io::Result<Option<Outcome>> {
        let assist = self.pairing_enabled() && matches!(&event, crossterm::event::Event::Key(_));
        let Some(action) = crate::input::decode(event) else {
            return Ok(None);
        };
        if assist && matches!(action, crate::Action::Insert(_) | crate::Action::Backspace) {
            self.require_writable()?;
            if self.document.apply_assisted(action) {
                self.map_other_pane();
                self.saving.edited();
            }
            Ok(Some(Outcome::Continue))
        } else {
            self.dispatch(action).map(Some)
        }
    }

    fn begin_format(&mut self, kind: FormatKind) -> io::Result<Outcome> {
        self.require_writable()?;
        if self.prompt.is_some() || self.history_preview.is_some() {
            return Err(io::Error::other("请先关闭当前编辑或预览，再排版正文"));
        }
        let chapter = self
            .active_chapter()
            .ok_or_else(|| io::Error::other("整章排版需要作品文件夹，以保存恢复点"))?;
        let title = format!(
            "「{}」· {} · Enter 确认整章 / Esc 取消",
            chapter.title,
            kind.label()
        );
        let mut prompt = crate::Prompt::new(Purpose::Format(FormatRequest {
            instance: self.document.instance,
            revision: self.document.revision,
            kind,
        }));
        prompt.title = title;
        self.prompt = Some(prompt);
        Ok(Outcome::Continue)
    }
    pub(crate) fn apply_format(&mut self, request: FormatRequest) -> io::Result<()> {
        self.require_writable()?;
        if self.document.instance != request.instance || self.document.revision != request.revision
        {
            return Err(io::Error::other("目标章节或正文已变化，请取消后重新排版"));
        }
        let text = transform(&self.document.text, request.kind);
        if text == self.document.text {
            return Ok(());
        }
        self.snapshot_before_change()?;
        let range = 0..self.document.text.len();
        self.document.replace_ranges(&[range], &text);
        self.map_other_pane();
        self.saving.edited();
        Ok(())
    }
}

fn transform(text: &str, kind: FormatKind) -> String {
    let paragraphs = matches!(kind, FormatKind::Paragraphs | FormatKind::Unindent);
    let mut output = String::with_capacity(text.len());
    let mut pending_blank = None;
    let mut fence = crate::markdown::Fence::default();
    for line in text.split_inclusive('\n') {
        let (body, ending) = line
            .strip_suffix("\r\n")
            .map(|s| (s, "\r\n"))
            .or_else(|| line.strip_suffix('\n').map(|s| (s, "\n")))
            .unwrap_or((line, ""));
        let stripped = body.trim();
        let protected = fence.protects(stripped) || markdown_line(stripped);
        if protected {
            if let Some(blank) = pending_blank.take() {
                output.push_str(blank);
            }
            output.push_str(body);
            output.push_str(ending);
            continue;
        }
        if paragraphs && stripped.is_empty() {
            pending_blank.get_or_insert(ending);
            continue;
        }
        if let Some(blank) = pending_blank.take() {
            output.push_str(blank);
        }
        match kind {
            FormatKind::Paragraphs | FormatKind::Unindent => {
                if matches!(kind, FormatKind::Paragraphs) && !chapter_heading(stripped) {
                    output.push_str("　　");
                }
                output.push_str(stripped);
            }
            FormatKind::Punctuation => output.push_str(&punctuation(body)),
            FormatKind::Spacing => {
                let mut previous = None;
                for ch in body.chars() {
                    if previous.is_some_and(|p: char| {
                        (cjk_spacing(p) && ch.is_ascii_alphanumeric())
                            || (p.is_ascii_alphanumeric() && cjk_spacing(ch))
                    }) {
                        output.push(' ');
                    }
                    output.push(ch);
                    previous = Some(ch);
                }
            }
        }
        output.push_str(ending);
    }
    output
}
fn cjk_spacing(ch: char) -> bool {
    matches!(ch, '\u{4e00}'..='\u{9fa5}' | '\u{3000}'..='\u{303f}')
}
fn chapter_heading(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('第') else {
        return false;
    };
    let rest = rest.trim_start_matches(|c: char| {
        c.is_ascii_digit() || "一二三四五六七八九十百千万".contains(c)
    });
    rest.len() + '第'.len_utf8() < text.len()
        && rest.starts_with(['章', '节', '卷', '篇', '回', '部', '集', '幕'])
}
fn markdown_line(text: &str) -> bool {
    let heading = text.trim_start_matches('#');
    let numbered = text.trim_start_matches(|c: char| c.is_ascii_digit());
    text.starts_with(['>', '|', '<'])
        || text.contains('`')
        // A bracketed label may be a shortcut reference resolved elsewhere in the chapter.
        || (text.contains('[') && text.contains(']'))
        || (heading.len() < text.len() && heading.starts_with(char::is_whitespace))
        || text
            .strip_prefix(['-', '*', '+'])
            .is_some_and(|s| s.starts_with(char::is_whitespace))
        || (numbered.len() < text.len()
            && numbered
                .strip_prefix(['.', ')'])
                .is_some_and(|s| s.starts_with(char::is_whitespace)))
        || (text.len() >= 3 && text.chars().all(|c| matches!(c, '-' | '*' | '_' | ' ')))
}
fn punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut normalized = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let run = if matches!(ch, '-' | '~' | '.' | '。' | '…') {
            chars[i..]
                .iter()
                .take(if ch == '…' { 2 } else { usize::MAX })
                .take_while(|c| **c == ch)
                .count()
        } else {
            1
        };
        if (matches!(ch, '-' | '~') && run >= 2)
            || (matches!(ch, '.' | '。') && run >= 3)
            || ch == '…'
        {
            normalized.extend(if matches!(ch, '-' | '~') {
                ['—', '—']
            } else {
                ['…', '…']
            });
            i += if ch == '…' { run.min(2) } else { run };
        } else {
            normalized.push(ch);
            i += 1;
        }
    }
    let mut output = String::with_capacity(text.len());
    let (mut double_open, mut single_open) = (true, true);
    for (i, &ch) in normalized.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|p| normalized.get(p)).copied();
        let next = normalized.get(i + 1).copied();
        let non_ascii = [prev, next]
            .into_iter()
            .flatten()
            .any(|c| !(' '..='~').contains(&c));
        output.push(match ch {
            ',' if non_ascii => '，',
            '.' if non_ascii => '。',
            ':' if non_ascii => '：',
            ';' if non_ascii => '；',
            '!' => '！',
            '?' => '？',
            '(' => '（',
            ')' => '）',
            '[' => '【',
            ']' => '】',
            '<' => '《',
            '>' => '》',
            '"' => {
                let q = if double_open { '“' } else { '”' };
                double_open = !double_open;
                q
            }
            '\'' if !(prev.is_some_and(|c| c.is_ascii_alphabetic())
                && next.is_some_and(|c| c.is_ascii_alphabetic())) =>
            {
                let q = if single_open { '‘' } else { '’' };
                single_open = !single_open;
                q
            }
            _ => ch,
        });
    }
    output
}
pub(crate) fn commands() -> Vec<Command> {
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    let entries: &[(&str, &str, Run)] = &[
        (
            "toggle-pairing",
            "开关引号/括号配对（默认关闭）",
            |w| {
                w.set_pairing_enabled(!w.modules.config.typography.pairing)?;
                Ok(Outcome::Continue)
            },
        ),
        (
            "format-paragraphs",
            "规范本章段落（缩进/空行）",
            |w| w.begin_format(FormatKind::Paragraphs),
        ),
        ("remove-indents", "清除本章段首缩进", |w| {
            w.begin_format(FormatKind::Unindent)
        }),
        ("clean-punctuation", "修复本章中文标点", |w| {
            w.begin_format(FormatKind::Punctuation)
        }),
        ("pangu-spacing", "规范本章中英文间距", |w| {
            w.begin_format(FormatKind::Spacing)
        }),
    ];
    entries
        .iter()
        .map(|(id, title, run)| Command {
            id: (*id).into(),
            title: (*title).into(),
            run: *run,
        })
        .collect()
}
