use crate::{Chapter, Command, Outcome, Workspace, fingerprint::Fingerprint};
use std::{
    io,
    ops::Range,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Read-only filesystem adapter, called only by the serial search worker.
pub trait SearchReader: Send + Sync {
    fn read(&self, root: &std::path::Path, relative: &str) -> io::Result<String>;
}
struct FileReader;
impl SearchReader for FileReader {
    fn read(&self, root: &std::path::Path, relative: &str) -> io::Result<String> {
        crate::project::managed_path(root, relative).and_then(std::fs::read_to_string)
    }
}

pub struct BookSearch {
    pub generation: u64,
    pub query: String,
    pub running: bool,
    pub focused: bool,
    pub total: usize,
    pub chapters: Vec<BookChapter>,
    pub entries: Vec<BookMatch>,
    pub selected: usize,
    pub errors: Vec<String>,
}
pub struct BookChapter {
    pub chapter: String,
    pub title: String,
    pub count: usize,
}
#[derive(Clone)]
pub struct BookMatch {
    pub chapter: String,
    pub title: String,
    pub is_title: bool,
    pub context: String,
    pub revision: String,
    range: Range<usize>,
    fingerprint: Fingerprint,
}

struct InputChapter {
    chapter: Chapter,
    live: Option<(String, u64)>,
}
struct Job {
    generation: u64,
    root: PathBuf,
    query: String,
    chapters: Vec<InputChapter>,
}

#[derive(Default)]
struct Queue {
    job: Mutex<Option<Job>>,
    result: Mutex<Option<BookSearch>>,
    wake: Condvar,
    generation: AtomicU64,
    closed: AtomicBool,
    active: AtomicBool,
}
pub(crate) struct BookSearching {
    queue: Option<Arc<Queue>>,
    panel: Option<BookSearch>,
    reader: Arc<dyn SearchReader>,
    origin: Option<crate::search::Origin>,
    ui_generation: u64,
}
impl Default for BookSearching {
    fn default() -> Self {
        Self {
            queue: None,
            panel: None,
            reader: Arc::new(FileReader),
            origin: None,
            ui_generation: 0,
        }
    }
}
impl Drop for BookSearching {
    fn drop(&mut self) {
        if let Some(queue) = &self.queue {
            queue.closed.store(true, Ordering::SeqCst);
            queue.generation.fetch_add(1, Ordering::SeqCst);
            queue.job.lock().unwrap().take();
            queue.wake.notify_one();
        }
    }
}
impl Queue {
    fn current(&self, generation: u64) -> bool {
        !self.closed.load(Ordering::SeqCst) && self.generation.load(Ordering::SeqCst) == generation
    }
}
impl BookSearching {
    fn worker(&mut self) -> io::Result<Arc<Queue>> {
        if let Some(queue) = &self.queue {
            return Ok(queue.clone());
        }
        let queue = Arc::new(Queue::default());
        let worker = queue.clone();
        let reader = self.reader.clone();
        std::thread::Builder::new()
            .name("novelite-search".into())
            .spawn(move || {
                loop {
                    let mut slot = worker.job.lock().unwrap();
                    while slot.is_none() && !worker.closed.load(Ordering::SeqCst) {
                        slot = worker.wake.wait(slot).unwrap();
                    }
                    if worker.closed.load(Ordering::SeqCst) {
                        return;
                    }
                    let job = slot.take().unwrap();
                    worker.active.store(true, Ordering::SeqCst);
                    drop(slot);
                    let generation = job.generation;
                    if let Some(result) = scan(job, &worker, reader.as_ref()) {
                        let mut slot = worker.result.lock().unwrap();
                        if worker.current(generation) {
                            *slot = Some(result);
                        }
                    }
                    worker.active.store(false, Ordering::SeqCst);
                }
            })?;
        self.queue = Some(queue.clone());
        Ok(queue)
    }
}

fn scan(job: Job, queue: &Queue, reader: &dyn SearchReader) -> Option<BookSearch> {
    let mut result = BookSearch::pending(job.generation, job.query);
    for input in job.chapters {
        if !queue.current(job.generation) {
            return None;
        }
        let chapter = input.chapter;
        let title_ranges = crate::search::matches(&chapter.title, &result.query, true);
        let text = input.live.map_or_else(
            || reader.read(&job.root, &chapter.path).map(|s| (s, None)),
            |(s, revision)| Ok((s, Some(revision))),
        );
        if !queue.current(job.generation) {
            return None;
        }
        let mut count = title_ranges.len();
        let title_stamp = Fingerprint::bytes(chapter.title.as_bytes());
        for range in title_ranges {
            result.push(
                &chapter,
                &chapter.title,
                range,
                MatchRevision::Title(title_stamp),
            );
        }
        match text {
            Ok((text, revision)) => {
                let stamp = MatchRevision::Body {
                    fingerprint: Fingerprint::bytes(text.as_bytes()),
                    revision,
                };
                let ranges = crate::search::matches(&text, &result.query, true);
                count += ranges.len();
                for range in ranges {
                    if !queue.current(job.generation) {
                        return None;
                    }
                    result.push(&chapter, &text, range, stamp);
                }
            }
            Err(error) => result.errors.push(format!("{}：{error}", chapter.title)),
        }
        result.total += count;
        if count > 0 {
            result.chapters.push(BookChapter {
                chapter: chapter.id,
                title: chapter.title,
                count,
            });
        }
    }
    result.running = false;
    Some(result)
}
#[derive(Clone, Copy)]
enum MatchRevision {
    Title(Fingerprint),
    Body {
        fingerprint: Fingerprint,
        revision: Option<u64>,
    },
}
impl BookSearch {
    fn pending(generation: u64, query: String) -> Self {
        Self {
            generation,
            query,
            running: true,
            focused: true,
            total: 0,
            chapters: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            errors: Vec::new(),
        }
    }
    fn push(&mut self, chapter: &Chapter, text: &str, range: Range<usize>, stamp: MatchRevision) {
        // Keep the panel bounded even when a common character matches an entire book.
        if self.entries.len() >= 1000 {
            return;
        }
        let (is_title, fingerprint, revision) = match stamp {
            MatchRevision::Title(f) => (true, f, None),
            MatchRevision::Body {
                fingerprint,
                revision,
            } => (false, fingerprint, revision),
        };
        self.entries.push(BookMatch {
            chapter: chapter.id.clone(),
            title: chapter.title.clone(),
            is_title,
            context: crate::search::context(text, range.clone()),
            revision: revision.map_or_else(
                || match fingerprint {
                    Fingerprint::Content { checksum, .. } => format!("内容版本 {checksum:016x}"),
                    Fingerprint::Missing => "未落盘".into(),
                },
                |r| format!("文稿修订 {r}"),
            ),
            fingerprint,
            range,
        });
    }
}

impl Workspace {
    pub fn book_search_generation(&self) -> u64 {
        self.book_searching.ui_generation
    }
    pub fn book_search_focused(&self) -> bool {
        self.book_search().is_some_and(|p| p.focused)
            && self.navigator.is_none()
            && self.history_preview.is_none()
    }
    pub(crate) fn set_book_focus(&mut self, focused: bool) {
        if let Some(panel) = &mut self.book_searching.panel {
            panel.focused = focused;
        }
    }
    pub(crate) fn toggle_book_focus(&mut self) -> io::Result<Outcome> {
        if self.book_search().is_none() {
            return Err(io::Error::other("请先全书搜索"));
        }
        let focus = !self.book_search_focused();
        if focus && (self.navigator.is_some() || self.history_preview.is_some()) {
            self.book_searching.origin = Some(crate::search::Origin::take(self));
        }
        self.set_book_focus(focus);
        Ok(Outcome::Continue)
    }
    pub fn set_search_reader(&mut self, reader: impl SearchReader + 'static) {
        let mut next = BookSearching::default();
        next.reader = Arc::new(reader);
        self.book_searching = next;
    }
    pub fn search_background_busy(&self) -> bool {
        self.book_searching
            .queue
            .as_ref()
            .is_some_and(|q| q.active.load(Ordering::SeqCst) || q.job.lock().unwrap().is_some())
    }
    pub fn book_search(&self) -> Option<&BookSearch> {
        self.book_searching.panel.as_ref()
    }
    pub(crate) fn select_book_result(&mut self, index: usize) {
        if let Some(panel) = &mut self.book_searching.panel {
            panel.selected = index.min(panel.entries.len().saturating_sub(1));
        }
    }

    pub fn start_book_search(&mut self, query: &str) -> io::Result<()> {
        if !self.module_enabled(crate::ModuleId::Search) {
            return Err(io::Error::other("搜索模块已关闭"));
        }
        if query.is_empty() {
            return Err(io::Error::other("查询不能为空"));
        }
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("全书搜索需要打开作品"))?;
        let chapters = store
            .manifest
            .volumes
            .iter()
            .flat_map(|v| &v.chapters)
            .map(|chapter| {
                let document = if chapter.id == store.active {
                    Some(&self.document)
                } else {
                    self.chapters
                        .get(&chapter.id)
                        .filter(|d| d.revision != d.saved_revision)
                };
                InputChapter {
                    chapter: chapter.clone(),
                    live: document.map(|d| (d.text.clone(), d.revision)),
                }
            })
            .collect();
        let root = store.root.clone();
        let queue = self.book_searching.worker()?;
        if self.book_searching.origin.is_none()
            || self.navigator.is_some()
            || self.history_preview.is_some()
        {
            self.book_searching.origin = Some(crate::search::Origin::take(self));
        }
        let generation = queue.generation.fetch_add(1, Ordering::SeqCst) + 1;
        queue.result.lock().unwrap().take();
        *queue.job.lock().unwrap() = Some(Job {
            generation,
            root,
            query: query.into(),
            chapters,
        });
        self.book_searching.panel = Some(BookSearch::pending(generation, query.into()));
        queue.wake.notify_one();
        Ok(())
    }

    pub fn poll_book_search(&mut self) {
        if let Some(queue) = &self.book_searching.queue
            && let Some(mut result) = queue.result.lock().unwrap().take()
            && let Some(panel) = &mut self.book_searching.panel
            && result.generation == panel.generation
        {
            result.focused = panel.focused;
            *panel = result;
            self.book_searching.ui_generation += 1;
        }
    }

    pub fn open_book_match(&mut self, index: usize) -> io::Result<()> {
        let panel = self
            .book_search()
            .ok_or_else(|| io::Error::other("请先全书搜索"))?;
        let entry = panel
            .entries
            .get(index)
            .cloned()
            .ok_or_else(|| io::Error::other("搜索结果不存在"))?;
        let query = panel.query.clone();
        self.switch_chapter(&entry.chapter)?;
        let text = if entry.is_title {
            &self.active_chapter().unwrap().title
        } else {
            self.text()
        };
        if Fingerprint::bytes(text.as_bytes()) != entry.fingerprint {
            self.start_book_search(&query)?;
            return Err(io::Error::other("搜索结果已过期，已重新查询；请重新选择"));
        }
        self.document
            .select_range(if entry.is_title { 0..0 } else { entry.range });
        self.navigator = None;
        self.history_preview = None;
        if let Some(panel) = &mut self.book_searching.panel {
            panel.focused = false;
        }
        self.book_searching.origin = None;
        Ok(())
    }

    pub(crate) fn close_book_search(&mut self) {
        if let Some(queue) = &self.book_searching.queue {
            queue.generation.fetch_add(1, Ordering::SeqCst);
            queue.job.lock().unwrap().take();
            queue.result.lock().unwrap().take();
        }
        self.book_searching.panel = None;
        if let Some(origin) = self.book_searching.origin.take() {
            origin.restore(self);
        }
    }

    pub(crate) fn book_search_event(
        &mut self,
        event: crossterm::event::Event,
    ) -> io::Result<Option<Outcome>> {
        use crossterm::event::{Event, KeyCode, KeyEventKind};
        if let Event::Key(key) = event
            && key.kind != KeyEventKind::Release
        {
            match key.code {
                KeyCode::Esc => self.close_book_search(),
                KeyCode::Enter => self.open_book_match(self.book_search().unwrap().selected)?,
                KeyCode::Up | KeyCode::Down => {
                    let panel = self.book_searching.panel.as_mut().unwrap();
                    panel.selected = if key.code == KeyCode::Up {
                        panel.selected.saturating_sub(1)
                    } else {
                        (panel.selected + 1).min(panel.entries.len().saturating_sub(1))
                    };
                }
                _ => {}
            }
        }
        Ok(Some(Outcome::Continue))
    }
}

pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "search-book".into(),
            title: "全书搜索 · 章节标题和正文".into(),
            run: |w| w.begin_prompt(crate::prompt::Purpose::SearchBook),
        },
        Command {
            id: "focus-book-search".into(),
            title: "切换全书结果/正文焦点".into(),
            run: Workspace::toggle_book_focus,
        },
        Command {
            id: "close-book-search".into(),
            title: "关闭全书搜索并取消任务".into(),
            run: |w| {
                w.close_book_search();
                Ok(Outcome::Continue)
            },
        },
    ]
}
