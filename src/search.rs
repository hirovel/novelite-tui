use crate::{Command, Outcome, Workspace, prompt::Purpose};
use std::{io, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

pub struct ChapterSearch {
    pub query: String,
    pub chapter: String,
    pub revision: u64,
    pub case_sensitive: bool,
    pub total: usize,
    /// One-based selected result; None means no result is currently selected.
    pub current: Option<usize>,
    pub context: String,
    pub stale: bool,
}

pub(crate) struct Search {
    instance: u64,
    revision: u64,
    query: String,
    matches: Vec<Range<usize>>,
    current: Option<usize>,
    case_sensitive: bool,
    origin: Origin,
}

pub(crate) struct Origin {
    instance: u64,
    navigator: Option<crate::navigator::Navigator>,
    preview: Option<crate::history_ui::Preview>,
    book_focused: bool,
    navigator_token: Option<(crate::ModuleId, u64)>,
    search_token: (crate::ModuleId, u64),
}
impl Origin {
    pub(crate) fn take(work: &mut Workspace) -> Self {
        let book_focused = work.book_search_focused();
        work.set_book_focus(false);
        Self {
            instance: work.document.instance,
            navigator_token: work
                .navigator
                .as_ref()
                .and_then(|n| n.module())
                .map(|id| work.modules.token(id)),
            search_token: work.modules.token(crate::ModuleId::Search),
            navigator: work.navigator.take(),
            preview: work.history_preview.take(),
            book_focused,
        }
    }
    pub(crate) fn restore(self, work: &mut Workspace) {
        if self.instance == work.document.instance {
            work.navigator = self.navigator.filter(|_| {
                self.navigator_token
                    .is_none_or(|token| work.modules.live(token))
            });
            work.history_preview = self.preview;
            work.set_book_focus(self.book_focused && work.modules.live(self.search_token));
        }
    }
}

#[derive(Clone)]
pub(crate) struct Replacement {
    instance: u64,
    revision: u64,
    ranges: Vec<Range<usize>>,
    selection: Option<Range<usize>>,
}

pub(crate) fn matches(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    // Map only whole grapheme boundaries, including lowercase expansions such as İ.
    let mut folded = String::new();
    let mut boundaries = std::collections::HashMap::new();
    for (offset, grapheme) in text.grapheme_indices(true) {
        boundaries.insert(folded.len(), offset);
        if case_sensitive {
            folded.push_str(grapheme);
        } else {
            folded.extend(grapheme.chars().flat_map(char::to_lowercase));
        }
    }
    boundaries.insert(folded.len(), text.len());
    let query = if case_sensitive {
        query.to_owned()
    } else {
        query.chars().flat_map(char::to_lowercase).collect()
    };
    folded
        .match_indices(&query)
        .filter_map(|(start, matched)| {
            Some(*boundaries.get(&start)?..*boundaries.get(&(start + matched.len()))?)
        })
        .collect()
}

pub(crate) fn context(text: &str, range: Range<usize>) -> String {
    let before: String = text[..range.start]
        .graphemes(true)
        .rev()
        .take(18)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let after: String = text[range.end..].graphemes(true).take(18).collect();
    format!("{before}【{}】{after}", &text[range])
}

impl Workspace {
    pub fn chapter_search(&self) -> Option<ChapterSearch> {
        let search = self
            .search
            .as_ref()
            .filter(|s| s.instance == self.document.instance)?;
        let stale = search.revision != self.document.revision;
        let current = search
            .current
            .filter(|i| !stale && self.document.selection() == search.matches[*i]);
        let context = current
            .map(|i| {
                let range = &search.matches[i];
                context(self.text(), range.clone())
            })
            .unwrap_or_else(|| {
                if stale {
                    "正文已变化；再次跳转或替换时重新查询"
                } else {
                    "无匹配选区"
                }
                .into()
            });
        Some(ChapterSearch {
            query: search.query.clone(),
            chapter: self
                .active_chapter()
                .map_or_else(|| self.path.display().to_string(), |c| c.title.clone()),
            revision: self.document.revision,
            case_sensitive: search.case_sensitive,
            total: if stale { 0 } else { search.matches.len() },
            current: current.map(|i| i + 1),
            context,
            stale,
        })
    }

    pub(crate) fn find_chapter(&mut self, query: String) {
        let case_sensitive = self.search.as_ref().is_none_or(|s| s.case_sensitive);
        let previous = self
            .search
            .take()
            .filter(|s| s.instance == self.document.instance);
        let origin = if self.navigator.is_none()
            && self.history_preview.is_none()
            && !self.book_search_focused()
        {
            previous.map(|s| s.origin)
        } else {
            None
        }
        .unwrap_or_else(|| Origin::take(self));
        self.search = Some(Search {
            instance: self.document.instance,
            revision: self.document.revision,
            matches: matches(self.text(), &query, case_sensitive),
            query,
            current: None,
            case_sensitive,
            origin,
        });
        let _ = self.find_next(false);
    }

    pub(crate) fn close_find(&mut self) {
        if let Some(search) = self
            .search
            .take()
            .filter(|s| s.instance == self.document.instance)
        {
            search.origin.restore(self);
        }
    }

    fn refresh_search(&mut self) -> io::Result<&mut Search> {
        let search = self
            .search
            .as_mut()
            .filter(|s| s.instance == self.document.instance)
            .ok_or_else(|| io::Error::other("请先查找当前章节"))?;
        if search.revision != self.document.revision {
            search.matches = matches(&self.document.text, &search.query, search.case_sensitive);
            search.revision = self.document.revision;
            search.current = None;
        }
        Ok(search)
    }

    fn find_next(&mut self, previous: bool) -> io::Result<Outcome> {
        let search = self.refresh_search()?;
        let count = search.matches.len();
        if count > 0 {
            let index = match search.current {
                Some(i) if previous => (i + count - 1) % count,
                Some(i) => (i + 1) % count,
                None if previous => count - 1,
                None => 0,
            };
            search.current = Some(index);
            let range = search.matches[index].clone();
            self.document.select_range(range);
        }
        Ok(Outcome::Continue)
    }

    fn begin_replace(&mut self, all: bool) -> io::Result<Outcome> {
        self.require_writable()?;
        if self.refresh_search()?.current.is_none() {
            self.find_next(false)?;
        }
        let search = self.refresh_search()?;
        let index = search
            .current
            .ok_or_else(|| io::Error::other("当前章节没有匹配"))?;
        let ranges = if all {
            search.matches.clone()
        } else {
            vec![search.matches[index].clone()]
        };
        if !all {
            self.document.select_range(ranges[0].clone());
        }
        let replacement = Replacement {
            instance: self.document.instance,
            revision: self.document.revision,
            selection: (!all).then(|| self.document.selection()),
            ranges,
        };
        self.begin_prompt(Purpose::Replace { replacement, all })
    }

    pub(crate) fn replace_matches(
        &mut self,
        replacement: Replacement,
        text: &str,
    ) -> io::Result<()> {
        self.require_writable()?;
        if replacement.instance != self.document.instance
            || replacement.revision != self.document.revision
            || replacement
                .selection
                .is_some_and(|s| s != self.document.selection())
        {
            return Err(io::Error::other(
                "文稿或选区已变化；Esc 返回后重新选择替换目标",
            ));
        }
        if self.document.replace_ranges(&replacement.ranges, text) {
            self.map_other_pane();
            self.saving.edited();
        }
        if self
            .search
            .as_ref()
            .is_some_and(|s| s.instance == self.document.instance)
        {
            self.refresh_search()?;
        }
        Ok(())
    }
}

pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "replace-current".into(),
            title: "替换当前匹配 · 可撤销".into(),
            run: |w| w.begin_replace(false),
        },
        Command {
            id: "replace-all".into(),
            title: "替换当前章节全部匹配 · 一次撤销".into(),
            run: |w| w.begin_replace(true),
        },
        Command {
            id: "find".into(),
            title: "查找当前章节 · 文字匹配".into(),
            run: |w| w.begin_prompt(Purpose::Find),
        },
        Command {
            id: "find-toggle-case".into(),
            title: "查找：切换区分/忽略大小写".into(),
            run: |w| {
                let search = w.refresh_search()?;
                search.case_sensitive = !search.case_sensitive;
                let query = search.query.clone();
                w.find_chapter(query);
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "find-next".into(),
            title: "下一个章节匹配".into(),
            run: |w| w.find_next(false),
        },
        Command {
            id: "find-previous".into(),
            title: "上一个章节匹配".into(),
            run: |w| w.find_next(true),
        },
        Command {
            id: "close-find".into(),
            title: "关闭章节查找 · 返回正文".into(),
            run: |w| {
                w.close_find();
                Ok(Outcome::Continue)
            },
        },
    ]
}
