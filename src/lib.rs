use std::{
    fs, io,
    path::{Path, PathBuf},
};
mod appearance;
mod book_search;
mod terminal_colors;
pub use terminal_colors::detect_color_support;
mod design_motion;
mod design_settings;
pub use appearance::{
    Background, Brightness, ColorMode, ColorTheme, Density, Material, TextWidth, VisualStyle,
};
mod clipboard;
mod render;
pub use book_search::{BookChapter, BookMatch, BookSearch, SearchReader};
mod commands;
pub use clipboard::Clipboard;
mod exporting;
mod external;
mod fingerprint;
mod immersion;
mod import_source;
mod import_ui;
mod importing;
pub use exporting::ExportFormat;
mod heading;
mod markdown;
mod project_import;
pub use importing::{ImportChapter, ImportEncoding, ImportPreview};
mod input;
mod modules;
mod mouse;
pub use modules::{ModuleId, ModulePanel};
mod workspace_layout;
pub use workspace_layout::{PanelPlacement, PanelPreferences, PanelSide, WorkspaceLayout};
mod panes;
pub use external::ExternalConflict;
pub use panes::PaneView;
mod keymap;
mod library;
mod navigation;
mod navigator;
mod recent_ui;
pub use library::RecentProject;
pub use navigator::{Navigation, NavigationEntry};
mod pairing;
mod planning;
mod project;
mod statistics;
mod typography;
mod word_count;
pub use planning::{ChapterPlan, WordProgress};
pub use statistics::WordCountReader;
mod line_editor;
mod prompt;
mod saving;
mod search;
pub use search::ChapterSearch;
mod storage;
mod structure;
mod structure_ui;
pub use commands::{Command, CommandEntry, Palette};
pub use project::{Chapter, Project, Recycled, Volume};
pub use prompt::Prompt;
pub use saving::SaveState;
mod document;
mod history;
mod history_ui;
pub use history::{HistoryPolicy, Snapshot};
mod view;
use document::Document;
pub use view::View;

pub enum Action {
    Insert(String),
    Newline,
    Move(Direction),
    Select(Direction),
    SelectAll,
    Undo,
    Redo,
    Backspace,
    Delete,
    Save,
    Quit,
}
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Exit,
}

pub struct Workspace {
    design_ui: design_settings::DesignUi,
    design_motion: design_motion::Motion,
    design_effect_regions: Vec<ratatui::layout::Rect>,
    path: PathBuf,
    document: Document,
    chapters: std::collections::HashMap<String, Document>,
    commands: commands::Commands,
    modules: modules::Modules,
    zen_origin: Option<search::Origin>,
    importing: Option<std::sync::Arc<importing::ImportPlan>>,
    import_job: Option<importing::ImportJob>,
    import_revision: u64,
    exporting: exporting::Export,
    import_ui: import_ui::ImportUi,
    enhanced_keyboard: bool,
    color_support: ColorMode,
    project: Option<project::ProjectStore>,
    saving: saving::Saving,
    prompt: Option<Prompt>,
    library: library::Library,
    navigator: Option<navigator::Navigator>,
    recent_ui: recent_ui::RecentUi,
    selected_volume: Option<String>,
    history: history::History,
    history_preview: Option<history_ui::Preview>,
    external: external::External,
    mouse: mouse::Mouse,
    clipboard: clipboard::ClipboardState,
    panes: panes::Panes,
    search: Option<search::Search>,
    book_searching: book_search::BookSearching,
    statistics: statistics::Statistics,
}

