use crossterm::event::{Event, KeyCode, KeyEventKind};

#[derive(Clone)]
pub(crate) enum Purpose {
    ExportAuthor,
    ExportDestination(crate::ExportFormat),
    ImportSource,
    ImportEncoding(std::path::PathBuf),
    ImportDestination,
    Format(crate::typography::FormatRequest),
    Outline(crate::planning::PlanEdit),
    ChapterGoal(crate::planning::PlanEdit),
    BookGoal {
        root: std::path::PathBuf,
        original: Option<u64>,
    },
    Find,
    SearchBook,
    Replace {
        replacement: crate::search::Replacement,
        all: bool,
    },
    ConflictCopy,
    ConflictOverwrite,
    HistoryPolicy,
    CopyChapter {
        chapter: String,
        volume: String,
    },
    SaveAs,
    RenameProject,
    OpenProject,
    NewProject,
    NewVolume,
    NewChapter,
    RenameVolume,
    RenameChapter,
}
pub struct Prompt {
    pub(crate) editor: Option<crate::document::Document>,
    pub(crate) editor_area: ratatui::layout::Rect,
    pub(crate) dragging: bool,
    pub title: String,
    pub text: String,
    pub(crate) purpose: Purpose,
    line: crate::line_editor::LineEditor,
    completion: crate::user_path::Completion,
}
pub(crate) enum Response {
    Editing,
    Cancel,
    Submit,
}
impl Prompt {
    pub(crate) fn new(purpose: Purpose) -> Self {
        Self {
            editor: None,
            editor_area: Default::default(),
            dragging: false,
            title: match &purpose {
                Purpose::ExportAuthor => "全书 TXT 作者 · 留空使用佚名（仅用于本次导出）",
                Purpose::ExportDestination(format) => match format {
                    crate::ExportFormat::ChapterMarkdown => {
                        "导出本章 Markdown 原文 · 输入新文件路径"
                    }
                    crate::ExportFormat::ChapterText => "导出本章 TXT · 输入新文件路径",
                    crate::ExportFormat::BookText { .. } => "导出全书 TXT · 输入新文件路径",
                    crate::ExportFormat::ProjectDirectory => {
                        "导出作品（大纲/历史/附带文件）· 输入新目录路径"
                    }
                },
                Purpose::ImportSource => "导入来源 · TXT/Markdown 文件或目录路径",
                Purpose::ImportEncoding(_) => {
                    "确认来源编码 · utf8 / utf16le / utf16be / gb18030 · Esc 取消"
                }
                Purpose::ImportDestination => "确认导入 · 输入不存在的新作品目录 · 不覆盖来源",
                Purpose::Format(_) => "整章排版 · Enter 确认 / Esc 取消",
                Purpose::Outline(_) => "本章大纲 · Enter 换行 · 保存键确认 · Esc 取消",
                Purpose::ChapterGoal(_) => "本章目标 · 输入字数，0 清除",
                Purpose::BookGoal { .. } => "全书目标 · 输入字数，0 清除",
                Purpose::Find => "查找当前章节 · 输入文字 · Enter 查询 · Esc 返回正文",
                Purpose::SearchBook => {
                    "全书搜索 · 标题/正文 · 区分大小写的文字匹配 · Enter 查询 · Esc 取消"
                }
                Purpose::Replace { all: true, .. } => {
                    "替换当前章节全部匹配 · 一次撤销 · 空文本表示删除 · Esc 取消"
                }
                Purpose::Replace { all: false, .. } => {
                    "替换当前选中匹配 · 空文本表示删除 · Esc 取消"
                }
                Purpose::ConflictCopy => "另存本地稿 · 输入作品外的新文件路径",
                Purpose::ConflictOverwrite => "备份双方后覆盖磁盘 · 输入「覆盖」确认 · Esc 取消",
                Purpose::HistoryPolicy => {
                    "历史策略 · 输入 近期点数,保留天数,每章容量MiB（例如 50,7,256）"
                }
                Purpose::CopyChapter { .. } => "复制章节 · 输入副本标题",
                Purpose::SaveAs => "另存 · 输入新的文件或作品文件夹路径",
                Purpose::RenameProject => "作品改名 · 输入新书名",
                Purpose::OpenProject => "打开 · 输入 TXT/Markdown 文件或作品文件夹路径",
                Purpose::NewProject => "新建作品 · 输入新的文件夹路径",
                Purpose::NewVolume => "新建卷 · 输入标题",
                Purpose::NewChapter => "新建章节 · 输入标题（当前选中卷）",
                Purpose::RenameVolume => "修改卷标题 · 输入新标题",
                Purpose::RenameChapter => "章节改名 · 正文不替换",
            }
            .into(),
            text: String::new(),
            line: Default::default(),
            completion: Default::default(),
            purpose,
        }
    }
    pub(crate) fn event(&mut self, event: Event) -> Response {
        if let Event::Key(key) = &event
            && key.kind != KeyEventKind::Release
        {
            match key.code {
                KeyCode::Esc => return Response::Cancel,
                KeyCode::Enter => return Response::Submit,
                KeyCode::Tab
                    if matches!(
                        self.purpose,
                        Purpose::OpenProject | Purpose::NewProject | Purpose::SaveAs
                    ) =>
                {
                    self.completion.apply(&mut self.text);
                    self.line = Default::default();
                    return Response::Editing;
                }
                _ => {}
            }
        }
        if self.line.event(&mut self.text, &event) {
            self.completion.reset();
        }
        Response::Editing
    }
    pub(crate) fn hint(&self) -> &str {
        if !self.completion.hint.is_empty() {
            &self.completion.hint
        } else if matches!(
            self.purpose,
            Purpose::OpenProject | Purpose::NewProject | Purpose::SaveAs
        ) {
            "Tab 补全路径 · Enter 确认 · Esc 取消"
        } else {
            "Enter 确认 · Esc 取消"
        }
    }
    pub(crate) fn line_view(&mut self, width: u16) -> (String, u16) {
        self.line.view(&self.text, width)
    }
}
