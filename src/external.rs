use crate::{
    Workspace,
    fingerprint::Fingerprint,
    storage::{WriteMode, atomic_write},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ExternalConflict {
    pub message: String,
    pub disk_text: Option<String>,
}
#[derive(Clone, PartialEq, Eq)]
struct Disk {
    body: Option<Vec<u8>>,
    manifest: Option<Vec<u8>>,
}
struct Loaded {
    manifest: Option<crate::Project>,
    active: Option<String>,
    path: PathBuf,
    text: String,
}
fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn stamp(bytes: &Option<Vec<u8>>) -> Fingerprint {
    bytes
        .as_ref()
        .map_or(Fingerprint::Missing, |b| Fingerprint::bytes(b))
}
#[derive(Default)]
pub(crate) struct External {
    pub manifest: Option<Fingerprint>,
    conflict: Option<ExternalConflict>,
    observed: Option<Disk>,
    backup: Option<PathBuf>,
    scan: Option<Scan>,
    last_scan: Option<Instant>,
    generation: u64,
}
struct Scan {
    job: JoinHandle<io::Result<Disk>>,
    path: PathBuf,
    body: Fingerprint,
    manifest: Option<Fingerprint>,
}
impl Drop for External {
    fn drop(&mut self) {
        if let Some(scan) = self.scan.take() {
            let _ = scan.job.join();
        }
    }
}
fn read_disk(path: &Path, root: Option<&Path>) -> io::Result<Disk> {
    if let Some(root) = root {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        crate::project::managed_path(root, &relative.to_string_lossy())?;
        crate::project::managed_path(root, "novelite.json")?;
    }
    Ok(Disk {
        body: read_optional(path)?,
        manifest: root
            .map(|r| read_optional(&r.join("novelite.json")))
            .transpose()?
            .flatten(),
    })
}
impl Workspace {
    pub(crate) fn observe_manifest(&mut self) -> io::Result<()> {
        self.external.manifest = self
            .project
            .as_ref()
            .map(|s| Fingerprint::read(&s.root.join("novelite.json")))
            .transpose()?;
        Ok(())
    }
    fn read_external(&self) -> io::Result<Disk> {
        read_disk(&self.path, self.project.as_ref().map(|s| s.root.as_path()))
    }
    pub fn external_conflict(&self) -> Option<&ExternalConflict> {
        self.external.conflict.as_ref()
    }
    pub fn last_conflict_backup(&self) -> Option<&Path> {
        self.external.backup.as_deref()
    }
    fn record_conflict(&mut self, disk: Disk, message: &str) {
        self.external.generation += 1;
        self.external.conflict = Some(ExternalConflict {
            message: message.into(),
            disk_text: disk
                .body
                .as_ref()
                .and_then(|v| String::from_utf8(v.clone()).ok()),
        });
        self.external.observed = Some(disk);
    }
    pub fn check_external(&mut self) -> io::Result<()> {
        self.finish_save(true)?;
        if self.is_read_only()
            && self
                .project
                .as_ref()
                .is_some_and(|s| s.root.join(".novelite-journal.json").exists())
        {
            return Ok(());
        }
        let disk = self.read_external()?;
        self.inspect_external(disk)
    }
    fn inspect_external(&mut self, disk: Disk) -> io::Result<()> {
        if self.external.conflict.is_some() && self.saving.uncertain {
            return Ok(());
        }
        if self.saving.uncertain
            && let Some(store) = &self.project
            && store.pending_matches_disk()?
        {
            return Ok(());
        }
        if stamp(&disk.body) == self.document.disk
            && self.external.manifest == self.project.as_ref().map(|_| stamp(&disk.manifest))
        {
            self.external.conflict = None;
            self.external.observed = None;
            return Ok(());
        }
        if self.external.conflict.is_some() {
            return Ok(());
        }
        if self.is_dirty() {
            self.record_conflict(
                disk,
                "磁盘与本地同时改变；请选择载入磁盘、另存本地副本或明确覆盖",
            );
            return Ok(());
        }
        if disk
            .body
            .as_ref()
            .is_none_or(|v| std::str::from_utf8(v).is_err())
        {
            self.record_conflict(disk, "磁盘正文已删除或不是 UTF-8；当前文稿保留");
            return Ok(());
        }
        if let Err(error) = self.loadable_disk(&disk, false) {
            self.record_conflict(
                disk,
                &format!("磁盘目录不可直接载入：{error}；当前作品保留，可另存本地副本"),
            );
            return Ok(());
        }
        self.adopt_external(disk, false)
    }
    pub fn external_generation(&self) -> u64 {
        self.external.generation
    }
    pub(crate) fn poll_external(&mut self, now: Instant) -> io::Result<()> {
        if self.saving.running() {
            return Ok(());
        }
        if self
            .project
            .as_ref()
            .is_some_and(|s| s.root.join(".novelite-journal.json").exists())
        {
            return Ok(());
        }
        if self
            .external
            .scan
            .as_ref()
            .is_some_and(|s| s.job.is_finished())
        {
            let scan = self.external.scan.take().unwrap();
            let disk = scan
                .job
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("外部检查线程异常")))?;
            if scan.path == self.path
                && scan.body == self.document.disk
                && scan.manifest == self.external.manifest
            {
                self.inspect_external(disk)?;
            }
        }
        if self.external.scan.is_none()
            && self.external.last_scan.is_none_or(|last| {
                now.saturating_duration_since(last) >= Duration::from_millis(500)
            })
        {
            let path = self.path.clone();
            let root = self.project.as_ref().map(|s| s.root.clone());
            self.external.scan = Some(Scan {
                path: path.clone(),
                body: self.document.disk,
                manifest: self.external.manifest,
                job: std::thread::Builder::new()
                    .name("novelite-external".into())
                    .spawn(move || read_disk(&path, root.as_deref()))?,
            });
            self.external.last_scan = Some(now);
        }
        Ok(())
    }
    pub(crate) fn saved_conflict(&mut self) {
        if let Ok(disk) = self.read_external() {
            self.record_conflict(disk, "保存前检测到磁盘变化；停止覆盖，请处理外部冲突");
        }
    }
    fn validated_manifest(&self, disk: &Disk) -> io::Result<Option<crate::Project>> {
        let Some(store) = &self.project else {
            return Ok(None);
        };
        let bytes = disk
            .manifest
            .as_ref()
            .ok_or_else(|| io::Error::other("作品清单已删除"))?;
        let manifest: crate::Project = serde_json::from_slice(bytes)?;
        crate::project::validate(&store.root, &manifest)?;
        if manifest.id != store.manifest.id {
            return Err(io::Error::other("磁盘作品身份已改变"));
        }
        Ok(Some(manifest))
    }
    fn loadable_disk(&self, disk: &Disk, allow_removed: bool) -> io::Result<Loaded> {
        let manifest = self.validated_manifest(disk)?;
        let mut active = self.project.as_ref().map(|s| s.active.clone());
        let mut path = self.path.clone();
        let bytes = if let (Some(store), Some(manifest)) = (&self.project, &manifest)
            && !manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .any(|c| c.id == store.active)
        {
            if !allow_removed {
                return Err(io::Error::other(
                    "当前章节已从磁盘目录移除；载入磁盘会跳转到现存章节",
                ));
            }
            let next = manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .next()
                .unwrap();
            active = Some(next.id.clone());
            path = crate::project::managed_path(&store.root, &next.path)?;
            fs::read(&path)?
        } else {
            disk.body
                .clone()
                .ok_or_else(|| io::Error::other("磁盘正文已删除；可另存本地副本或明确覆盖恢复"))?
        };
        let text = String::from_utf8(bytes).map_err(io::Error::other)?;
        Ok(Loaded {
            manifest,
            active,
            path,
            text,
        })
    }
    fn backup_conflict(&mut self, disk: &Disk) -> io::Result<()> {
        self.require_writable()?;
        let id = crate::project::identity()?;
        let dir = if let Some(store) = &self.project {
            crate::project::managed_path(&store.root, &format!("conflicts/{id}"))?
        } else {
            self.path
                .parent()
                .unwrap()
                .join(format!("novelite-conflict-{id}"))
        };
        fs::create_dir_all(&dir)?;
        atomic_write(
            &dir.join("local.md"),
            self.text().as_bytes(),
            WriteMode::CreateNew,
        )?;
        if let Some(bytes) = &disk.body {
            atomic_write(&dir.join("disk.bin"), bytes, WriteMode::CreateNew)?;
        }
        if let Some(store) = &self.project {
            atomic_write(
                &dir.join("local-manifest.json"),
                &serde_json::to_vec_pretty(&store.manifest)?,
                WriteMode::CreateNew,
            )?;
            if let Some(bytes) = &disk.manifest {
                atomic_write(&dir.join("disk-manifest.bin"), bytes, WriteMode::CreateNew)?;
            }
            for (id, document) in &self.chapters {
                atomic_write(
                    &dir.join(format!("cached-{id}.md")),
                    document.text.as_bytes(),
                    WriteMode::CreateNew,
                )?;
            }
        }
        self.external.backup = Some(dir);
        Ok(())
    }
    fn adopt_external(&mut self, disk: Disk, allow_removed: bool) -> io::Result<()> {
        let loaded = self.loadable_disk(&disk, allow_removed)?;
        if self.project.is_some() && !self.is_read_only() {
            self.snapshot_before_change()?;
        }
        if (self.project.is_none()
            || self.external.manifest != self.project.as_ref().map(|_| stamp(&disk.manifest)))
            && !self.is_read_only()
        {
            self.backup_conflict(&disk)?;
        }
        self.document.refresh(loaded.text);
        self.path = loaded.path;
        if let (Some(store), Some(manifest)) = (&mut self.project, loaded.manifest) {
            store.active = loaded.active.unwrap();
            store.manifest = manifest;
            store.changed();
            store.dirty = false;
            store.pending.clear();
        }
        self.saving = crate::saving::Saving::default();
        self.reconcile_panes();
        self.external.manifest = self.project.as_ref().map(|_| stamp(&disk.manifest));
        self.clear_history_view();
        self.external.conflict = None;
        self.external.observed = None;
        self.external.generation += 1;
        Ok(())
    }
    fn recheck_conflict(&mut self) -> io::Result<Disk> {
        let disk = self.read_external()?;
        let observed = self
            .external
            .observed
            .as_ref()
            .ok_or_else(|| io::Error::other("没有待处理的外部冲突"))?;
        if &disk != observed {
            self.record_conflict(disk, "磁盘再次变化；已更新冲突，请查看后重新选择");
            return Err(io::Error::other("磁盘再次变化，未执行旧选择"));
        }
        Ok(disk)
    }
    pub fn load_external(&mut self) -> io::Result<()> {
        self.finish_save(true)?;
        let disk = self.recheck_conflict()?;
        self.backup_conflict(&disk)?;
        self.loadable_disk(&disk, true)?;
        self.recheck_conflict()?;
        self.archive_pending()?;
        self.adopt_external(disk, true)
    }
    fn archive_pending(&mut self) -> io::Result<()> {
        if let Some(store) = &self.project {
            let path = crate::project::managed_path(&store.root, ".novelite-journal.json")?;
            if let Some(bytes) = read_optional(&path)? {
                let backup = self
                    .external
                    .backup
                    .as_ref()
                    .ok_or_else(|| io::Error::other("需要先备份冲突"))?;
                atomic_write(
                    &backup.join("abandoned-journal.json"),
                    &bytes,
                    WriteMode::CreateNew,
                )?;
                fs::remove_file(path)?;
                crate::storage::sync_directory(&store.root)?;
            }
        }
        Ok(())
    }
    pub fn save_conflict_copy(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.protect_layout(path.as_ref(), false)?;
        self.recheck_conflict()?;
        let destination = crate::keymap::resolve_destination(path.as_ref())?;
        self.library.protect(&destination, false)?;
        if self
            .project
            .as_ref()
            .is_some_and(|s| destination.starts_with(&s.root))
            || self.commands.keymap_path() == Some(destination.as_path())
            || destination == self.path
        {
            return Err(io::Error::other("本地副本须放在作品与配置之外的新文件"));
        }
        atomic_write(&destination, self.text().as_bytes(), WriteMode::CreateNew)
    }
    pub fn overwrite_external(&mut self) -> io::Result<()> {
        self.require_writable()?;
        self.finish_save(true)?;
        let disk = self.recheck_conflict()?;
        // A higher or unrecognized format is never overwritten, even by this choice.
        if let (Some(store), Some(bytes)) = (&self.project, &disk.manifest) {
            let manifest: crate::Project = serde_json::from_slice(bytes)?;
            crate::project::validate(&store.root, &manifest)?;
            if manifest.id != store.manifest.id {
                return Err(io::Error::other("作品身份改变；请另存本地副本"));
            }
        }
        self.backup_conflict(&disk)?;
        self.recheck_conflict()?;
        self.archive_pending()?;
        self.document.disk = stamp(&disk.body);
        self.document.new_file = false;
        if let Some(store) = &mut self.project {
            store.pending.remove(&store.active);
        }
        self.external.manifest = self.project.as_ref().map(|_| stamp(&disk.manifest));
        self.external.conflict = None;
        self.external.observed = None;
        self.saving.uncertain = true;
        self.saving.error = None;
        let result = self.save();
        if result.is_err()
            && let Ok(disk) = self.read_external()
        {
            self.record_conflict(disk, "覆盖未完成；正文仍在会话中，请重新选择");
        }
        result
    }
    pub(crate) fn reset_external_after_copy(&mut self) -> io::Result<()> {
        self.external = External::default();
        self.document.disk = Fingerprint::bytes(self.text().as_bytes());
        self.observe_manifest()
    }
    pub(crate) fn ensure_no_conflict(&self) -> io::Result<()> {
        if self.external.conflict.is_some() {
            Err(io::Error::other(
                "存在外部冲突；F1 选择冲突处理，当前稿保留",
            ))
        } else {
            Ok(())
        }
    }
}
pub(crate) fn commands() -> Vec<crate::Command> {
    vec![
        crate::Command {
            id: "conflicts".into(),
            title: "处理外部冲突 / 载入、另存或覆盖".into(),
            run: |work| {
                work.check_external()?;
                if work.external_conflict().is_none() {
                    return Err(io::Error::other("没有待处理的外部冲突"));
                }
                work.show_navigation(crate::navigator::Kind::Conflicts)
            },
        },
        crate::Command {
            id: "check-external".into(),
            title: "立即检查外部文件变化".into(),
            run: |work| {
                work.check_external()?;
                Ok(crate::Outcome::Continue)
            },
        },
    ]
}
pub(crate) fn copy_backups(source: &Path, target: &Path) -> io::Result<()> {
    fn copy_tree(source: &Path, target: &Path, relative: &Path) -> io::Result<()> {
        let path = crate::project::managed_path(source, &relative.to_string_lossy())?;
        if !path.try_exists()? {
            return Ok(());
        }
        let destination = target.join(relative);
        if path.is_dir() {
            fs::create_dir(&destination)?;
            for entry in fs::read_dir(&path)? {
                copy_tree(source, target, &relative.join(entry?.file_name()))?;
            }
        } else {
            atomic_write(&destination, &fs::read(path)?, WriteMode::CreateNew)?;
        }
        Ok(())
    }
    copy_tree(source, target, Path::new("conflicts"))?;
    copy_tree(source, target, Path::new(".novelite-v1-backup.json"))
}
