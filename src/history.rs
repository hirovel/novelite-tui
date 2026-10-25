use crate::{
    Action, Workspace,
    project::{identity, managed_path},
    storage::{WriteMode, atomic_write},
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub created: u64,
    pub marked: bool,
    pub sequence: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicy {
    pub recent: usize,
    pub days: u64,
    pub max_bytes: u64,
}
impl Default for HistoryPolicy {
    fn default() -> Self {
        Self {
            recent: 50,
            days: 7,
            max_bytes: 256 * 1024 * 1024,
        }
    }
}
fn policy(root: &Path) -> io::Result<HistoryPolicy> {
    let path = managed_path(root, "history-policy.json")?;
    let value = if path.try_exists()? {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        HistoryPolicy::default()
    };
    validate_policy(&value)?;
    Ok(value)
}
fn validate_policy(policy: &HistoryPolicy) -> io::Result<()> {
    if policy.recent == 0 || policy.max_bytes == 0 {
        return Err(io::Error::other("历史至少保留 1 个近期点，容量必须大于 0"));
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    chapter: String,
    snapshot: Snapshot,
    text: String,
    checksum: u64,
}
// FNV-1a detects accidental changes; it is not a security signature.
fn checksum(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn directory(root: &Path, chapter: &str) -> io::Result<PathBuf> {
    if !valid_id(chapter) {
        return Err(io::Error::other("历史章节身份无效"));
    }
    managed_path(root, &format!("history/{chapter}"))
}
fn records(root: &Path, chapter: &str) -> io::Result<Vec<Record>> {
    let dir = directory(root, chapter)?;
    if !dir.try_exists()? {
        return Ok(vec![]);
    }
    let mut records = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Atomic-write temporary files are not published snapshots.
        if !name.ends_with(".json") {
            continue;
        }
        let path = managed_path(root, &format!("history/{chapter}/{name}"))?;
        let record: Record = serde_json::from_slice(&fs::read(path)?).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("历史快照损坏：{name}: {e}"),
            )
        })?;
        if record.version != 1
            || record.chapter != chapter
            || !valid_id(&record.snapshot.id)
            || name != format!("{}.json", record.snapshot.id)
            || checksum(&record.text) != record.checksum
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("历史快照校验失败：{name}"),
            ));
        }
        records.push(record);
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.snapshot.sequence));
    Ok(records)
}
fn capture(root: &Path, chapter: &str, text: String, created: u64, marked: bool) -> io::Result<()> {
    let records = records(root, chapter)?;
    if let Some(last) = records.first()
        && last.text == text
    {
        if marked && !last.snapshot.marked {
            let mut record = records.into_iter().next().unwrap();
            record.snapshot.marked = true;
            atomic_write(
                &directory(root, chapter)?.join(format!("{}.json", record.snapshot.id)),
                &serde_json::to_vec(&record)?,
                WriteMode::Replace,
            )?;
        }
        return Ok(());
    }
    let dir = directory(root, chapter)?;
    fs::create_dir_all(&dir)?;
    let id = identity()?;
    let sequence = records
        .first()
        .map_or(1, |r| r.snapshot.sequence.saturating_add(1));
    let record = Record {
        version: 1,
        chapter: chapter.into(),
        snapshot: Snapshot {
            id: id.clone(),
            created,
            marked,
            sequence,
        },
        checksum: checksum(&text),
        text,
    };
    let policy = policy(root)?;
    let mut all = vec![record];
    all.extend(records);
    let mut recent = 0;
    let mut days = std::collections::HashSet::new();
    let keep: Vec<bool> = all
        .iter()
        .map(|r| {
            if r.snapshot.marked {
                return true;
            }
            recent += 1;
            let day = r.snapshot.created / 86_400;
            let daily = (created / 86_400).saturating_sub(day) < policy.days && days.insert(day);
            recent <= policy.recent || daily
        })
        .collect();
    let mut retained_bytes = 0u64;
    for (record, keep) in all.iter().zip(&keep) {
        if *keep {
            retained_bytes =
                retained_bytes.saturating_add(serde_json::to_vec(record)?.len() as u64);
        }
    }
    if retained_bytes > policy.max_bytes {
        return Err(io::Error::other(
            "本章历史超过配置容量；请提高容量，手动标记点不会自动删除",
        ));
    }
    atomic_write(
        &dir.join(format!("{id}.json")),
        &serde_json::to_vec(&all[0])?,
        WriteMode::CreateNew,
    )?;
    // Publish the new recovery point before removing any expired automatic points.
    for (record, keep) in all.iter().zip(keep) {
        if !keep {
            fs::remove_file(dir.join(format!("{}.json", record.snapshot.id)))?;
        }
    }
    crate::storage::sync_directory(&dir)
}

