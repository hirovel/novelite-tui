use crate::storage::{WriteMode, atomic_write, sync_directory};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_words: Option<u64>,
    pub volumes: Vec<Volume>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recycle_bin: Vec<Recycled>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recycled {
    pub id: String,
    pub title: String,
    pub chapters: Vec<Chapter>,
}
impl Project {
    pub(crate) fn all_chapters(&self) -> impl Iterator<Item = &Chapter> {
        self.volumes
            .iter()
            .flat_map(|v| &v.chapters)
            .chain(self.recycle_bin.iter().flat_map(|r| &r.chapters))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    pub id: String,
    pub title: String,
    pub chapters: Vec<Chapter>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "crate::ChapterPlan::is_empty")]
    pub plan: crate::ChapterPlan,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    manifest: Project,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default)]
    bodies: Vec<BodyWrite>,
    #[serde(default)]
    expected_manifest: Option<crate::fingerprint::Fingerprint>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyWrite {
    id: String,
    text: String,
    create: bool,
    #[serde(default)]
    expected: Option<crate::fingerprint::Fingerprint>,
}

#[derive(Clone)]
pub(crate) struct ProjectStore {
    pub root: PathBuf,
    pub manifest: Project,
    pub dirty: bool,
    pub revision: u64,
    pub lock: Option<std::sync::Arc<fs::File>>,
    pub active: String,
    pub pending: std::collections::BTreeMap<String, String>,
    pub recovery_text: Option<String>,
}
impl ProjectStore {
    pub fn pending_matches_disk(&self) -> io::Result<bool> {
        let path = managed_path(&self.root, ".novelite-journal.json")?;
        if !path.try_exists()? {
            return Ok(false);
        }
        let journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
        validate(&self.root, &journal.manifest)?;
        if journal.manifest.id != self.manifest.id {
            return Ok(false);
        }
        if let Some(expected) = journal.expected_manifest {
            let current = crate::fingerprint::Fingerprint::read(&self.root.join("novelite.json"))?;
            let target = crate::fingerprint::Fingerprint::bytes(&serde_json::to_vec_pretty(
                &journal.manifest,
            )?);
            if current != expected && current != target {
                return Ok(false);
            }
        }
        for body in &journal.bodies {
            let chapter = journal
                .manifest
                .all_chapters()
                .find(|c| c.id == body.id)
                .ok_or_else(|| io::Error::other("日志正文没有对应章节"))?;
            let current =
                crate::fingerprint::Fingerprint::read(&managed_path(&self.root, &chapter.path)?)?;
            let target = crate::fingerprint::Fingerprint::bytes(body.text.as_bytes());
            if current != target && Some(current) != body.expected {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn create(root: &Path, title: &str) -> io::Result<Self> {
        let chapter_id = identity()?;
        let manifest = Project {
            version: 1,
            id: identity()?,
            title: title.into(),
            recycle_bin: vec![],
            target_words: None,
            volumes: vec![Volume {
                id: identity()?,
                title: "第一卷".into(),
                chapters: vec![Chapter {
                    path: format!("chapters/{chapter_id}.md"),
                    id: chapter_id,
                    title: "第一章".into(),
                    plan: Default::default(),
                }],
            }],
        };
        fs::create_dir(root)?;
        let root = fs::canonicalize(root)?;
        fs::create_dir(root.join("chapters"))?;
        let lock = acquire_lock(&root)?;
        let mut store = Self {
            recovery_text: None,
            active: manifest.volumes[0].chapters[0].id.clone(),
            pending: std::collections::BTreeMap::new(),
            root,
            manifest,
            dirty: true,
            revision: 0,
            lock,
        };
        store.commit("")?;
        Ok(store)
    }
    pub fn open(root: &Path) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        if root.join(".novelite-importing").try_exists()? {
            return Err(io::Error::other(
                "未完成的导入，源文件未改动；请使用新目录重试",
            ));
        }
        managed_path(&root, "novelite.json")?;
        // Validate an existing manifest before recovery: never migrate a newer format implicitly.
        let mut corrupt_manifest = false;
        if root.join("novelite.json").try_exists()? {
            match serde_json::from_slice::<Project>(&fs::read(root.join("novelite.json"))?) {
                Ok(manifest) => validate(&root, &manifest)?,
                Err(error) => {
                    if !root.join(".novelite-journal.json").try_exists()? {
                        return Err(error.into());
                    }
                    corrupt_manifest = true;
                }
            }
        }
        let lock = acquire_lock(&root)?;
        if root.join(".novelite-journal.json").try_exists()? {
            if lock.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "作品保存或恢复中，请稍后只读打开",
                ));
            }
            if corrupt_manifest {
                return Self::open_conflicted(root, lock);
            }
            if let Err(error) = recover(&root) {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
                return Self::open_conflicted(root, lock);
            }
        }
        let manifest: Project = serde_json::from_slice(&fs::read(root.join("novelite.json"))?)?;
        validate(&root, &manifest)?;
        Ok(Self {
            recovery_text: None,
            active: manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .next()
                .unwrap()
                .id
                .clone(),
            pending: std::collections::BTreeMap::new(),
            root,
            manifest,
            dirty: false,
            revision: 0,
            lock,
        })
    }
    fn open_conflicted(root: PathBuf, lock: Option<std::sync::Arc<fs::File>>) -> io::Result<Self> {
        let journal: Journal =
            serde_json::from_slice(&fs::read(managed_path(&root, ".novelite-journal.json")?)?)?;
        validate(&root, &journal.manifest)?;
        // This path never writes. An unreadable manifest must not hide the journal's draft;
        // explicit conflict choices validate the current manifest again before any write.
        let mut pending = std::collections::BTreeMap::new();
        for body in &journal.bodies {
            if !journal
                .manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .any(|c| c.id == body.id)
                || pending.insert(body.id.clone(), body.text.clone()).is_some()
            {
                return Err(io::Error::other("恢复正文身份无效或重复"));
            }
        }
        let (active, text) = if let Some(body) = journal.bodies.last() {
            (body.id.clone(), body.text.clone())
        } else if let Some(text) = journal.text {
            (journal.manifest.volumes[0].chapters[0].id.clone(), text)
        } else {
            return Err(io::Error::other("恢复日志没有本地正文"));
        };
        Ok(Self {
            root,
            manifest: journal.manifest,
            lock,
            active,
            pending,
            dirty: true,
            revision: 0,
            recovery_text: Some(text),
        })
    }
    pub fn path(&self) -> PathBuf {
        self.root.join(
            &self
                .chapter(&self.active)
                .expect("active chapter exists")
                .path,
        )
    }
    pub fn chapter(&self, id: &str) -> Option<&Chapter> {
        self.manifest
            .volumes
            .iter()
            .flat_map(|volume| &volume.chapters)
            .find(|chapter| chapter.id == id)
    }
    pub fn chapter_path(&self, id: &str) -> io::Result<PathBuf> {
        managed_path(
            &self.root,
            &self
                .chapter(id)
                .ok_or_else(|| io::Error::other("章节不存在"))?
                .path,
        )
    }
    pub fn changed(&mut self) {
        self.revision += 1;
        self.dirty = true;
    }
    pub fn create_volume(&mut self, title: &str) -> io::Result<String> {
        let id = identity()?;
        self.manifest.volumes.push(Volume {
            id: id.clone(),
            title: title.into(),
            chapters: vec![],
        });
        self.changed();
        Ok(id)
    }
    pub fn create_chapter(&mut self, volume: &str, title: &str) -> io::Result<String> {
        let volume = self
            .manifest
            .volumes
            .iter_mut()
            .find(|v| v.id == volume)
            .ok_or_else(|| io::Error::other("卷不存在"))?;
        let id = identity()?;
        volume.chapters.push(Chapter {
            id: id.clone(),
            title: title.into(),
            path: format!("chapters/{id}.md"),
            plan: Default::default(),
        });
        self.pending.insert(id.clone(), String::new());
        self.changed();
        Ok(id)
    }
    pub fn copy_to(&self, root: &Path, text: &str) -> io::Result<Self> {
        validate(&self.root, &self.manifest)?;
        fs::create_dir(root)?;
        let root = fs::canonicalize(root)?;
        let lock = acquire_lock(&root)?;
        fs::create_dir(root.join("chapters"))?;
        let mut copy = Self {
            recovery_text: None,
            active: self.active.clone(),
            pending: self.pending.clone(),
            root,
            manifest: self.manifest.clone(),
            dirty: true,
            revision: self.revision,
            lock,
        };
        for chapter in self
            .manifest
            .all_chapters()
            .filter(|chapter| chapter.id != self.active && !self.pending.contains_key(&chapter.id))
        {
            let contents = fs::read(managed_path(&self.root, &chapter.path)?)?;
            atomic_write(
                &copy.root.join(&chapter.path),
                &contents,
                WriteMode::CreateNew,
            )?;
        }
        crate::history::copy_history(&self.root, &copy.root, &self.manifest)?;
        crate::external::copy_backups(&self.root, &copy.root)?;
        copy.commit(text)?;
        Ok(copy)
    }
    pub fn commit(&mut self, text: &str) -> io::Result<()> {
        self.commit_inner(text, None, None)
    }
    pub fn commit_guarded(
        &mut self,
        text: &str,
        body: crate::fingerprint::Fingerprint,
        manifest: Option<crate::fingerprint::Fingerprint>,
    ) -> io::Result<()> {
        self.commit_inner(text, Some(body), manifest)
    }
    fn commit_inner(
        &mut self,
        text: &str,
        mut expected_body: Option<crate::fingerprint::Fingerprint>,
        mut expected_manifest: Option<crate::fingerprint::Fingerprint>,
    ) -> io::Result<()> {
        if self.lock.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "作品已被其他实例打开，只读模式",
            ));
        }
        validate(&self.root, &self.manifest)?;
        validate_current(&self.root, &self.manifest.id)?;
        if self.root.join(".novelite-journal.json").try_exists()? {
            let previous: Journal =
                serde_json::from_slice(&fs::read(self.root.join(".novelite-journal.json"))?)?;
            recover(&self.root)?;
            expected_manifest = Some(crate::fingerprint::Fingerprint::bytes(
                &serde_json::to_vec_pretty(&previous.manifest)?,
            ));
            if let Some(body) = previous.bodies.iter().find(|b| b.id == self.active) {
                expected_body = Some(crate::fingerprint::Fingerprint::bytes(body.text.as_bytes()));
            }
        }
        // A failed earlier attempt may already have published these identities.
        // Only paths absent from the committed manifest are still create-only.
        let persisted: std::collections::HashSet<String> =
            if self.root.join("novelite.json").is_file() {
                let current: Project =
                    serde_json::from_slice(&fs::read(self.root.join("novelite.json"))?)?;
                current
                    .all_chapters()
                    .map(|chapter| chapter.id.clone())
                    .collect()
            } else {
                std::collections::HashSet::new()
            };
        let mut bodies: Vec<_> = self
            .pending
            .iter()
            .filter(|(id, _)| **id != self.active)
            .map(|(id, text)| BodyWrite {
                id: id.clone(),
                text: text.clone(),
                create: !persisted.contains(id),
                expected: Some(if persisted.contains(id) {
                    crate::fingerprint::Fingerprint::bytes(text.as_bytes())
                } else {
                    crate::fingerprint::Fingerprint::Missing
                }),
            })
            .collect();
        bodies.push(BodyWrite {
            id: self.active.clone(),
            text: text.into(),
            create: self.pending.contains_key(&self.active) && !persisted.contains(&self.active),
            expected: expected_body,
        });
        let journal = Journal {
            manifest: self.manifest.clone(),
            text: None,
            bodies,
            expected_manifest,
        };
        let journal_path = managed_path(&self.root, ".novelite-journal.json")?;
        atomic_write(
            &journal_path,
            &serde_json::to_vec(&journal)?,
            WriteMode::Replace,
        )?;
        apply_journal(&self.root, &journal)?;
        self.dirty = false;
        self.pending.clear();
        Ok(())
    }
}
fn recover(root: &Path) -> io::Result<()> {
    let path = managed_path(root, ".novelite-journal.json")?;
    let journal: Journal = serde_json::from_slice(&fs::read(&path)?)?;
    validate(root, &journal.manifest)?;
    validate_current(root, &journal.manifest.id)?;
    apply_journal(root, &journal)
}
fn validate_current(root: &Path, id: &str) -> io::Result<()> {
    if root.join("novelite.json").is_file() {
        let current: Project = serde_json::from_slice(&fs::read(root.join("novelite.json"))?)?;
        validate(root, &current)?;
        if current.id != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "恢复日志与作品不匹配",
            ));
        }
    }
    Ok(())
}
fn apply_journal(root: &Path, journal: &Journal) -> io::Result<()> {
    if let Some(expected) = journal.expected_manifest {
        crate::fingerprint::ensure(
            &managed_path(root, "novelite.json")?,
            expected,
            crate::fingerprint::Fingerprint::bytes(&serde_json::to_vec_pretty(&journal.manifest)?),
        )?;
    }
    // Preserve the old manifest before the first v2 transaction publishes any writes.
    // Chapter paths and bytes do not change during this format upgrade.
    let manifest_path = managed_path(root, "novelite.json")?;
    if journal.manifest.version == 2 && manifest_path.is_file() {
        let bytes = fs::read(&manifest_path)?;
        let old: Project = serde_json::from_slice(&bytes)?;
        if old.version == 1 {
            let backup = managed_path(root, ".novelite-v1-backup.json")?;
            if backup.try_exists()? {
                if fs::read(&backup)? != bytes {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "格式升级备份路径已占用",
                    ));
                }
            } else {
                atomic_write(&backup, &bytes, WriteMode::CreateNew)?;
            }
        }
    }
    let legacy = journal.text.as_ref().map(|text| BodyWrite {
        id: journal.manifest.volumes[0].chapters[0].id.clone(),
        text: text.clone(),
        create: false,
        expected: None,
    });
    let mut ids = std::collections::HashSet::new();
    for body in journal.bodies.iter().chain(legacy.iter()) {
        let chapter = journal
            .manifest
            .volumes
            .iter()
            .flat_map(|v| &v.chapters)
            .find(|chapter| chapter.id == body.id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "恢复正文没有对应章节"))?;
        if !ids.insert(&body.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "恢复正文身份重复",
            ));
        }
        let path = managed_path(root, &chapter.path)?;
        if let Some(expected) = body.expected {
            crate::fingerprint::ensure(
                &path,
                expected,
                crate::fingerprint::Fingerprint::bytes(body.text.as_bytes()),
            )?;
        }
        if body.create && path.try_exists()? {
            if fs::read(&path)? != body.text.as_bytes() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "新章节路径已被其他内容占用",
                ));
            }
        } else {
            atomic_write(
                &path,
                body.text.as_bytes(),
                if body.create {
                    WriteMode::CreateNew
                } else {
                    WriteMode::Replace
                },
            )?;
        }
    }
    if let Some(expected) = journal.expected_manifest {
        crate::fingerprint::ensure(
            &managed_path(root, "novelite.json")?,
            expected,
            crate::fingerprint::Fingerprint::bytes(&serde_json::to_vec_pretty(&journal.manifest)?),
        )?;
    }
    atomic_write(
        &managed_path(root, "novelite.json")?,
        &serde_json::to_vec_pretty(&journal.manifest)?,
        WriteMode::Replace,
    )?;
    fs::remove_file(managed_path(root, ".novelite-journal.json")?)?;
    sync_directory(root)
}
fn acquire_lock(root: &Path) -> io::Result<Option<std::sync::Arc<fs::File>>> {
    managed_path(root, ".novelite.lock")?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".novelite.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(std::sync::Arc::new(file))),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}
pub(crate) fn validate(root: &Path, project: &Project) -> io::Result<()> {
    if !(1..=2).contains(&project.version)
        || (project.version == 1 && !project.recycle_bin.is_empty())
        || project
            .volumes
            .iter()
            .all(|volume| volume.chapters.is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "不支持的作品版本或空结构",
        ));
    }
    let mut identities = std::collections::HashSet::new();
    let mut check_id = |id: &str| -> io::Result<()> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !identities.insert(id.to_owned())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "作品身份无效或重复",
            ));
        }
        Ok(())
    };
    check_id(&project.id)?;
    for volume in &project.volumes {
        check_id(&volume.id)?;
    }
    for item in &project.recycle_bin {
        check_id(&item.id)?;
    }
    for chapter in project.all_chapters() {
        check_id(&chapter.id)?;
        if chapter.path != format!("chapters/{}.md", chapter.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "正文路径必须由稳定章节身份生成",
            ));
        }
        managed_path(root, &chapter.path)?;
    }
    managed_path(root, "novelite.json")?;
    Ok(())
}
pub(crate) fn managed_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(part) = component else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "路径越界"));
        };
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "作品内部不能使用符号链接",
                ));
            }
            Ok(_) => {
                if !fs::canonicalize(&path)?.starts_with(root) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "路径越界"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(path)
}
pub(crate) fn identity() -> io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
