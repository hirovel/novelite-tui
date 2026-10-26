use crate::{Action, Command, Outcome, Workspace};
use std::{io, path::PathBuf, sync::mpsc};

/// Terminal adapter. Operations run serially on a worker, never while drawing.
pub trait Clipboard: Send {
    fn read_text(&mut self) -> io::Result<String>;
    fn write_text(&mut self, text: &str) -> io::Result<()>;
}
#[derive(Default)]
struct SystemClipboard(Option<arboard::Clipboard>);
impl SystemClipboard {
    fn get(&mut self) -> io::Result<&mut arboard::Clipboard> {
        if self.0.is_none() {
            self.0 = Some(arboard::Clipboard::new().map_err(io::Error::other)?);
        }
        Ok(self.0.as_mut().unwrap())
    }
}
impl Clipboard for SystemClipboard {
    fn read_text(&mut self) -> io::Result<String> {
        self.get()?.get_text().map_err(io::Error::other)
    }
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        if text.contains('\0') {
            return Err(io::Error::other(
                "文本含 NUL，系统剪贴板可能截断；请使用内部副本或保存文件",
            ));
        }
        self.get()?.set_text(text).map_err(io::Error::other)
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Copy,
    Cut,
    Paste,
}
struct Pending {
    edit_generation: u64,
    document: u64,
    operation: Operation,
    path: PathBuf,
    revision: u64,
    cursor: usize,
    selection: String,
}
struct Worker {
    request: mpsc::Sender<Option<String>>,
    result: mpsc::Receiver<io::Result<String>>,
}
pub(crate) struct ClipboardState {
    adapter: Option<Box<dyn Clipboard>>,
    worker: Option<Worker>,
    pending: Option<Pending>,
    internal: Option<String>,
    message: Option<String>,
    generation: u64,
    cancelled: bool,
}
impl Default for ClipboardState {
    fn default() -> Self {
        Self {
            adapter: Some(Box::<SystemClipboard>::default()),
            worker: None,
            pending: None,
            internal: None,
            message: None,
            generation: 0,
            cancelled: false,
        }
    }
}
impl ClipboardState {
    fn worker(&mut self) -> io::Result<&Worker> {
        if self.worker.is_none() {
            let mut adapter = self
                .adapter
                .take()
                .unwrap_or_else(|| Box::<SystemClipboard>::default());
            let (request, rx) = mpsc::channel::<Option<String>>();
            let (tx, result) = mpsc::channel();
            std::thread::Builder::new()
                .name("novelite-clipboard".into())
                .spawn(move || {
                    while let Ok(text) = rx.recv() {
                        let value = match text {
                            Some(text) => adapter.write_text(&text).map(|_| text),
                            None => adapter.read_text(),
                        };
                        if tx.send(value).is_err() {
                            break;
                        }
                    }
                })?;
            self.worker = Some(Worker { request, result });
        }
        Ok(self.worker.as_ref().unwrap())
    }
    fn report(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
        self.generation += 1;
    }
}
impl Workspace {
    pub fn set_clipboard(&mut self, adapter: Box<dyn Clipboard>) -> io::Result<()> {
        if self.clipboard_busy() {
            return Err(io::Error::other("剪贴板操作尚未完成"));
        }
        self.clipboard.worker = None;
        self.clipboard.adapter = Some(adapter);
        Ok(())
    }
    pub fn clipboard_busy(&self) -> bool {
        self.clipboard.pending.is_some()
    }
    pub(crate) fn clipboard_blocks_exit(&self) -> bool {
        self.clipboard_busy() && !self.clipboard.cancelled
    }
    pub fn clipboard_message(&self) -> Option<&str> {
        self.clipboard.message.as_deref()
    }
    pub fn clipboard_generation(&self) -> u64 {
        self.clipboard.generation
    }
    fn clipboard_document(&self) -> &crate::document::Document {
        self.prompt
            .as_ref()
            .and_then(|p| p.editor.as_ref())
            .unwrap_or(&self.document)
    }
    fn clipboard_editor(&self) -> io::Result<()> {
        if self.edit_target() == crate::editing::EditTarget::Tool {
            return Err(io::Error::other("请先返回正文再操作文稿剪贴板"));
        }
        Ok(())
    }
    fn system_clipboard(&mut self, operation: Operation) -> io::Result<Outcome> {
        self.sync_edit_focus();
        self.clipboard_editor()?;
        if self.clipboard_busy() {
            return Err(io::Error::other("剪贴板操作尚未完成，可继续编辑"));
        }
        if !matches!(operation, Operation::Copy) {
            self.require_writable()?;
        }
        let selection = self.clipboard_document().selected_text().to_owned();
        if !matches!(operation, Operation::Paste) && selection.is_empty() {
            return Err(io::Error::other("请先选择要复制的正文"));
        }
        let text = if matches!(operation, Operation::Paste) {
            None
        } else {
            self.clipboard.internal = Some(selection.clone());
            Some(selection.clone())
        };
        self.clipboard
            .worker()?
            .request
            .send(text)
            .map_err(io::Error::other)?;
        self.clipboard.pending = Some(Pending {
            edit_generation: self.edit_focus.generation(),
            document: self.clipboard_document().instance,
            operation,
            path: self.path.clone(),
            revision: self.clipboard_document().revision,
            cursor: self.clipboard_document().cursor,
            selection,
        });
        self.clipboard.report("系统剪贴板处理中，可继续编辑");
        Ok(Outcome::Continue)
    }
    pub(crate) fn poll_clipboard(&mut self) -> io::Result<()> {
        self.sync_edit_focus();
        if !self.clipboard_busy() {
            return Ok(());
        }
        let result = match self.clipboard.worker.as_ref().unwrap().result.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => Err(io::Error::other("剪贴板线程已退出")),
        };
        self.dismiss_export_notice();
        let pending = self.clipboard.pending.take().unwrap();
        if self.clipboard.cancelled {
            self.clipboard.cancelled = false;
            self.clipboard.report("已丢弃取消的剪贴板结果，正文未改变");
            return Ok(());
        }
        let unchanged = self.edit_focus.generation() == pending.edit_generation
            && self.clipboard_document().instance == pending.document
            && self.path == pending.path
            && self.clipboard_document().revision == pending.revision
            && self.clipboard_document().cursor == pending.cursor
            && self.clipboard_document().selected_text() == pending.selection
            && self.clipboard_editor().is_ok();
        match (pending.operation, result) {
            (Operation::Paste, Ok(text)) => {
                self.clipboard.internal = Some(text.clone());
                if unchanged {
                    self.dispatch(Action::Insert(text))?;
                    self.clipboard.report("已从系统剪贴板粘贴");
                } else {
                    self.clipboard
                        .report("位置或正文已变化，未自动粘贴；已保留本次内容，可用内部粘贴");
                }
            }
            (Operation::Paste, Err(error)) => self.clipboard.report(format!(
                "系统剪贴板读取失败，未粘贴：{error}；可用终端原生粘贴或显式内部粘贴"
            )),
            (operation, Ok(_)) => {
                if matches!(operation, Operation::Cut) && unchanged {
                    self.dispatch(Action::Insert(String::new()))?;
                    self.clipboard.report("已剪切到系统剪贴板");
                } else if matches!(operation, Operation::Cut) {
                    self.clipboard
                        .report("已复制到系统剪贴板；选区或正文已变化，未删除正文");
                } else {
                    self.clipboard.report("已复制到系统剪贴板");
                }
            }
            (_, Err(error)) => self.clipboard.report(format!(
                "系统剪贴板写入失败，正文保留：{error}；本次选区已留在内部剪贴板"
            )),
        }
        Ok(())
    }
    fn internal_clipboard(&mut self, operation: Operation) -> io::Result<Outcome> {
        self.clipboard_editor()?;
        if !matches!(operation, Operation::Copy) {
            self.require_writable()?;
        }
        if matches!(operation, Operation::Paste) {
            let text = self
                .clipboard
                .internal
                .clone()
                .ok_or_else(|| io::Error::other("内部剪贴板为空"))?;
            self.dispatch(Action::Insert(text))?;
            self.clipboard.report("已粘贴内部副本（未读取系统剪贴板）");
        } else {
            let text = self.clipboard_document().selected_text().to_owned();
            if text.is_empty() {
                return Err(io::Error::other("请先选择正文"));
            }
            self.clipboard.internal = Some(text);
            if matches!(operation, Operation::Cut) {
                self.dispatch(Action::Insert(String::new()))?;
            }
            self.clipboard
                .report("已存入内部剪贴板（未写入系统剪贴板）");
        }
        Ok(Outcome::Continue)
    }
}
pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "cancel-clipboard".into(),
            title: "放弃待处理的剪贴板结果 / 允许退出".into(),
            run: |work| {
                if work.clipboard_busy() {
                    work.clipboard.cancelled = true;
                    work.clipboard
                        .report("已放弃剪贴板结果，可退出；系统操作可能仍在结束");
                }
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "copy".into(),
            title: "复制选区到系统剪贴板".into(),
            run: |w| w.system_clipboard(Operation::Copy),
        },
        Command {
            id: "cut".into(),
            title: "剪切选区到系统剪贴板".into(),
            run: |w| w.system_clipboard(Operation::Cut),
        },
        Command {
            id: "paste".into(),
            title: "从系统剪贴板粘贴".into(),
            run: |w| w.system_clipboard(Operation::Paste),
        },
        Command {
            id: "copy-internal".into(),
            title: "复制到内部剪贴板".into(),
            run: |w| w.internal_clipboard(Operation::Copy),
        },
        Command {
            id: "cut-internal".into(),
            title: "剪切到内部剪贴板".into(),
            run: |w| w.internal_clipboard(Operation::Cut),
        },
        Command {
            id: "paste-internal".into(),
            title: "粘贴内部副本（不读取系统）".into(),
            run: |w| w.internal_clipboard(Operation::Paste),
        },
    ]
}