pub(crate) struct History {
    job: Option<JoinHandle<io::Result<()>>>,
    checked: std::collections::HashMap<String, u64>,
    pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub error: Option<String>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            job: None,
            checked: Default::default(),
            clock: Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }),
            error: None,
        }
    }
}
impl History {
    fn finish(&mut self, wait: bool) -> io::Result<()> {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| wait || job.is_finished())
        {
            return Ok(());
        }
        let result = self
            .job
            .take()
            .unwrap()
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("历史线程异常；当前文稿保留")));
        self.error = result.as_ref().err().map(|e| format!("历史快照失败：{e}"));
        result
    }
}
impl Drop for History {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.join();
        }
    }
}
impl Workspace {
    /// Clock adapter returns Unix seconds; the default uses the local system clock.
    pub fn set_history_clock(&mut self, clock: Arc<dyn Fn() -> u64 + Send + Sync>) {
        self.history.clock = clock;
        self.history.checked.clear();
    }
    pub fn history_entries(&mut self) -> io::Result<Vec<Snapshot>> {
        self.history.finish(true)?;
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?;
        Ok(records(&store.root, &store.active)?
            .into_iter()
            .map(|r| r.snapshot)
            .collect())
    }
    pub fn preview_snapshot(&mut self, id: &str) -> io::Result<String> {
        self.history.finish(true)?;
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?;
        records(&store.root, &store.active)?
            .into_iter()
            .find(|r| r.snapshot.id == id)
            .map(|r| r.text)
            .ok_or_else(|| io::Error::other("历史快照不存在"))
    }
    pub fn mark_snapshot(&mut self) -> io::Result<()> {
        self.checkpoint(true, true)
    }
    pub fn history_policy(&self) -> io::Result<HistoryPolicy> {
        policy(
            &self
                .project
                .as_ref()
                .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?
                .root,
        )
    }
    pub fn set_history_policy(&mut self, value: HistoryPolicy) -> io::Result<()> {
        self.require_writable()?;
        self.history.finish(true)?;
        validate_policy(&value)?;
        let root = &self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?
            .root;
        atomic_write(
            &managed_path(root, "history-policy.json")?,
            &serde_json::to_vec_pretty(&value)?,
            WriteMode::Replace,
        )
    }
    pub fn history_usage(&mut self) -> io::Result<u64> {
        self.history.finish(true)?;
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?;
        records(&store.root, &store.active)?
            .iter()
            .try_fold(0u64, |sum, record| {
                Ok(sum + serde_json::to_vec(record)?.len() as u64)
            })
    }
    pub fn restore_snapshot(&mut self, id: &str) -> io::Result<()> {
        self.require_writable()?;
        let text = self.preview_snapshot(id)?;
        self.snapshot_before_change()?;
        self.dispatch(Action::SelectAll)?;
        self.dispatch(Action::Insert(text))?;
        self.save()
    }
    pub fn history_tick(&mut self) -> io::Result<()> {
        self.history.finish(false)?;
        let Some(store) = &self.project else {
            return Ok(());
        };
        let now = (self.history.clock)();
        let last = self
            .history
            .checked
            .entry(store.active.clone())
            .or_insert(now);
        if now.saturating_sub(*last) >= 300 && self.history.job.is_none() {
            *last = now;
            self.checkpoint(false, false)?;
        }
        Ok(())
    }
    pub(crate) fn checkpoint(&mut self, marked: bool, wait: bool) -> io::Result<()> {
        self.history.finish(wait)?;
        if !marked && (self.project.is_none() || self.is_read_only() || !self.document.has_edits())
        {
            return Ok(());
        }
        self.start_snapshot(marked, wait)
    }
    pub fn snapshot_before_change(&mut self) -> io::Result<()> {
        self.history.finish(true)?;
        self.start_snapshot(false, true)
    }
    fn start_snapshot(&mut self, marked: bool, wait: bool) -> io::Result<()> {
        self.require_writable()?;
        if self.history.job.is_some() {
            return Ok(());
        }
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("历史需要作品文件夹"))?
            .clone();
        let text = self.text().to_owned();
        let created = (self.history.clock)();
        self.history.error = None;
        self.history.job = Some(
            std::thread::Builder::new()
                .name("novelite-history".into())
                .spawn(move || {
                    let _lock = store.lock;
                    capture(&store.root, &store.active, text, created, marked)
                })?,
        );
        self.history.finish(wait)
    }
    pub fn history_warning(&self) -> Option<&str> {
        self.history.error.as_deref()
    }
    pub(crate) fn finish_history(&mut self) -> io::Result<()> {
        self.history.finish(true)
    }
}

pub(crate) fn copy_history(
    source: &Path,
    target: &Path,
    project: &crate::Project,
) -> io::Result<()> {
    let policy_path = managed_path(source, "history-policy.json")?;
    if policy_path.try_exists()? {
        let value = policy(source)?;
        atomic_write(
            &managed_path(target, "history-policy.json")?,
            &serde_json::to_vec_pretty(&value)?,
            WriteMode::CreateNew,
        )?;
    }
    for chapter in project.all_chapters() {
        for record in records(source, &chapter.id)? {
            let dir = directory(target, &chapter.id)?;
            fs::create_dir_all(&dir)?;
            atomic_write(
                &dir.join(format!("{}.json", record.snapshot.id)),
                &serde_json::to_vec(&record)?,
                WriteMode::CreateNew,
            )?;
        }
    }
    Ok(())
}
