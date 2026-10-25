use crate::{ModuleId, Workspace};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

/// Read-only adapter, used by the background word counter.
pub trait WordCountReader: Send + Sync {
    fn read(&self, root: &Path, relative: &str) -> io::Result<String>;
}
struct FileReader;
impl WordCountReader for FileReader {
    fn read(&self, root: &Path, relative: &str) -> io::Result<String> {
        crate::project::managed_path(root, relative).and_then(std::fs::read_to_string)
    }
}
type Revision = (u64, u64);
struct Count {
    words: u64,
    revision: Option<Revision>,
}
struct ResultSet {
    epoch: u64,
    counts: HashMap<String, Count>,
    errors: Vec<String>,
}
struct Scan {
    thread: JoinHandle<()>,
    result: mpsc::Receiver<ResultSet>,
}
pub(crate) struct Statistics {
    identity: Option<(PathBuf, u64)>,
    epoch: u64,
    cancellation: Arc<AtomicU64>,
    generation: u64,
    counts: HashMap<String, Count>,
    errors: Vec<String>,
    requested: bool,
    scan: Option<Scan>,
    reader: Arc<dyn WordCountReader>,
}
impl Default for Statistics {
    fn default() -> Self {
        Self {
            identity: None,
            epoch: 0,
            cancellation: Arc::new(AtomicU64::new(0)),
            generation: 0,
            counts: HashMap::new(),
            errors: vec![],
            requested: true,
            scan: None,
            reader: Arc::new(FileReader),
        }
    }
}
impl Statistics {
    pub fn invalidate(&mut self) {
        self.epoch += 1;
        self.cancellation.store(self.epoch, Ordering::SeqCst);
        self.generation += 1;
        self.counts.clear();
        self.errors.clear();
        self.requested = true;
    }
}
impl Workspace {
    pub fn set_word_count_reader(&mut self, reader: Arc<dyn WordCountReader>) {
        self.statistics.reader = reader;
        self.refresh_word_counts();
    }
    /// Refresh inactive chapters after external edits. Current unsaved text always wins.
    pub fn refresh_word_counts(&mut self) {
        self.statistics.invalidate();
    }
    pub fn word_count_generation(&self) -> u64 {
        self.statistics.generation
    }
    pub(crate) fn poll_word_counts(&mut self) -> io::Result<()> {
        let identity = self.project.as_ref().map(|p| (p.root.clone(), p.revision));
        if self.statistics.identity != identity {
            self.statistics.invalidate();
            self.statistics.identity = identity;
        }
        if let Some(scan) = &self.statistics.scan
            && scan.thread.is_finished()
        {
            let scan = self.statistics.scan.take().unwrap();
            let joined = scan.thread.join();
            if let Ok(result) = scan.result.try_recv() {
                if result.epoch == self.statistics.epoch {
                    self.statistics.counts.extend(result.counts);
                    self.statistics.errors = result.errors;
                    self.statistics.generation += 1;
                }
            } else if joined.is_err() {
                self.statistics.errors = vec!["统计线程异常，请刷新字数".into()];
                self.statistics.generation += 1;
            }
        }
        if !self.module_enabled(ModuleId::Planning) {
            return Ok(());
        }
        let Some(store) = &self.project else {
            return Ok(());
        };
        // Retain the departing chapter's newest count without copying or rescanning text.
        self.statistics.counts.insert(
            store.active.clone(),
            Count {
                words: self.document.words,
                revision: Some((self.document.instance, self.document.revision)),
            },
        );
        if !self.statistics.requested || self.statistics.scan.is_some() {
            return Ok(());
        }
        let inputs: Vec<_> = store
            .manifest
            .volumes
            .iter()
            .flat_map(|v| &v.chapters)
            .filter(|c| c.id != store.active)
            .map(|c| {
                (
                    c.id.clone(),
                    c.path.clone(),
                    self.chapters.get(&c.id).map(|d| (d.instance, d.revision)),
                )
            })
            .collect();
        let root = store.root.clone();
        let epoch = self.statistics.epoch;
        let cancellation = self.statistics.cancellation.clone();
        let reader = self.statistics.reader.clone();
        let (send, result) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("novelite-count".into())
            .spawn(move || {
                let mut result = ResultSet {
                    epoch,
                    counts: HashMap::new(),
                    errors: vec![],
                };
                for (id, path, revision) in inputs {
                    if cancellation.load(Ordering::SeqCst) != epoch {
                        return;
                    }
                    let text = reader.read(&root, &path);
                    if cancellation.load(Ordering::SeqCst) != epoch {
                        return;
                    }
                    match text {
                        Ok(text) => {
                            result.counts.insert(
                                id,
                                Count {
                                    words: crate::word_count::count(text.chars()),
                                    revision,
                                },
                            );
                        }
                        Err(e) => result.errors.push(format!("{path}: {e}")),
                    }
                }
                let _ = send.send(result);
            })?;
        self.statistics.scan = Some(Scan { thread, result });
        self.statistics.requested = false;
        Ok(())
    }
    pub fn word_progress(&self) -> crate::WordProgress {
        let store = self.project.as_ref();
        let current_identity = store.map(|p| (p.root.clone(), p.revision));
        let valid = current_identity == self.statistics.identity;
        let book_words = store.and_then(|p| {
            p.manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .try_fold(0u64, |total, c| {
                    if c.id == p.active {
                        return Some(total + self.document.words);
                    }
                    let count = valid.then(|| self.statistics.counts.get(&c.id)).flatten();
                    let resident = self.chapters.get(&c.id);
                    let words = match (resident, count) {
                        (Some(doc), _) if doc.revision != doc.saved_revision => Some(doc.words),
                        (Some(doc), Some(count))
                            if count.revision != Some((doc.instance, doc.revision)) =>
                        {
                            Some(doc.words)
                        }
                        (_, Some(count)) => Some(count.words),
                        _ => None,
                    };
                    words.map(|n| total + n)
                })
        });
        crate::WordProgress {
            chapter_words: self.document.words,
            chapter_goal: self.chapter_plan().and_then(|p| p.target_words),
            book_words,
            book_goal: store.and_then(|s| s.manifest.target_words),
            running: store.is_some()
                && (!valid || self.statistics.requested || self.statistics.scan.is_some()),
            errors: if valid {
                self.statistics.errors.clone()
            } else {
                vec![]
            },
        }
    }
}

impl Drop for Statistics {
    fn drop(&mut self) {
        self.cancellation.fetch_add(1, Ordering::SeqCst);
    }
}
