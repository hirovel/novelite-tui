use crate::project::ProjectStore;
use crate::storage::{WriteMode, atomic_write};
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

pub(crate) enum Target {
    File { path: PathBuf, new: bool },
    Project(Box<ProjectStore>),
}
struct Job {
    handle: JoinHandle<io::Result<()>>,
    revision: u64,
    metadata: u64,
    disk: crate::fingerprint::Fingerprint,
    manifest: Option<crate::fingerprint::Fingerprint>,
}
pub(crate) struct Saved {
    pub revision: u64,
    pub metadata: u64,
    pub disk: crate::fingerprint::Fingerprint,
    pub manifest: Option<crate::fingerprint::Fingerprint>,
}
#[derive(Default)]
pub(crate) struct Saving {
    job: Option<Job>,
    first_dirty: Option<Instant>,
    last_edit: Option<Instant>,
    pub error: Option<String>,
    pub completed: bool,
    pub uncertain: bool,
}
impl Saving {
    pub fn edited(&mut self) {
        let now = Instant::now();
        self.first_dirty.get_or_insert(now);
        self.last_edit = Some(now);
        self.error = None;
    }
    pub fn running(&self) -> bool {
        self.job.is_some()
    }
    pub fn due(&self, now: Instant) -> bool {
        !self.running()
            && self.error.is_none()
            && (self
                .last_edit
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(300))
                || self
                    .first_dirty
                    .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(2)))
    }
    pub fn request(
        &mut self,
        target: Target,
        text: String,
        revision: u64,
        metadata: u64,
        expected_body: crate::fingerprint::Fingerprint,
        expected_manifest: Option<crate::fingerprint::Fingerprint>,
    ) -> io::Result<()> {
        if self.running() {
            return Ok(());
        }
        let disk = crate::fingerprint::Fingerprint::bytes(text.as_bytes());
        let manifest = match &target {
            Target::Project(store) => Some(crate::fingerprint::Fingerprint::bytes(
                &serde_json::to_vec_pretty(&store.manifest)?,
            )),
            Target::File { .. } => None,
        };
        let handle = std::thread::Builder::new()
            .name("novelite-save".into())
            .spawn(move || match target {
                Target::Project(mut store) => {
                    store.commit_guarded(&text, expected_body, expected_manifest)
                }
                Target::File { path, new } => {
                    crate::fingerprint::ensure(&path, expected_body, disk)?;
                    atomic_write(
                        &path,
                        text.as_bytes(),
                        if new {
                            WriteMode::CreateNew
                        } else {
                            WriteMode::Replace
                        },
                    )
                }
            })?;
        self.job = Some(Job {
            handle,
            revision,
            metadata,
            disk,
            manifest,
        });
        self.uncertain = true;
        self.first_dirty = None;
        self.error = None;
        Ok(())
    }
    pub fn complete(&mut self, wait: bool) -> io::Result<Option<Saved>> {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| wait || job.handle.is_finished())
        {
            return Ok(None);
        }
        let job = self.job.take().unwrap();
        let result = job
            .handle
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("保存线程异常；正文仍在会话中")));
        match result {
            Ok(()) => {
                self.completed = true;
                self.uncertain = false;
                Ok(Some(Saved {
                    revision: job.revision,
                    metadata: job.metadata,
                    disk: job.disk,
                    manifest: job.manifest,
                }))
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }
}
impl Drop for Saving {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.handle.join();
        }
    }
}