impl Workspace {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let input = std::path::absolute(path)?;
        let new_file = !input.try_exists()?;
        let path = if new_file {
            fs::canonicalize(
                input
                    .parent()
                    .ok_or_else(|| io::Error::other("Missing parent directory"))?,
            )?
            .join(
                input
                    .file_name()
                    .ok_or_else(|| io::Error::other("Missing filename"))?,
            )
        } else {
            fs::canonicalize(input)?
        };
        let text = if new_file {
            String::new()
        } else {
            fs::read_to_string(&path)?
        };
        Ok(Self::from_document(path, Document::new(text, new_file)))
    }
    fn from_document(path: PathBuf, document: Document) -> Self {
        Self {
            path,
            design_ui: Default::default(),
            design_motion: Default::default(),
            design_effect_regions: Vec::new(),
            document,
            chapters: std::collections::HashMap::new(),
            commands: commands::Commands::default(),
            modules: modules::Modules::default(),
            zen_origin: None,
            importing: None,
            import_job: None,
            import_revision: 0,
            exporting: Default::default(),
            import_ui: Default::default(),
            history: history::History::default(),
            history_preview: None,
            external: external::External::default(),
            mouse: mouse::Mouse::default(),
            clipboard: clipboard::ClipboardState::default(),
            panes: panes::Panes::default(),
            search: None,
            book_searching: book_search::BookSearching::default(),
            statistics: statistics::Statistics::default(),
            enhanced_keyboard: false,
            color_support: ColorMode::Basic,
            project: None,
            saving: saving::Saving::default(),
            prompt: None,
            library: library::Library::default(),
            navigator: None,
            recent_ui: Default::default(),
            selected_volume: None,
        }
    }

    pub fn create_project(root: impl AsRef<Path>, title: &str) -> io::Result<Self> {
        let store = project::ProjectStore::create(root.as_ref(), title)?;
        let mut work = Self::open(store.path())?;
        work.project = Some(store);
        work.observe_manifest()?;
        Ok(work)
    }
    pub fn open_project(root: impl AsRef<Path>) -> io::Result<Self> {
        let mut store = project::ProjectStore::open(root.as_ref())?;
        let recovering = store.recovery_text.is_some();
        let mut work = if let Some(text) = store.recovery_text.take() {
            Self::from_document(store.path(), Document::new(text, false))
        } else {
            Self::open(store.path())?
        };
        work.project = Some(store);
        work.observe_manifest()?;
        if recovering {
            work.saving.uncertain = true;
            work.saved_conflict();
        }
        Ok(work)
    }
    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref().map(|store| &store.manifest)
    }
    pub fn rename_project(&mut self, title: &str) -> io::Result<()> {
        self.require_writable()?;
        let store = self
            .project
            .as_mut()
            .ok_or_else(|| io::Error::other("No project open"))?;
        if store.manifest.title != title {
            store.manifest.title = title.into();
            store.dirty = true;
            store.revision += 1;
            self.saving.edited();
        }
        Ok(())
    }

    pub fn dispatch(&mut self, action: Action) -> io::Result<Outcome> {
        self.dismiss_export_notice();
        if self.import_publishing()
            || (self.import_active() && !matches!(action, Action::Save | Action::Quit))
        {
            return Err(io::Error::other("请先完成或取消导入"));
        }
        if matches!(action, Action::Save | Action::Quit) {
            self.commit_outline()?;
        } else if let Some(editor) = self.prompt.as_mut().and_then(|p| p.editor.as_mut()) {
            editor.apply(action);
            return Ok(Outcome::Continue);
        }
        let undo = matches!(action, Action::Undo);
        let redo = matches!(action, Action::Redo);
        if matches!(
            action,
            Action::Insert(_)
                | Action::Newline
                | Action::Backspace
                | Action::Delete
                | Action::Undo
                | Action::Redo
        ) {
            self.require_writable()?;
        }
        match action {
            Action::Save => self.save()?,
            Action::Quit => {
                self.require_export_idle()?;
                if self.clipboard_blocks_exit() {
                    return Err(io::Error::other(
                        "剪贴板操作尚未完成；稍后退出，或 F1 执行 cancel-clipboard 放弃结果",
                    ));
                }
                if self.is_read_only() {
                    return Ok(Outcome::Exit);
                }
                self.save()?;
                self.checkpoint(false, true)?;
                return Ok(Outcome::Exit);
            }
            action => {
                if self.document.apply(action) {
                    self.map_other_pane();
                    self.saving.edited();
                    if undo || redo {
                        self.announce_history_target(redo);
                    }
                }
            }
        }
        Ok(Outcome::Continue)
    }
    /// Decode terminal input through the same session used by semantic commands.
    pub fn handle_event(&mut self, event: crossterm::event::Event) -> io::Result<Option<Outcome>> {
        let scene = self.design_scene();
        let palette_was_open = self.commands.palette_selection().is_some();
        let immediate = matches!(
            &event,
            crossterm::event::Event::Paste(_)
                | crossterm::event::Event::Resize(_, _)
                | crossterm::event::Event::Key(crossterm::event::KeyEvent {
                    code: crossterm::event::KeyCode::Char(_)
                        | crossterm::event::KeyCode::Backspace
                        | crossterm::event::KeyCode::Delete,
                    ..
                })
        );
        let result = self.handle_event_inner(event);
        if immediate && palette_was_open && self.commands.palette_selection().is_some() {
            self.cancel_design_transition();
        } else if self.design_scene() != scene {
            self.begin_design_transition(std::time::Instant::now());
        } else if immediate {
            self.cancel_design_transition();
        }
        result
    }
    fn handle_event_inner(
        &mut self,
        event: crossterm::event::Event,
    ) -> io::Result<Option<Outcome>> {
        if matches!(event, crossterm::event::Event::Paste(_))
            || matches!(event, crossterm::event::Event::Key(k) if k.kind != crossterm::event::KeyEventKind::Release)
        {
            self.dismiss_export_notice();
        }
        if let Some(outcome) = self.design_event(&event)? {
            return Ok(Some(outcome));
        }
        if self.import_captures(&event) {
            return self.import_event(event);
        }
        if let crossterm::event::Event::Mouse(event) = event {
            self.handle_mouse(event)?;
            return Ok(Some(Outcome::Continue));
        }
        self.cancel_mouse_drag();
        if let Some(id) = self.commands.global_command(&event) {
            return self.execute_command(&id).map(Some);
        }
        if self.prompt.as_ref().is_some_and(|p| p.editor.is_some()) {
            if matches!(&event, crossterm::event::Event::Key(k) if k.code == crossterm::event::KeyCode::Esc && k.kind != crossterm::event::KeyEventKind::Release)
            {
                self.prompt = None;
                self.restore_popup_if_closed();
            } else if let Some(id) = self.commands.bound_command(&event) {
                if matches!(
                    id.as_str(),
                    "undo"
                        | "redo"
                        | "select-all"
                        | "copy"
                        | "cut"
                        | "paste"
                        | "copy-internal"
                        | "cut-internal"
                        | "paste-internal"
                        | "cancel-clipboard"
                ) {
                    return self.execute_command(&id).map(Some);
                }
            } else if let Some(action) = input::decode(event) {
                self.dispatch(action)?;
            }
            return Ok(Some(Outcome::Continue));
        }
        if let Some(prompt) = &mut self.prompt {
            let response = prompt.event(event);
            let transfers_focus = matches!(&response, prompt::Response::Submit)
                && matches!(
                    prompt.purpose,
                    prompt::Purpose::Find | prompt::Purpose::SearchBook
                );
            match response {
                prompt::Response::Cancel => {
                    let cancel_import = matches!(
                        prompt.purpose,
                        prompt::Purpose::ImportSource | prompt::Purpose::ImportEncoding(_)
                    );
                    self.prompt = None;
                    if cancel_import {
                        self.cancel_import();
                    }
                }
                prompt::Response::Submit => {
                    let text = prompt.text.clone();
                    let empty = match prompt.purpose {
                        prompt::Purpose::ExportAuthor
                        | prompt::Purpose::Replace { .. }
                        | prompt::Purpose::Format(_) => false,
                        prompt::Purpose::Find | prompt::Purpose::SearchBook => text.is_empty(),
                        _ => text.trim().is_empty(),
                    };
                    if empty {
                        return Err(io::Error::other("输入不能为空"));
                    }
                    match prompt.purpose.clone() {
                        prompt::Purpose::ExportAuthor => {
                            self.begin_prompt(prompt::Purpose::ExportDestination(
                                ExportFormat::BookText { author: text },
                            ))?;
                            return Ok(Some(Outcome::Continue));
                        }
                        prompt::Purpose::ExportDestination(format) => {
                            self.export_manuscript(format, &text)?
                        }
                        prompt::Purpose::ImportSource => {
                            return self.import_source_response(
                                PathBuf::from(&text),
                                ImportEncoding::Utf8,
                            );
                        }
                        prompt::Purpose::ImportEncoding(source) => {
                            return self
                                .import_source_response(source, import_ui::encoding(&text)?);
                        }
                        prompt::Purpose::ImportDestination => self.confirm_import(&text)?,
                        prompt::Purpose::Format(request) => self.apply_format(request)?,
                        prompt::Purpose::Outline(_) => unreachable!("outline uses its editor"),
                        prompt::Purpose::ChapterGoal(edit) => {
                            self.apply_plan_edit(edit, &text, false)?
                        }
                        prompt::Purpose::BookGoal { root, original } => {
                            self.apply_book_goal(root, original, &text)?
                        }
                        prompt::Purpose::Find => self.find_chapter(text),
                        prompt::Purpose::SearchBook => self.start_book_search(&text)?,
                        prompt::Purpose::Replace { replacement, .. } => {
                            self.replace_matches(replacement, &text)?
                        }
                        prompt::Purpose::ConflictCopy => self.save_conflict_copy(&text)?,
                        prompt::Purpose::ConflictOverwrite => {
                            if text != "覆盖" {
                                return Err(io::Error::other("请输入「覆盖」确认，或 Esc 取消"));
                            }
                            self.overwrite_external()?;
                        }
                        prompt::Purpose::HistoryPolicy => self.apply_history_settings(&text)?,
                        prompt::Purpose::CopyChapter { chapter, volume } => {
                            self.copy_chapter(&chapter, &volume, &text)?;
                        }
                        prompt::Purpose::SaveAs => self.save_as(&text)?,
                        prompt::Purpose::RenameProject => self.rename_project(&text)?,
                        prompt::Purpose::OpenProject => self.switch_path(&text)?,
                        prompt::Purpose::NewProject => {
                            let title = Path::new(&text)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            self.new_project(&text, &title)?;
                        }
                        prompt::Purpose::NewVolume => {
                            self.selected_volume = Some(self.create_volume(&text)?);
                        }
                        prompt::Purpose::NewChapter => {
                            let volume = self.selected_volume_id()?;
                            let id = self.create_chapter(&volume, &text)?;
                            self.switch_chapter(&id)?;
                        }
                        prompt::Purpose::RenameVolume => {
                            let id = self.selected_volume_id()?;
                            self.rename_volume(&id, &text)?;
                        }
                        prompt::Purpose::RenameChapter => {
                            let id = self
                                .active_chapter()
                                .ok_or_else(|| io::Error::other("请先打开作品"))?
                                .id
                                .clone();
                            self.rename_chapter(&id, &text)?;
                        }
                    }
                    self.prompt = None;
                }
                prompt::Response::Editing => {}
            }
            self.finish_export_prompt();
            if !transfers_focus {
                self.restore_popup_if_closed();
            }
            return Ok(Some(Outcome::Continue));
        }
        if self.book_search_focused() && self.palette().is_none() {
            use crossterm::event::{Event, KeyCode, KeyModifiers};
            let command_key = matches!(&event, Event::Key(key) if matches!(key.code, KeyCode::F(_)) || key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER));
            if !command_key {
                return self.book_search_event(event);
            }
        }
        if self.navigator.is_some() && self.palette().is_none() {
            use crossterm::event::{Event, KeyCode, KeyModifiers};
            let command_key = matches!(&event, Event::Key(key) if matches!(key.code, KeyCode::F(_)) || key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER));
            if !command_key || self.recent_record_action(&event) {
                return self.navigation_event(event);
            }
        }
        if self.history_preview.is_some() && self.palette().is_none() {
            use crossterm::event::{Event, KeyCode, KeyModifiers};
            let command_key = matches!(&event, Event::Key(key) if matches!(key.code, KeyCode::F(_)) || key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER));
            if !command_key {
                return self.history_preview_event(event).map(Some);
            }
        }
        if self.palette().is_none()
            && self.chapter_search().is_some()
            && matches!(&event, crossterm::event::Event::Key(key) if key.code == crossterm::event::KeyCode::Esc && key.kind != crossterm::event::KeyEventKind::Release)
        {
            self.close_find();
            return Ok(Some(Outcome::Continue));
        }
        match self.commands.event(&event) {
            commands::CommandEvent::Consumed => return Ok(None),
            commands::CommandEvent::Execute(id) => return self.execute_command(&id).map(Some),
            commands::CommandEvent::Editor => {}
        }
        self.typing_event(event)
    }

    pub fn palette(&self) -> Option<Palette> {
        self.commands.palette()
    }
    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }
    pub(crate) fn begin_prompt(&mut self, purpose: prompt::Purpose) -> io::Result<Outcome> {
        if matches!(
            purpose,
            prompt::Purpose::RenameProject
                | prompt::Purpose::NewVolume
                | prompt::Purpose::NewChapter
                | prompt::Purpose::RenameVolume
                | prompt::Purpose::RenameChapter
        ) {
            self.require_writable()?;
            if self.project.is_none() {
                return Err(io::Error::other("当前打开的是单篇文稿"));
            }
        }
        self.prompt = Some(Prompt::new(purpose));
        Ok(Outcome::Continue)
    }

    pub fn configure_keyboard(&mut self, probe: io::Result<bool>) -> bool {
        self.enhanced_keyboard = probe.unwrap_or(false);
        self.enhanced_keyboard
    }
    pub fn enhanced_keyboard(&self) -> bool {
        self.enhanced_keyboard
    }
    pub fn register_command(&mut self, command: Command) -> io::Result<()> {
        self.commands.register(command)
    }
    pub fn execute_command(&mut self, id: &str) -> io::Result<Outcome> {
        self.dismiss_export_notice();
        self.guard_import_command(id)?;
        let result = (self.commands.get(id)?)(self);
        if result.is_ok()
            && !matches!(
                id,
                "save" | "quit" | "undo" | "redo" | "copy" | "cut" | "paste"
            )
        {
            self.design_ui.revision += 1;
        }
        result
    }

    pub fn load_keymap(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let resolved = keymap::resolve_destination(path).ok();
        if resolved.is_none()
            || resolved.as_deref() == Some(self.path.as_path())
            || resolved.as_deref() == self.library.path()
            || resolved.as_deref() == self.modules.path.as_deref()
            || resolved.as_ref().is_some_and(|path| {
                self.project
                    .as_ref()
                    .is_some_and(|store| path.starts_with(&store.root))
            })
        {
            self.commands
                .reject_keymap("键位路径无效或与文稿、作品、最近记录冲突；使用默认键位");
        } else {
            self.commands.load_keymap(path);
        }
    }
    pub fn set_keybinding(&mut self, key: &str, id: &str) -> io::Result<()> {
        self.commands.set_keybinding(key, id)
    }
    pub fn configuration_warning(&self) -> Option<&str> {
        self.commands.warning()
    }
    pub fn command_shortcuts(&self, id: &str) -> String {
        self.commands.shortcuts(id)
    }

    pub fn selected_text(&self) -> &str {
        self.document.selected_text()
    }

    fn save(&mut self) -> io::Result<()> {
        self.finish_save(true)?;
        if !self.saving.uncertain {
            self.check_external()?;
        }
        self.ensure_no_conflict()?;
        if self.is_dirty() {
            self.request_save()?;
            self.finish_save(true)?;
        }
        self.remember_location()
    }

    pub fn request_save(&mut self) -> io::Result<()> {
        if !self.is_dirty() || self.saving.running() {
            return Ok(());
        }
        self.require_writable()?;
        self.ensure_no_conflict()?;
        let target = self.project.as_ref().map_or_else(
            || saving::Target::File {
                path: self.path.clone(),
                new: self.document.new_file,
            },
            |store| saving::Target::Project(Box::new(store.clone())),
        );
        let metadata = self.project.as_ref().map_or(0, |store| store.revision);
        self.saving.request(
            target,
            self.document.text.clone(),
            self.document.revision,
            metadata,
            self.document.disk,
            self.external.manifest,
        )
    }
    /// Save a complete copy to a new destination, then continue editing that copy.
    pub fn save_as(&mut self, destination: impl AsRef<Path>) -> io::Result<()> {
        self.require_export_idle()?;
        self.protect_layout(destination.as_ref(), self.project.is_some())?;
        self.finish_history()?;
        self.library
            .protect(destination.as_ref(), self.project.is_some())?;
        // Drain an older write, including its failure, before switching destinations.
        let _ = self.finish_save(true);
        if let Some(store) = &self.project {
            let copy = store.copy_to(destination.as_ref(), &self.document.text)?;
            self.path = copy.path();
            self.project = Some(copy);
        } else {
            let mut copy = Self::open(destination)?;
            if !copy.document.new_file {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "另存目标已存在",
                ));
            }
            copy.dispatch(Action::Insert(self.document.text.clone()))?;
            copy.save()?;
            self.path = copy.path.clone();
        }
        self.document.new_file = false;
        self.document.saved_revision = self.document.revision;
        self.saving.error = None;
        self.saving.completed = true;
        self.saving.uncertain = false;
        self.reset_external_after_copy()?;
        self.close_book_search();
        if let Some(path) = self.commands.keymap_path().map(Path::to_owned) {
            self.load_keymap(path);
        }
        Ok(())
    }
    fn finish_save(&mut self, wait: bool) -> io::Result<()> {
        let completed = self.saving.complete(wait).inspect_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                self.saved_conflict();
            }
        })?;
        if let Some(saving::Saved {
            revision,
            metadata,
            disk,
            manifest,
        }) = completed
        {
            self.document.disk = disk;
            self.external.manifest = manifest;
            self.document.saved_revision = revision;
            self.document.new_file = false;
            if let Some(store) = &mut self.project {
                store.dirty = store.revision != metadata;
                store.pending.clear();
            }
        }
        Ok(())
    }
    pub fn poll_save(&mut self) -> io::Result<()> {
        self.finish_save(false)
    }
    pub fn tick(&mut self, now: std::time::Instant) -> io::Result<()> {
        self.poll_import()?;
        self.poll_export()?;
        self.tick_mouse(now);
        self.poll_book_search();
        self.poll_word_counts()?;
        self.poll_clipboard()?;
        self.poll_save()?;
        self.poll_external(now)?;
        if self.external_conflict().is_none() && self.is_dirty() && self.saving.due(now) {
            self.request_save()?;
        }
        self.history_tick()
    }
    pub fn save_state(&self) -> SaveState {
        if let Some(conflict) = self.external_conflict() {
            SaveState::Conflict(conflict.message.clone())
        } else if self.is_read_only() {
            SaveState::ReadOnly
        } else if let Some(error) = &self.saving.error {
            SaveState::Failed(error.clone())
        } else if self.saving.running() {
            SaveState::Saving
        } else if self.is_dirty() {
            SaveState::Pending
        } else if self.saving.completed {
            SaveState::Saved
        } else {
            SaveState::Unchanged
        }
    }

    pub fn text(&self) -> &str {
        &self.document.text
    }
    pub fn is_read_only(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|store| store.lock.is_none())
    }
    fn require_writable(&self) -> io::Result<()> {
        if self.is_read_only() {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "作品已被其他实例打开，只读模式",
            ))
        } else {
            Ok(())
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.saving.uncertain
            || self.document.new_file
            || self.document.revision != self.document.saved_revision
            || self.project.as_ref().is_some_and(|store| store.dirty)
    }
    pub fn cursor(&self) -> usize {
        self.document.cursor
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn view(&mut self, width: u16, height: u16) -> View {
        self.document
            .writing_view(width, height, self.immersion_settings())
    }
}
