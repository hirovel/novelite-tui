use crate::{Chapter, Workspace};
use std::io;

impl Workspace {
    pub fn active_chapter(&self) -> Option<&Chapter> {
        let store = self.project.as_ref()?;
        store.chapter(&store.active)
    }
    pub fn create_volume(&mut self, title: &str) -> io::Result<String> {
        self.require_writable()?;
        check_title(title)?;
        self.save()?;
        let id = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?
            .create_volume(title)?;
        self.saving.edited();
        self.save()?;
        Ok(id)
    }
    pub fn create_chapter(&mut self, volume: &str, title: &str) -> io::Result<String> {
        self.require_writable()?;
        check_title(title)?;
        self.save()?;
        let id = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?
            .create_chapter(volume, title)?;
        self.saving.edited();
        self.save()?;
        Ok(id)
    }
    pub fn rename_chapter(&mut self, id: &str, title: &str) -> io::Result<()> {
        self.require_writable()?;
        check_title(title)?;
        let store = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        let chapter = store
            .manifest
            .volumes
            .iter_mut()
            .flat_map(|v| &mut v.chapters)
            .find(|c| c.id == id)
            .ok_or_else(|| io::Error::other("章节不存在"))?;
        chapter.title = title.into();
        store.changed();
        self.saving.edited();
        Ok(())
    }
    pub fn rename_volume(&mut self, id: &str, title: &str) -> io::Result<()> {
        self.require_writable()?;
        check_title(title)?;
        let store = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        let volume = store
            .manifest
            .volumes
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or_else(|| io::Error::other("卷不存在"))?;
        volume.title = title.into();
        store.changed();
        self.saving.edited();
        Ok(())
    }
    pub fn switch_chapter(&mut self, id: &str) -> io::Result<()> {
        self.save()?;
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        if store.active == id {
            return Ok(());
        }
        let path = store.chapter_path(id)?;
        let cached = self.chapters.contains_key(id);
        let next = if cached {
            None
        } else {
            let opened = Self::open(&path)?;
            if opened.document.new_file {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "章节正文缺失；保留当前文稿",
                ));
            }
            Some(opened.document)
        };
        self.save()?;
        self.checkpoint(false, true)?;
        let next = self.chapters.remove(id).or(next).unwrap();
        let previous = std::mem::replace(&mut self.document, next);
        let store = self.project.as_mut().unwrap();
        self.chapters.insert(store.active.clone(), previous);
        store.active = id.into();
        self.path = path;
        self.saving = crate::saving::Saving::default();
        self.clear_history_view();
        if !cached {
            self.restore_cursor();
        }
        self.check_external()
    }
    pub fn next_chapter(&mut self) -> io::Result<()> {
        self.adjacent_chapter(true)
    }
    pub fn previous_chapter(&mut self) -> io::Result<()> {
        self.adjacent_chapter(false)
    }
    fn adjacent_chapter(&mut self, forward: bool) -> io::Result<()> {
        let store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("请先打开作品"))?;
        let chapters: Vec<_> = store
            .manifest
            .volumes
            .iter()
            .flat_map(|v| &v.chapters)
            .collect();
        let index = chapters.iter().position(|c| c.id == store.active).unwrap();
        let next = if forward {
            (index + 1).min(chapters.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        let id = chapters[next].id.clone();
        self.switch_chapter(&id)
    }
}
pub(crate) fn check_title(title: &str) -> io::Result<()> {
    if title.trim().is_empty() || title.chars().any(char::is_control) {
        Err(io::Error::other("标题不能为空或包含控制字符"))
    } else {
        Ok(())
    }
}
