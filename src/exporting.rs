use crate::{
    Workspace,
    storage::{WriteMode, atomic_write},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone)]
pub enum ExportFormat {
    ChapterMarkdown,
    ChapterText,
    ProjectDirectory,
    BookText { author: String },
}
#[derive(Default)]
pub(crate) struct Export {
    job: Option<(PathBuf, Receiver<io::Result<()>>)>,
    message: Option<String>,
    generation: u64,
    origin: Option<crate::search::Origin>,
}
impl Workspace {
    /// Snapshot the current manuscript, then publish to a new path in the background.
    /// Completion/errors are delivered by `tick`; this does not save or switch the source.
    pub fn export_manuscript(
        &mut self,
        format: ExportFormat,
        destination: impl AsRef<Path>,
    ) -> io::Result<()> {
        let result = self.start_export(format, destination.as_ref());
        if let Err(e) = &result
            && !self.export_busy()
        {
            self.exporting.message = Some(format!("导出失败：{e}"));
            self.exporting.generation = self.exporting.generation.wrapping_add(1);
        }
        result
    }
    fn start_export(&mut self, format: ExportFormat, destination: &Path) -> io::Result<()> {
        self.require_export_idle()?;
        if !self.module_enabled(crate::ModuleId::Export) {
            return Err(io::Error::other("导出模块已关闭"));
        }
        if self.import_active()
            || self
                .prompt
                .as_ref()
                .is_some_and(|p| !matches!(p.purpose, crate::prompt::Purpose::ExportDestination(_)))
        {
            return Err(io::Error::other("请先完成或取消当前输入/导入"));
        }
        let destination = crate::import_source::destination(destination)?;
        self.library.protect(&destination, true)?;
        self.protect_layout(&destination, true)?;
        if destination == self.path
            || self
                .project
                .as_ref()
                .is_some_and(|s| destination.starts_with(&s.root))
            || self
                .commands
                .keymap_path()
                .is_some_and(|p| p.starts_with(&destination))
        {
            return Err(io::Error::other("导出目标必须在作品与配置之外"));
        }
        let chapter_title = self
            .active_chapter()
            .map(|c| c.title.clone())
            .unwrap_or_else(|| {
                self.path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let text = self.document.text.clone();
        let book = if matches!(
            format,
            ExportFormat::BookText { .. } | ExportFormat::ProjectDirectory
        ) {
            Some(Book::snapshot(self)?)
        } else {
            None
        };
        let target = destination.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("manuscript-export".into())
            .spawn(move || {
                let result = (|| {
                    crate::import_source::reject_link_chain(target.parent().unwrap())?;
                    let output = match format {
                        ExportFormat::ProjectDirectory => return book.unwrap().directory(&target),
                        ExportFormat::ChapterMarkdown => text,
                        ExportFormat::ChapterText => chapter_text(&chapter_title, &text),
                        ExportFormat::BookText { author } => book.unwrap().text(&author)?,
                    };
                    atomic_write(&target, output.as_bytes(), WriteMode::CreateNew)
                })();
                let _ = tx.send(result);
            })?;
        self.exporting.message = Some(format!("正在导出：{}", destination.display()));
        self.exporting.job = Some((destination, rx));
        self.exporting.generation = self.exporting.generation.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn finish_export_prompt(&mut self) {
        if self.prompt.is_none()
            && let Some(origin) = self.exporting.origin.take()
        {
            origin.restore(self);
        }
    }
    pub(crate) fn dismiss_export_notice(&mut self) {
        if !self.export_busy() && self.exporting.message.take().is_some() {
            self.exporting.generation = self.exporting.generation.wrapping_add(1);
        }
    }
    pub fn export_busy(&self) -> bool {
        self.exporting.job.is_some()
    }
    pub fn export_message(&self) -> Option<&str> {
        self.exporting.message.as_deref()
    }
    pub fn export_generation(&self) -> u64 {
        self.exporting.generation
    }
    pub(crate) fn require_export_idle(&self) -> io::Result<()> {
        if self.export_busy() {
            Err(io::Error::other("导出尚未完成，请稍候"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn poll_export(&mut self) -> io::Result<()> {
        let Some((destination, rx)) = self.exporting.job.take() else {
            return Ok(());
        };
        let result = match rx.try_recv() {
            Err(TryRecvError::Empty) => {
                self.exporting.job = Some((destination, rx));
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("导出任务中断，请检查目标路径"))
            }
            Ok(result) => result,
        };
        self.exporting.generation = self.exporting.generation.wrapping_add(1);
        self.exporting.message = Some(match &result {
            Ok(()) => format!("已导出文稿：{}", destination.display()),
            Err(e) => format!("导出失败：{e} · {}", destination.display()),
        });
        result
    }
}

struct Book {
    root: PathBuf,
    manifest: crate::Project,
    live: std::collections::BTreeMap<String, String>,
}
impl Book {
    fn snapshot(w: &Workspace) -> io::Result<Self> {
        let store = w
            .project
            .as_ref()
            .ok_or_else(|| io::Error::other("全书导出需要打开作品"))?;
        let mut live = store.pending.clone();
        for (id, document) in &w.chapters {
            live.insert(id.clone(), document.text.clone());
        }
        live.insert(store.active.clone(), w.document.text.clone());
        Ok(Self {
            root: store.root.clone(),
            manifest: store.manifest.clone(),
            live,
        })
    }
    fn body(&self, chapter: &crate::Chapter) -> io::Result<String> {
        if let Some(text) = self.live.get(&chapter.id) {
            return Ok(text.clone());
        }
        let path = crate::project::managed_path(&self.root, &chapter.path)?;
        crate::import_source::reject_link_chain(&path)?;
        std::fs::read_to_string(path)
    }
    fn directory(&self, target: &Path) -> io::Result<()> {
        let sources = crate::import_source::capture_project(&self.root)?;
        let mut files: std::collections::BTreeMap<_, _> = sources
            .into_iter()
            .filter(|s| s.name != "novelite.json")
            .map(|s| (s.name, s.bytes))
            .collect();
        for chapter in self.manifest.all_chapters() {
            let bytes = if let Some(text) = self.live.get(&chapter.id) {
                text.as_bytes().to_vec()
            } else {
                files
                    .get(&chapter.path)
                    .ok_or_else(|| io::Error::other("章节文件缺失，导出已取消"))?
                    .clone()
            };
            files.insert(chapter.path.clone(), bytes);
        }
        crate::project_import::publish(target, self.manifest.clone(), files.into_iter().collect())
    }
    fn text(&self, author: &str) -> io::Result<String> {
        let author = if author.is_empty() { "佚名" } else { author };
        let title = if self.manifest.title.is_empty() {
            "未命名小说"
        } else {
            &self.manifest.title
        };
        let mut out =
            format!("《{title}》\n作者：{author}\n\n====================================\n\n");
        for volume in &self.manifest.volumes {
            out.push_str(&format!("\n【{}】\n\n", volume.title));
            for chapter in &volume.chapters {
                out.push('\n');
                out.push_str(&chapter_text(&chapter.title, &self.body(chapter)?));
                out.push_str("\n\n");
            }
        }
        Ok(out)
    }
}
// Mirrors desktop ExportNovelService.formatChapterWysiwyg; no paragraph formatting.
fn chapter_text(title: &str, raw: &str) -> String {
    let trimmed = raw.trim_matches(desktop_space);
    if trimmed.is_empty() {
        return format!("{title}\n");
    }
    let leading = raw.trim_start_matches(desktop_space);
    if leading.starts_with('#') {
        let after = leading
            .trim_start_matches('#')
            .trim_start_matches(desktop_space);
        let (heading, rest) = after.split_once('\n').unwrap_or((after, ""));
        if heading.contains(['\u{2028}', '\u{2029}']) {
            return if trimmed.starts_with(title) {
                raw.into()
            } else {
                format!("{title}\n\n{raw}")
            };
        }
        let heading = heading.trim_matches(desktop_space);
        let heading = if heading.is_empty() { title } else { heading };
        let rest = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix('\n'))
            .unwrap_or(rest);
        return format!("{heading}\n\n{rest}");
    }
    if trimmed.starts_with(title) {
        raw.into()
    } else {
        format!("{title}\n\n{raw}")
    }
}

impl Workspace {
    fn begin_export(&mut self, purpose: crate::prompt::Purpose) -> io::Result<crate::Outcome> {
        self.require_export_idle()?;
        if self.prompt.is_some() {
            return Err(io::Error::other("请先完成或取消当前输入"));
        }
        if matches!(
            purpose,
            crate::prompt::Purpose::ExportAuthor
                | crate::prompt::Purpose::ExportDestination(ExportFormat::ProjectDirectory)
        ) && self.project.is_none()
        {
            return Err(io::Error::other("请先打开作品"));
        }
        self.exporting.origin = Some(crate::search::Origin::take(self));
        self.begin_prompt(purpose)
    }
}
pub(crate) fn commands() -> Vec<crate::Command> {
    use crate::prompt::Purpose;
    vec![
        crate::Command {
            id: "export-chapter-md".into(),
            title: "导出当前章节 Markdown 原始源码（新文件）".into(),
            run: |w| w.begin_export(Purpose::ExportDestination(ExportFormat::ChapterMarkdown)),
        },
        crate::Command {
            id: "export-chapter-txt".into(),
            title: "导出当前章节 TXT（新文件）".into(),
            run: |w| w.begin_export(Purpose::ExportDestination(ExportFormat::ChapterText)),
        },
        crate::Command {
            id: "export-book-txt".into(),
            title: "导出全书 TXT（卷章顺序，填写作者）".into(),
            run: |w| w.begin_export(Purpose::ExportAuthor),
        },
        crate::Command {
            id: "export-project".into(),
            title: "导出可携带作品目录（含大纲、历史及附带文件）".into(),
            run: |w| w.begin_export(Purpose::ExportDestination(ExportFormat::ProjectDirectory)),
        },
    ]
}

fn desktop_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
