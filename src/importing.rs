use crate::Workspace;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Gb18030,
}

impl ImportEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Gb18030 => "GB18030/GBK",
        }
    }
    fn decode(self, bytes: &[u8]) -> io::Result<String> {
        let error = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "编码不匹配或含无效字符；请确认 UTF-8 / UTF-16LE / UTF-16BE / GB18030 后重新预览",
            )
        };
        let text = match self {
            Self::Utf8 => {
                std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
                    .map_err(|_| error())?
                    .to_owned()
            }
            Self::Utf16Le | Self::Utf16Be => {
                let (bom, codec) = if self == Self::Utf16Le {
                    ([0xff, 0xfe], encoding_rs::UTF_16LE)
                } else {
                    ([0xfe, 0xff], encoding_rs::UTF_16BE)
                };
                if bytes.starts_with(&[bom[1], bom[0]]) {
                    return Err(error());
                }
                codec
                    .decode_without_bom_handling_and_without_replacement(
                        bytes.strip_prefix(&bom).unwrap_or(bytes),
                    )
                    .ok_or_else(error)?
                    .into_owned()
            }
            Self::Gb18030 => encoding_rs::GB18030
                .decode_without_bom_handling_and_without_replacement(bytes)
                .ok_or_else(error)?
                .into_owned(),
        };
        if text.contains('\0') {
            return Err(error());
        }
        Ok(text)
    }
}

