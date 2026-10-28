use crate::{
    Workspace,
    fingerprint::Fingerprint,
    project::ProjectStore,
    storage::{WriteMode, atomic_write},
};
use std::{
    io,
    path::PathBuf,
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Unchanged,
    Pending,
    Saving,
    Saved,
    Failed(String),
    ReadOnly,
    Conflict(String),
}

// These values are captured together, before the worker can observe any further edits.
struct SaveSnapshot {
    target: Target,
    text: String,
    expected_body: Fingerprint,
    expected_manifest: Option<Fingerprint>,
    receipt: SaveReceipt,
}

enum Target {
    File { path: PathBuf, new: bool },
    Project(Box<ProjectStore>),
}

struct SaveReceipt {
    document: u64,
    path: PathBuf,
    // Undo can restore an earlier identity; this is not an increasing save sequence.
    revision: u64,
    disk: Fingerprint,
    project: Option<ProjectReceipt>,
}

struct ProjectReceipt {
    id: String,
    chapter: String,
    revision: u64,
    manifest: Fingerprint,
}

impl SaveSnapshot {
    fn capture(work: &Workspace) -> io::Result<Self> {
        let text = work.document.text.clone();
        let (target, project, path) = if let Some(store) = &work.project {
            let manifest = serde_json::to_vec_pretty(&store.manifest)?;
            (
                Target::Project(Box::new(store.clone())),
                Some(ProjectReceipt {
                    id: store.manifest.id.clone(),
                    chapter: store.active.clone(),
                    revision: store.revision,
                    manifest: Fingerprint::bytes(&manifest),
                }),
                store.path(),
            )
        } else {
            (
                Target::File {
                    path: work.path.clone(),
                    new: work.document.new_file,
                },
                None,
                work.path.clone(),
            )
        };
        if path != work.path {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "保存目标与当前文稿不一致；正文仍在会话中",
            ));
        }
        let receipt = SaveReceipt {
            document: work.document.instance,
            path,
            revision: work.document.revision,
            disk: Fingerprint::bytes(text.as_bytes()),
            project,
        };
        Ok(Self {
            target,
            text,
            expected_body: work.document.disk,
            expected_manifest: work.external.manifest,
            receipt,
        })
    }

    fn write(self) -> io::Result<SaveReceipt> {
        match self.target {
            Target::Project(mut store) => {
                store.commit_guarded(&self.text, self.expected_body, self.expected_manifest)?;
            }
            Target::File { path, new } => {
                crate::fingerprint::ensure(&path, self.expected_body, self.receipt.disk)?;
                atomic_write(
                    &path,
                    self.text.as_bytes(),
                    if new {
                        WriteMode::CreateNew
                    } else {
                        WriteMode::Replace
                    },
                )?;
            }
        }
        Ok(self.receipt)
    }
}

impl SaveReceipt {
    fn confirm(self, work: &mut Workspace) -> io::Result<()> {
        let same_project = match (&self.project, &work.project) {
            (None, None) => true,
            (Some(saved), Some(store)) => {
                saved.id == store.manifest.id && saved.chapter == store.active
            }
            _ => false,
        };
        if self.document != work.document.instance || self.path != work.path || !same_project {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "保存完成目标已改变；当前文稿未确认，请重新保存",
            ));
        }
        // Even when newer input exists, this snapshot is the new disk baseline.
        work.document.disk = self.disk;
        work.document.saved_revision = self.revision;
        work.document.new_file = false;
        work.external.manifest = self.project.as_ref().map(|saved| saved.manifest);
        if let (Some(saved), Some(store)) = (self.project, &mut work.project) {
            store.dirty = store.revision != saved.revision;
            store.pending.clear();
        }
        work.saving.completed = true;
        work.saving.uncertain = false;
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Saving {
    job: Option<JoinHandle<io::Result<SaveReceipt>>>,
    first_dirty: Option<Instant>,
    last_edit: Option<Instant>,
    pub(super) error: Option<String>,
    pub(super) completed: bool,
    pub(super) uncertain: bool,
}
impl Saving {
    pub(super) fn edited(&mut self) {
        let now = Instant::now();
        self.first_dirty.get_or_insert(now);
        self.last_edit = Some(now);
        self.error = None;
    }
    pub(super) fn running(&self) -> bool {
        self.job.is_some()
    }
    pub(super) fn due(&self, now: Instant) -> bool {
        !self.running()
            && self.error.is_none()
            && (self
                .last_edit
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(300))
                || self
                    .first_dirty
                    .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(2)))
    }
    fn request(&mut self, snapshot: SaveSnapshot) -> io::Result<()> {
        if self.running() {
            return Ok(());
        }
        self.job = Some(
            std::thread::Builder::new()
                .name("novelite-save".into())
                .spawn(move || snapshot.write())?,
        );
        self.uncertain = true;
        self.first_dirty = None;
        self.error = None;
        Ok(())
    }
    fn complete(&mut self, wait: bool) -> io::Result<Option<SaveReceipt>> {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| wait || job.is_finished())
        {
            return Ok(None);
        }
        self.job
            .take()
            .unwrap()
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("保存线程异常；正文仍在会话中")))
            .map(Some)
    }
}
impl Drop for Saving {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.join();
        }
    }
}

impl Workspace {
    pub(super) fn save(&mut self) -> io::Result<()> {
        self.finish_save(true)?;
        if !self.saving.uncertain {
            self.check_external()?;
        }
        self.ensure_no_conflict()?;
        if self.is_dirty() {
            self.request_save()?;
            self.finish_save(true)?;
        }
        self.remember_location()
    }

    pub fn request_save(&mut self) -> io::Result<()> {
        if !self.is_dirty() || self.saving.running() {
            return Ok(());
        }
        self.require_writable()?;
        self.ensure_no_conflict()?;
        let snapshot = SaveSnapshot::capture(self)?;
        self.saving.request(snapshot)
    }

    pub(super) fn finish_save(&mut self, wait: bool) -> io::Result<()> {
        let result = self.saving.complete(wait).and_then(|receipt| {
            if let Some(receipt) = receipt {
                receipt.confirm(self)?;
            }
            Ok(())
        });
        result.inspect_err(|error| {
            self.saving.error = Some(error.to_string());
            if error.kind() == io::ErrorKind::AlreadyExists {
                self.saved_conflict();
            }
        })
    }

    pub fn poll_save(&mut self) -> io::Result<()> {
        self.finish_save(false)
    }

    pub fn save_state(&self) -> SaveState {
        if let Some(conflict) = self.external_conflict() {
            SaveState::Conflict(conflict.message.clone())
        } else if self.is_read_only() {
            SaveState::ReadOnly
        } else if let Some(error) = &self.saving.error {
            SaveState::Failed(error.clone())
        } else if self.saving.running() {
            SaveState::Saving
        } else if self.is_dirty() {
            SaveState::Pending
        } else if self.saving.completed {
            SaveState::Saved
        } else {
            SaveState::Unchanged
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.saving.uncertain
            || self.document.new_file
            || self.document.revision != self.document.saved_revision
            || self.project.as_ref().is_some_and(|store| store.dirty)
    }
}
