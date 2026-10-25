use crate::{
    Document, Workspace,
    project::{ProjectStore, Recycled, identity},
};
use std::{fs, io};

impl Workspace {
    pub(crate) fn change_structure<T>(
        &mut self,
        change: impl FnOnce(&mut ProjectStore) -> io::Result<T>,
    ) -> io::Result<T> {
        self.require_writable()?;
        self.save()?;
        let mut store = self
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("请先打开作品"))?
            .clone();
        let result = change(&mut store)?;
        store.changed();
        if store.chapter(&store.active).is_none() {
            let next = store
                .manifest
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .next()
                .ok_or_else(|| io::Error::other("作品至少需要一章"))?
                .id
                .clone();
            let document = if let Some(text) = store.pending.get(&next) {
                Document::new(text.clone(), true)
            } else {
                Document::new(fs::read_to_string(store.chapter_path(&next)?)?, false)
            };
            let document = self.chapters.remove(&next).unwrap_or(document);
            self.chapters.insert(
                store.active.clone(),
                std::mem::replace(&mut self.document, document),
            );
            store.active = next;
            self.path = store.path();
        }
        self.project = Some(store);
        self.reconcile_panes();
        self.clear_history_view();
        self.saving.edited();
        self.save()?;
        Ok(result)
    }
    pub fn recycle_chapter(&mut self, id: &str) -> io::Result<String> {
        self.change_structure(|store| {
            let volume = store
                .manifest
                .volumes
                .iter_mut()
                .find(|v| v.chapters.iter().any(|c| c.id == id))
                .ok_or_else(|| io::Error::other("章节不存在"))?;
            let at = volume.chapters.iter().position(|c| c.id == id).unwrap();
            let chapter = volume.chapters.remove(at);
            store.recycle(format!("章节「{}」", chapter.title), vec![chapter])
        })
    }
    pub fn recycle_volume(&mut self, id: &str) -> io::Result<String> {
        self.change_structure(|store| {
            let at = store
                .manifest
                .volumes
                .iter()
                .position(|v| v.id == id)
                .ok_or_else(|| io::Error::other("卷不存在"))?;
            let volume = store.manifest.volumes.remove(at);
            store.recycle(format!("卷「{}」", volume.title), volume.chapters)
        })
    }
    pub fn restore_recycled(&mut self, id: &str, volume: &str, position: usize) -> io::Result<()> {
        self.change_structure(|store| {
            let at = store
                .manifest
                .recycle_bin
                .iter()
                .position(|r| r.id == id)
                .ok_or_else(|| io::Error::other("回收记录不存在"))?;
            for chapter in &store.manifest.recycle_bin[at].chapters {
                fs::read_to_string(crate::project::managed_path(&store.root, &chapter.path)?)?;
            }
            let volume = store
                .manifest
                .volumes
                .iter_mut()
                .find(|v| v.id == volume)
                .ok_or_else(|| io::Error::other("目标卷不存在"))?;
            let item = store.manifest.recycle_bin.remove(at);
            let at = position.min(volume.chapters.len());
            volume.chapters.splice(at..at, item.chapters);
            Ok(())
        })
    }
    pub fn move_chapter(&mut self, id: &str, volume: &str, position: usize) -> io::Result<()> {
        self.change_structure(|store| store.move_chapter(id, volume, position))
    }
    pub fn move_volume(&mut self, id: &str, position: usize) -> io::Result<()> {
        self.change_structure(|store| store.move_volume(id, position))
    }
    pub fn copy_chapter(&mut self, id: &str, volume: &str, title: &str) -> io::Result<String> {
        crate::navigation::check_title(title)?;
        self.change_structure(|store| {
            let text = fs::read_to_string(store.chapter_path(id)?)?;
            let plan = store
                .chapter(id)
                .ok_or_else(|| io::Error::other("章节不存在"))?
                .plan
                .clone();
            let copy = store.create_chapter(volume, title)?;
            store
                .manifest
                .volumes
                .iter_mut()
                .flat_map(|v| &mut v.chapters)
                .find(|c| c.id == copy)
                .unwrap()
                .plan = plan;
            store.pending.insert(copy.clone(), text);
            Ok(copy)
        })
    }
    pub fn number_chapters(&mut self) -> io::Result<()> {
        self.change_structure(|store| {
            for (index, chapter) in store
                .manifest
                .volumes
                .iter_mut()
                .flat_map(|v| &mut v.chapters)
                .enumerate()
            {
                let title = without_number(&chapter.title);
                chapter.title = if title.is_empty() {
                    format!("第{}章", index + 1)
                } else {
                    format!("第{}章 {title}", index + 1)
                };
            }
            Ok(())
        })
    }
}
impl ProjectStore {
    fn recycle(&mut self, title: String, chapters: Vec<crate::Chapter>) -> io::Result<String> {
        let id = identity()?;
        self.manifest.recycle_bin.push(Recycled {
            id: id.clone(),
            title,
            chapters,
        });
        self.manifest.version = 2;
        // Keep a writable landing chapter when the entire manuscript was recycled.
        if self.manifest.volumes.iter().all(|v| v.chapters.is_empty()) {
            let volume = if let Some(v) = self.manifest.volumes.first() {
                v.id.clone()
            } else {
                self.create_volume("第一卷")?
            };
            self.create_chapter(&volume, "第一章")?;
        }
        Ok(id)
    }
    fn move_chapter(&mut self, id: &str, target: &str, position: usize) -> io::Result<()> {
        let (from, index) = self
            .manifest
            .volumes
            .iter()
            .enumerate()
            .find_map(|(v, volume)| {
                volume
                    .chapters
                    .iter()
                    .position(|c| c.id == id)
                    .map(|c| (v, c))
            })
            .ok_or_else(|| io::Error::other("章节不存在"))?;
        let to = self
            .manifest
            .volumes
            .iter()
            .position(|v| v.id == target)
            .ok_or_else(|| io::Error::other("目标卷不存在"))?;
        let chapter = self.manifest.volumes[from].chapters.remove(index);
        let chapters = &mut self.manifest.volumes[to].chapters;
        chapters.insert(position.min(chapters.len()), chapter);
        Ok(())
    }
    fn move_volume(&mut self, id: &str, position: usize) -> io::Result<()> {
        let from = self
            .manifest
            .volumes
            .iter()
            .position(|v| v.id == id)
            .ok_or_else(|| io::Error::other("卷不存在"))?;
        let volume = self.manifest.volumes.remove(from);
        let at = position.min(self.manifest.volumes.len());
        self.manifest.volumes.insert(at, volume);
        Ok(())
    }
}
fn without_number(title: &str) -> &str {
    if let Some(rest) = title.strip_prefix('第')
        && let Some((number, rest)) = rest.split_once('章')
        && !number.is_empty()
        && number
            .chars()
            .all(|c| c.is_ascii_digit() || "零〇一二三四五六七八九十百千万两".contains(c))
    {
        rest.trim_start()
    } else {
        title
    }
}
