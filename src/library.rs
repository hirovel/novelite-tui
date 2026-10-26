use crate::{
    Workspace,
    storage::{WriteMode, atomic_write},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecentKind {
    #[default]
    Project,
    File,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentProject {
    #[serde(default)]
    pub kind: RecentKind,
    pub root: PathBuf,
    pub title: String,
    pub chapter: String,
    pub positions: BTreeMap<String, usize>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    version: u32,
    projects: Vec<RecentProject>,
}
#[derive(Clone, Default)]
pub(crate) struct Library {
    path: Option<PathBuf>,
    pub entries: Vec<RecentProject>,
    forgotten: HashSet<PathBuf>,
    source: Option<Vec<u8>>,
    blocked: Option<String>,
    warning: Option<String>,
}
impl Library {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub(crate) fn read(path: &Path) -> io::Result<Self> {
        let path = crate::keymap::resolve_destination(path)?;
        let mut source = None;
        let mut entries = match fs::read(&path) {
            Ok(bytes) => {
                let file: LibraryFile = serde_json::from_slice(&bytes)?;
                if !matches!(file.version, 1 | 2) {
                    return Err(io::Error::other("不支持的最近作品格式"));
                }
                source = Some(bytes);
                file.projects
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => vec![],
            Err(error) => return Err(error),
        };
        for entry in &mut entries {
            entry.root = crate::keymap::resolve_destination(&entry.root)?;
        }
        let mut seen = HashSet::new();
        entries.retain(|entry| seen.insert(entry.root.clone()));
        let library = Self {
            path: Some(path),
            entries,
            forgotten: HashSet::new(),
            source,
            blocked: None,
            warning: None,
        };
        for entry in &library.entries {
            library.protect(&entry.root, entry.kind == RecentKind::Project)?;
        }
        Ok(library)
    }
    pub fn protect(&self, destination: &Path, project: bool) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let destination = crate::keymap::resolve_destination(destination)?;
        if *path == destination || project && path.starts_with(&destination) {
            return Err(io::Error::other("最近作品记录必须放在文稿和作品文件夹之外"));
        }
        Ok(())
    }
    pub(crate) fn unavailable(path: PathBuf, error: &io::Error) -> Self {
        let warning = format!("最近记录未保存：{error}；正文保存不受影响");
        let path = crate::keymap::resolve_destination(&path)
            .or_else(|_| std::path::absolute(&path))
            .unwrap_or(path);
        Self {
            path: Some(path),
            blocked: Some(error.to_string()),
            warning: Some(warning),
            ..Self::default()
        }
    }
    pub(crate) fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
    pub(crate) fn persist(&mut self) -> io::Result<()> {
        if let Some(error) = &self.blocked {
            return Err(io::Error::other(error.clone()));
        }
        if let Some(path) = &self.path {
            for entry in &self.entries {
                self.protect(&entry.root, entry.kind == RecentKind::Project)?;
            }
            let actual = match fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if actual != self.source {
                return Err(io::Error::other("最近记录已被其他程序修改，请重启后重试"));
            }
            fs::create_dir_all(path.parent().unwrap())?;
            let bytes = serde_json::to_vec_pretty(&LibraryFile {
                version: 2,
                projects: self.entries.clone(),
            })?;
            atomic_write(path, &bytes, WriteMode::Replace)?;
            self.source = Some(bytes);
        }
        self.warning = None;
        Ok(())
    }
}
impl Workspace {
    pub fn load_library(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let resolved = crate::keymap::resolve_destination(path.as_ref())?;
        self.protect_layout(&resolved, false)?;
        if self
            .commands
            .keymap_path()
            .and_then(|path| crate::keymap::resolve_destination(path).ok())
            .as_ref()
            == Some(&resolved)
        {
            return Err(io::Error::other("最近作品记录不能与键位配置共用文件"));
        }
        let candidate = Library {
            path: Some(resolved.clone()),
            ..Library::default()
        };
        candidate.protect(
            self.project
                .as_ref()
                .map_or(self.path.as_path(), |store| &store.root),
            self.project.is_some(),
        )?;
        let restore = !self.is_dirty() && self.chapters.is_empty();
        self.library = match Library::read(path.as_ref()) {
            Ok(library) => library,
            Err(error) => {
                self.library = Library::unavailable(resolved, &error);
                return Err(error);
            }
        };
        if restore {
            self.restore_location()?;
        }
        self.remember_location()
    }
    pub fn library_warning(&self) -> Option<&str> {
        self.library.warning()
    }
    pub fn recent_projects(&self) -> &[RecentProject] {
        &self.library.entries
    }
    pub fn forget_project(&mut self, root: impl AsRef<Path>) -> io::Result<()> {
        let root = crate::keymap::resolve_destination(root.as_ref())?;
        let mut next = self.library.clone();
        next.entries.retain(|entry| entry.root != root);
        next.forgotten.insert(root);
        next.persist()?;
        self.library = next;
        Ok(())
    }
    pub fn resume(state: impl AsRef<Path>) -> io::Result<Option<Self>> {
        let library = Library::read(state.as_ref())?;
        let Some(entry) = library.entries.first() else {
            return Ok(None);
        };
        let mut work = Self::open_recent(entry)?;
        work.library = library;
        work.restore_location()?;
        Ok(Some(work))
    }
    pub(crate) fn open_recent(entry: &RecentProject) -> io::Result<Self> {
        let path = crate::user_path::resolve(&entry.root, false)?;
        match entry.kind {
            RecentKind::Project if path.is_dir() => Self::open_project(path),
            RecentKind::File if path.is_file() => Self::open(path),
            _ => Err(io::Error::other("最近记录的路径类型已改变，请重新打开")),
        }
    }
    pub(crate) fn switch_recent(&mut self, root: &Path) -> io::Result<()> {
        if let Some(entry) = self.library.entries.iter().find(|e| e.root == root) {
            let path = crate::user_path::resolve(root, false)?;
            if (entry.kind == RecentKind::File) != path.is_file() {
                return Err(io::Error::other("最近记录的路径类型已改变，请重新打开"));
            }
        }
        self.switch_path(root)
    }
    /// Open an existing standalone manuscript or a managed project without abandoning edits.
    pub fn switch_path(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        let expanded = crate::user_path::expand(path.as_ref())?;
        // Opening the already-active chapter is a no-op, not a standalone request.
        if fs::canonicalize(&expanded).ok().as_deref() == Some(self.path.as_path()) {
            return Ok(());
        }
        let path = crate::user_path::resolve(&expanded, false)?;
        if path.is_dir() {
            return self.switch_project(path);
        }
        self.protect_user_destination(&path, false)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::open(&path)?;
        self.adopt_project(next)
    }

    pub fn switch_project(&mut self, root: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        let root = fs::canonicalize(crate::user_path::expand(root.as_ref())?)?;
        if self
            .project
            .as_ref()
            .is_some_and(|store| store.root == root)
        {
            return Ok(());
        }
        self.protect_user_destination(&root, true)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::open_project(&root)?;
        self.adopt_project(next)
    }
    pub fn new_project(&mut self, root: impl AsRef<Path>, title: &str) -> io::Result<()> {
        self.require_export_idle()?;
        let root = crate::user_path::expand(root.as_ref())?;
        let root = root.as_path();
        self.protect_user_destination(root, true)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::create_project(root, title)?;
        self.adopt_project(next)
    }
    pub(crate) fn adopt_project(&mut self, mut next: Self) -> io::Result<()> {
        self.require_export_idle()?;
        next.library = self.library.clone();
        let root = next.location_root().to_owned();
        next.library.forgotten.remove(&root);
        next.restore_location()?;
        next.remember_location()?;
        next.commands = std::mem::take(&mut self.commands);
        next.modules.config = self.modules.config.clone();
        next.modules.path = self.modules.path.clone();
        next.enhanced_keyboard = self.enhanced_keyboard;
        next.color_support = self.color_support;
        next.set_design_motion(self.design_motion_enabled());
        next.configure_mouse(self.mouse_enabled());
        next.clipboard = std::mem::take(&mut self.clipboard);
        next.history.clock = self.history.clock.clone();
        if let Some(path) = next.commands.keymap_path().map(Path::to_owned) {
            next.load_keymap(path);
        }
        *self = next;
        Ok(())
    }
    fn location_root(&self) -> &Path {
        self.project
            .as_ref()
            .map_or(self.path.as_path(), |store| store.root.as_path())
    }
    pub(crate) fn remember_location(&mut self) -> io::Result<()> {
        let root = self.location_root().to_owned();
        if self.library.forgotten.contains(&root) {
            return Ok(());
        }
        let mut next = self.library.clone();
        let mut positions = next
            .entries
            .iter()
            .find(|e| e.root == root)
            .map(|e| e.positions.clone())
            .unwrap_or_default();
        let (kind, title, chapter) = if let Some(store) = &self.project {
            positions.extend(
                self.chapters
                    .iter()
                    .map(|(id, doc)| (id.clone(), doc.cursor)),
            );
            (
                RecentKind::Project,
                store.manifest.title.clone(),
                store.active.clone(),
            )
        } else {
            (
                RecentKind::File,
                self.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                String::new(),
            )
        };
        positions.insert(chapter.clone(), self.document.cursor);
        next.entries.retain(|entry| entry.root != root);
        next.entries.insert(
            0,
            RecentProject {
                kind,
                root,
                title,
                chapter,
                positions,
            },
        );
        if let Err(error) = next.persist() {
            next.warning = Some(format!("最近记录未保存：{error}；正文保存不受影响"));
        }
        self.library = next;
        Ok(())
    }
    pub(crate) fn restore_location(&mut self) -> io::Result<()> {
        if self.external_conflict().is_some() {
            return Ok(());
        }
        let Some(store) = &self.project else {
            self.restore_cursor();
            return Ok(());
        };
        let Some(entry) = self
            .library
            .entries
            .iter()
            .find(|entry| entry.root == store.root)
            .cloned()
        else {
            return Ok(());
        };
        let restore_chapter =
            store.active != entry.chapter && store.chapter(&entry.chapter).is_some();
        self.restore_cursor();
        if restore_chapter {
            let store = self.project.as_ref().unwrap();
            let path = store.chapter_path(&entry.chapter)?;
            let opened = Self::open(&path)?;
            if opened.document.new_file {
                return Err(io::Error::new(io::ErrorKind::NotFound, "上次章节正文缺失"));
            }
            let previous = std::mem::replace(&mut self.document, opened.document);
            let store = self.project.as_mut().unwrap();
            self.chapters.insert(store.active.clone(), previous);
            store.active = entry.chapter;
            self.path = path;
        }
        self.restore_cursor();
        Ok(())
    }
    pub(crate) fn restore_cursor(&mut self) {
        use unicode_segmentation::UnicodeSegmentation;
        let root = self.location_root();
        let chapter = self
            .project
            .as_ref()
            .map_or("", |store| store.active.as_str());
        if let Some(position) = self
            .library
            .entries
            .iter()
            .find(|entry| entry.root == root)
            .and_then(|entry| entry.positions.get(chapter))
        {
            self.document.cursor = self
                .document
                .text
                .grapheme_indices(true)
                .map(|(offset, _)| offset)
                .chain(std::iter::once(self.document.text.len()))
                .take_while(|offset| offset <= position)
                .last()
                .unwrap_or(0);
        }
    }
}
