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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentProject {
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
}
impl Library {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    fn read(path: &Path) -> io::Result<Self> {
        let path = crate::keymap::resolve_destination(path)?;
        let entries = match fs::read(&path) {
            Ok(bytes) => {
                let file: LibraryFile = serde_json::from_slice(&bytes)?;
                if file.version != 1 {
                    return Err(io::Error::other("不支持的最近作品格式"));
                }
                file.projects
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => vec![],
            Err(error) => return Err(error),
        };
        let library = Self {
            path: Some(path),
            entries,
            forgotten: HashSet::new(),
        };
        for entry in &library.entries {
            library.protect(&entry.root, true)?;
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
    fn persist(&self) -> io::Result<()> {
        if let Some(path) = &self.path {
            for entry in &self.entries {
                self.protect(&entry.root, true)?;
            }
            fs::create_dir_all(path.parent().unwrap())?;
            atomic_write(
                path,
                &serde_json::to_vec_pretty(&LibraryFile {
                    version: 1,
                    projects: self.entries.clone(),
                })?,
                WriteMode::Replace,
            )?;
        }
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
            path: Some(resolved),
            entries: vec![],
            forgotten: HashSet::new(),
        };
        candidate.protect(
            self.project
                .as_ref()
                .map_or(self.path.as_path(), |store| &store.root),
            self.project.is_some(),
        )?;
        let restore = !self.is_dirty() && self.chapters.is_empty();
        self.library = Library::read(path.as_ref())?;
        if restore {
            self.restore_location()?;
        }
        self.remember_location()
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
        let mut work = Self::open_project(&entry.root)?;
        work.library = library;
        work.restore_location()?;
        Ok(Some(work))
    }
    /// Open an existing standalone manuscript or a managed project without abandoning edits.
    pub fn switch_path(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        let path = fs::canonicalize(path)?;
        if path.is_dir() {
            return self.switch_project(path);
        }
        if path == self.path {
            return Ok(());
        }
        if !path.is_file()
            || !path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("txt") || ext.eq_ignore_ascii_case("md")
                })
        {
            return Err(io::Error::other("请选择 TXT/Markdown 文件或作品文件夹"));
        }
        // Managed chapters must retain the project's lock, journal and shared state.
        for parent in path.ancestors().skip(1) {
            if parent.join("novelite.json").try_exists()? {
                return Err(io::Error::other(
                    "这是作品内的文件，请打开所属作品文件夹，再从章节目录选择",
                ));
            }
        }
        self.library.protect(&path, false)?;
        self.protect_layout(&path, false)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::open(&path)?;
        self.adopt_project(next)
    }

    pub fn switch_project(&mut self, root: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        let root = fs::canonicalize(root)?;
        if self
            .project
            .as_ref()
            .is_some_and(|store| store.root == root)
        {
            return Ok(());
        }
        self.library.protect(&root, true)?;
        self.protect_layout(&root, true)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::open_project(&root)?;
        self.adopt_project(next)
    }
    pub fn new_project(&mut self, root: impl AsRef<Path>, title: &str) -> io::Result<()> {
        self.require_export_idle()?;
        self.library.protect(root.as_ref(), true)?;
        self.protect_layout(root.as_ref(), true)?;
        self.save()?;
        self.checkpoint(false, true)?;
        let next = Self::create_project(root, title)?;
        self.adopt_project(next)
    }
    pub(crate) fn adopt_project(&mut self, mut next: Self) -> io::Result<()> {
        self.require_export_idle()?;
        next.library = self.library.clone();
        if let Some(store) = &next.project {
            next.library.forgotten.remove(&store.root);
        }
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
    pub(crate) fn remember_location(&mut self) -> io::Result<()> {
        let Some(store) = &self.project else {
            return Ok(());
        };
        if self.library.forgotten.contains(&store.root) {
            return Ok(());
        }
        let mut next = self.library.clone();
        let mut positions = next
            .entries
            .iter()
            .find(|entry| entry.root == store.root)
            .map(|entry| entry.positions.clone())
            .unwrap_or_default();
        positions.extend(
            self.chapters
                .iter()
                .map(|(id, doc)| (id.clone(), doc.cursor)),
        );
        positions.insert(store.active.clone(), self.document.cursor);
        next.entries.retain(|entry| entry.root != store.root);
        next.entries.insert(
            0,
            RecentProject {
                root: store.root.clone(),
                title: store.manifest.title.clone(),
                chapter: store.active.clone(),
                positions,
            },
        );
        next.persist()?;
        self.library = next;
        Ok(())
    }
    fn restore_location(&mut self) -> io::Result<()> {
        if self.external_conflict().is_some() {
            return Ok(());
        }
        let Some(store) = &self.project else {
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
        let Some(store) = &self.project else {
            return;
        };
        if let Some(position) = self
            .library
            .entries
            .iter()
            .find(|entry| entry.root == store.root)
            .and_then(|entry| entry.positions.get(&store.active))
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