pub struct ImportChapter {
    pub volume: String,
    pub title: String,
    pub text: String,
}
pub struct ImportPreview {
    pub title: String,
    pub source: PathBuf,
    pub encoding: ImportEncoding,
    pub chapters: Vec<ImportChapter>,
}
#[derive(PartialEq, Eq)]
pub(crate) struct Source {
    pub name: String,
    pub bytes: Vec<u8>,
}
pub(crate) struct ImportPlan {
    pub preview: ImportPreview,
    pub sources: Vec<Source>,
}
impl Workspace {
    pub fn import_preview(&self) -> Option<&ImportPreview> {
        self.importing.as_ref().map(|p| &p.preview)
    }
    /// Start a preview scan. Completion and deferred errors arrive through `tick`.
    pub fn prepare_import(
        &mut self,
        source: impl AsRef<Path>,
        encoding: ImportEncoding,
    ) -> io::Result<()> {
        if !self.module_enabled(crate::ModuleId::Import) {
            return Err(io::Error::other("导入模块已停用"));
        }
        if self.prompt.as_ref().is_some_and(|p| {
            !matches!(
                p.purpose,
                crate::prompt::Purpose::ImportSource | crate::prompt::Purpose::ImportEncoding(_)
            )
        }) {
            return Err(io::Error::other("请先完成或取消当前输入"));
        }
        self.require_import_idle()?;
        let source = source.as_ref().to_owned();
        let worker_source = source.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("import-preview".into())
            .spawn(move || {
                let _ = tx.send(ImportPlan::read(&worker_source, encoding).map(Arc::new));
            })?;
        self.import_job = Some(ImportJob::Preview {
            source,
            encoding,
            result: rx,
            cancelled: false,
        });
        self.import_revision = self.import_revision.wrapping_add(1);
        Ok(())
    }
    /// Save the current draft, then start publication; wait for `import_busy` to clear.
    pub fn confirm_import(&mut self, destination: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        self.require_import_idle()?;
        if self
            .prompt
            .as_ref()
            .is_some_and(|p| !matches!(p.purpose, crate::prompt::Purpose::ImportDestination))
        {
            return Err(io::Error::other("请先完成或取消当前输入"));
        }
        let destination = crate::import_source::destination(destination.as_ref())?;
        self.library.protect(&destination, true)?;
        self.protect_layout(&destination, true)?;
        if self
            .project
            .as_ref()
            .is_some_and(|p| destination.starts_with(&p.root))
            || self
                .commands
                .keymap_path()
                .is_some_and(|p| p.starts_with(&destination))
        {
            return Err(io::Error::other("新作品不能嵌入当前作品或包含配置文件"));
        }
        if self.importing.is_none() {
            return Err(io::Error::other("请先预览导入"));
        }
        let plan = self.importing.as_ref().unwrap();
        if destination.starts_with(&plan.preview.source) {
            return Err(io::Error::other("导入目标必须位于来源目录之外"));
        }
        self.save()?;
        self.checkpoint(false, true)?;
        let plan = Arc::clone(self.importing.as_ref().unwrap());
        let target = destination.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("import-publish".into())
            .spawn(move || {
                let result = (|| {
                    crate::import_source::reject_link_chain(&plan.preview.source)?;
                    if crate::import_source::capture(&plan.preview.source)? != plan.sources {
                        return Err(io::Error::other("来源已变化，请重新预览后再确认"));
                    }
                    crate::import_source::reject_link_chain(target.parent().unwrap())?;
                    crate::project_import::create(&target, &plan)
                })();
                let _ = tx.send(result);
            })?;
        self.import_job = Some(ImportJob::Publish {
            destination,
            result: rx,
        });
        self.import_revision = self.import_revision.wrapping_add(1);
        Ok(())
    }
    /// Work is bounded to one background job. Drive completion and errors through `tick`.
    pub fn import_busy(&self) -> bool {
        self.import_job.is_some()
    }
    pub fn import_generation(&self) -> u64 {
        self.import_revision
    }
    pub(crate) fn import_active(&self) -> bool {
        self.importing.is_some()
            || matches!(
                self.import_job,
                Some(
                    ImportJob::Preview {
                        cancelled: false,
                        ..
                    } | ImportJob::Publish { .. }
                )
            )
    }
    pub(crate) fn import_publishing(&self) -> bool {
        matches!(self.import_job, Some(ImportJob::Publish { .. }))
    }
    pub(crate) fn require_import_idle(&self) -> io::Result<()> {
        if self.import_busy() {
            Err(io::Error::other("上一项导入仍在处理，请稍候"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn poll_import(&mut self) -> io::Result<()> {
        let Some(job) = self.import_job.take() else {
            return Ok(());
        };
        let result = match &job {
            ImportJob::Preview { result, .. } => result.try_recv().map(|r| r.map(Some)),
            ImportJob::Publish { result, .. } => result.try_recv().map(|r| r.map(|()| None)),
        };
        let result = match result {
            Err(TryRecvError::Empty) => {
                self.import_job = Some(job);
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("导入任务中断，请检查目标目录并重试"))
            }
            Ok(result) => result,
        };
        self.import_revision = self.import_revision.wrapping_add(1);
        match job {
            ImportJob::Preview {
                cancelled: true, ..
            } => Ok(()),
            ImportJob::Preview {
                source, encoding, ..
            } => match result {
                Ok(Some(plan)) => {
                    self.importing = Some(plan);
                    self.prompt = None;
                    Ok(())
                }
                Err(e)
                    if e.kind() == io::ErrorKind::InvalidData
                        && encoding == ImportEncoding::Utf8
                        && self.import_ui.origin.is_some() =>
                {
                    self.prompt = Some(crate::prompt::Prompt::new(
                        crate::prompt::Purpose::ImportEncoding(source),
                    ));
                    Ok(())
                }
                Err(e) => {
                    if self.import_ui.origin.is_some() {
                        let purpose = if e.kind() == io::ErrorKind::InvalidData {
                            crate::prompt::Purpose::ImportEncoding(source)
                        } else {
                            crate::prompt::Purpose::ImportSource
                        };
                        self.prompt = Some(crate::prompt::Prompt::new(purpose));
                    }
                    Err(e)
                }
                _ => unreachable!(),
            },
            ImportJob::Publish { destination, .. } => {
                result?;
                self.cancel_import();
                Self::open_project(&destination)
                    .and_then(|next| self.adopt_project(next))
                    .map_err(|e| {
                        io::Error::other(format!(
                            "导入作品已完整创建于 {}，打开失败：{e}；可用打开作品重试",
                            destination.display()
                        ))
                    })
            }
        }
    }
}
pub(crate) enum ImportJob {
    Preview {
        source: PathBuf,
        encoding: ImportEncoding,
        result: Receiver<io::Result<Arc<ImportPlan>>>,
        cancelled: bool,
    },
    Publish {
        destination: PathBuf,
        result: Receiver<io::Result<()>>,
    },
}
impl ImportPlan {
    fn read(source: &Path, encoding: ImportEncoding) -> io::Result<Self> {
        crate::import_source::reject_link_chain(source)?;
        let source = fs::canonicalize(source)?;
        let sources = crate::import_source::capture(&source)?;
        let title = if source.is_dir() {
            source.file_name()
        } else {
            source.file_stem()
        }
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
        let mut chapters = Vec::new();
        for file in &sources {
            let path = Path::new(&file.name);
            if !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let text = encoding
                .decode(&file.bytes)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", file.name)))?;
            let fallback = path.file_stem().unwrap().to_string_lossy();
            let volume = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| "正文".into(), |p| p.to_string_lossy().into_owned());
            chapters.extend(split_text(&text, &fallback, &volume));
        }
        if chapters.is_empty() {
            return Err(io::Error::other("来源中没有 TXT/Markdown 正文"));
        }
        Ok(ImportPlan {
            preview: ImportPreview {
                title,
                source,
                encoding,
                chapters,
            },
            sources,
        })
    }
}

fn split_text(text: &str, fallback: &str, default_volume: &str) -> Vec<ImportChapter> {
    let mut result = Vec::new();
    let (mut start, mut offset) = (0, 0);
    let mut title = fallback.to_owned();
    let mut volume = default_volume.to_owned();
    let mut fence = crate::markdown::Fence::default();
    for line in text.split_inclusive('\n') {
        let stripped = line.trim();
        let protected = fence.protects(stripped);
        let stripped = stripped.trim_start_matches('#').trim();
        let markdown = line.trim_start().starts_with('#')
            && line
                .trim_start()
                .trim_start_matches('#')
                .starts_with(char::is_whitespace);
        let volume_marker = (stripped.starts_with('【') && stripped.ends_with('】'))
            || (stripped.starts_with('[') && stripped.ends_with(']'));
        let kind = if protected {
            None
        } else {
            heading(stripped).or({
                if volume_marker {
                    Some(true)
                } else if markdown {
                    Some(false)
                } else {
                    None
                }
            })
        };
        if let Some(is_volume) = kind {
            if offset > start {
                result.push(ImportChapter {
                    volume: volume.clone(),
                    title: title.clone(),
                    text: text[start..offset].into(),
                });
            }
            if is_volume {
                volume = stripped.into();
                title = "卷首".into();
            } else {
                title = stripped.into();
            }
            start = offset;
        }
        offset += line.len();
    }
    if start < text.len() || result.is_empty() {
        result.push(ImportChapter {
            volume,
            title,
            text: text[start..].into(),
        });
    }
    result
}
fn heading(line: &str) -> Option<bool> {
    let rest = line.strip_prefix('第')?;
    let after = rest.trim_start_matches(|c: char| {
        c.is_ascii_digit() || "零〇一二三四五六七八九十百千万两".contains(c)
    });
    if after.len() == rest.len() {
        return None;
    }
    if after.starts_with('卷') {
        Some(true)
    } else if after.starts_with(['章', '节', '回']) {
        Some(false)
    } else {
        None
    }
}
