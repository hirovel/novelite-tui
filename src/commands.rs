use crate::{Action, Outcome, Workspace};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io;

const COMMON_COMMANDS: &[&str] = &[
    "open-project",
    "save",
    "find",
    "design-settings",
    "recent-projects",
    "save-as",
    "chapters",
    "quit",
    "browse-commands",
];

pub struct Command {
    pub id: String,
    pub title: String,
    pub run: fn(&mut Workspace) -> io::Result<Outcome>,
}
pub struct Palette<'a> {
    pub query: &'a str,
    pub entries: Vec<&'a Command>,
    pub selected: usize,
}
pub(crate) enum CommandEvent {
    Editor,
    Consumed,
    Execute(String),
}
pub(crate) struct Commands {
    entries: Vec<Command>,
    owners: std::collections::HashMap<String, crate::ModuleId>,
    disabled: std::collections::HashSet<crate::ModuleId>,
    query: Option<String>,
    query_editor: crate::line_editor::LineEditor,
    selected: usize,
    browsing_all: bool,
    keymap: crate::keymap::Keymap,
}

impl Default for Commands {
    fn default() -> Self {
        let mut entries = vec![
            Command {
                id: "browse-commands".into(),
                title: "浏览全部操作".into(),
                run: |work| {
                    work.commands.open_palette();
                    work.commands.browsing_all = true;
                    Ok(Outcome::Continue)
                },
            },
            Command {
                id: "design-settings".into(),
                title: "设置".into(),
                run: Workspace::open_design_settings,
            },
            Command {
                id: "design-commands".into(),
                title: "查找命令".into(),
                run: |w| {
                    w.design_ui.open = false;
                    w.commands.open_palette();
                    Ok(Outcome::Continue)
                },
            },
            Command {
                id: "design-motion".into(),
                title: "开关界面动效".into(),
                run: |work| work.toggle_saved_motion(),
            },
            Command {
                id: "save".into(),
                title: "保存文稿".into(),
                run: |work| work.dispatch(Action::Save),
            },
            Command {
                id: "save-as".into(),
                title: "另存到新路径".into(),
                run: |work| work.begin_prompt(crate::prompt::Purpose::SaveAs),
            },
            Command {
                id: "rename-project".into(),
                title: "作品改名".into(),
                run: |work| work.begin_prompt(crate::prompt::Purpose::RenameProject),
            },
            Command {
                id: "quit".into(),
                title: "保存并退出".into(),
                run: |work| work.dispatch(Action::Quit),
            },
            Command {
                id: "undo".into(),
                title: "撤销".into(),
                run: |work| work.run_edit_command(Action::Undo),
            },
            Command {
                id: "redo".into(),
                title: "重做".into(),
                run: |work| work.run_edit_command(Action::Redo),
            },
            Command {
                id: "select-all".into(),
                title: "全选文稿".into(),
                run: |work| work.run_edit_command(Action::SelectAll),
            },
            Command {
                id: "reset-keys".into(),
                title: "恢复默认键位".into(),
                run: |work| {
                    work.commands.keymap.reset()?;
                    Ok(Outcome::Continue)
                },
            },
        ];
        entries.extend(
            crate::navigator::commands()
                .into_iter()
                .filter(|c| crate::modules::is_library_command(&c.id)),
        );
        entries.extend(crate::heading::commands());
        entries.extend(crate::history_ui::commands());
        entries.extend(crate::external::commands());
        entries.extend(crate::clipboard::commands());
        entries.extend(crate::mouse::commands());
        entries.extend(crate::panes::commands());
        entries.extend(crate::modules::commands());
        entries.extend(crate::appearance::commands());
        let mut owners = std::collections::HashMap::new();
        for contribution in crate::modules::contributions() {
            for command in contribution.commands {
                owners.insert(command.id.clone(), contribution.id);
                entries.push(command);
            }
        }
        Self {
            entries,
            owners,
            disabled: Default::default(),
            query: None,
            query_editor: Default::default(),
            selected: 0,
            browsing_all: false,
            keymap: crate::keymap::Keymap::default(),
        }
    }
}

impl Commands {
    pub(crate) fn palette_selection(&self) -> Option<usize> {
        self.query.as_ref().map(|_| self.selected)
    }
    pub(crate) fn toggles_palette(event: &Event) -> bool {
        matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release
            && (key.code == KeyCode::F(1)
                || key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::CONTROL))
    }
    pub(crate) fn close_palette(&mut self) {
        self.query = None;
        self.query_editor = Default::default();
        self.selected = 0;
        self.browsing_all = false;
    }
    pub(crate) fn open_palette(&mut self) {
        self.query = Some(String::new());
        self.query_editor = Default::default();
        self.selected = 0;
        self.browsing_all = false;
    }

    pub(crate) fn search_palette(&mut self, query: &str) {
        self.open_palette();
        self.query = Some(query.into());
        self.query_editor.event(
            self.query.as_mut().unwrap(),
            &Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::End,
                KeyModifiers::NONE,
            )),
        );
    }

    pub fn global_command(&self, event: &Event) -> Option<String> {
        let id = self.bound_command(event)?;
        matches!(id.as_str(), "save" | "quit").then_some(id)
    }
    pub fn bound_command(&self, event: &Event) -> Option<String> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind == KeyEventKind::Release {
            return None;
        }
        self.keymap.lookup(*key)
    }
    pub fn enable_module(&mut self, id: crate::ModuleId, enabled: bool) {
        if enabled {
            self.disabled.remove(&id);
        } else {
            self.disabled.insert(id);
        }
    }
    fn active(&self, id: &str) -> bool {
        self.owners
            .get(id)
            .is_none_or(|owner| !self.disabled.contains(owner))
    }
    pub fn keymap_path(&self) -> Option<&std::path::Path> {
        self.keymap.path()
    }
    /// Borrow titles for painting; derive common order and exact-id priority
    /// directly instead of cloning and sorting every command on every frame.
    pub fn palette(&self) -> Option<Palette<'_>> {
        let query = self.query.as_ref()?;
        let lowered = query.to_lowercase();
        let common_only = query.is_empty() && !self.browsing_all;
        let matches = |command: &&Command| {
            self.active(&command.id)
                && (!self.browsing_all || !query.is_empty() || command.id != "browse-commands")
                && (command.id.contains(&lowered)
                    || command.title.to_lowercase().contains(&lowered))
        };
        let entries = if common_only {
            COMMON_COMMANDS
                .iter()
                .filter_map(|id| self.entries.iter().find(|command| command.id == *id))
                .filter(matches)
                .collect()
        } else {
            self.entries
                .iter()
                .filter(|command| command.id == lowered)
                .chain(self.entries.iter().filter(|command| command.id != lowered))
                .filter(matches)
                .collect::<Vec<_>>()
        };
        Some(Palette {
            query,
            selected: self.selected.min(entries.len().saturating_sub(1)),
            entries,
        })
    }

    pub(crate) fn description(&self, id: &str) -> &'static str {
        match id {
            "open-project" => "打开本地文稿或作品文件夹，继续写作。",
            "save" => "将当前修改保存到磁盘，保留正文与光标位置。",
            "find" => "查找当前章节中的文字，查看匹配位置。",
            "design-settings" => "调整排版、布局与写作偏好。字体与光标外观沿用终端。",
            "recent-projects" => "从最近文件与作品继续写作。移除记录不会删除稿件。",
            "save-as" => "把当前文稿保存到新路径，保留已有文件。",
            "chapters" => "浏览作品的卷与章节，在当前窗格打开所选章节。",
            "quit" => "保存当前修改后退出；保存失败时保留工作区。",
            "browse-commands" => "浏览全部可用操作，也可输入名称或命令 ID 筛选。",
            "undo" => "撤销当前章节的最近一次编辑。",
            "redo" => "重做当前章节已撤销的编辑。",
            _ => "在当前工作区执行所选操作。Esc 返回正文。",
        }
    }

    pub(crate) fn query_view(&mut self, width: u16) -> (String, u16) {
        self.query_editor
            .view(self.query.as_deref().unwrap_or(""), width)
    }

    pub fn event(&mut self, event: &Event) -> CommandEvent {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                return CommandEvent::Consumed;
            }
            if key.code == KeyCode::Esc && self.query.is_some() || Self::toggles_palette(event) {
                if self.query.is_some() {
                    self.close_palette();
                } else {
                    self.open_palette();
                }
                return CommandEvent::Consumed;
            }
        }
        if self.query.is_none() {
            if let Event::Key(key) = event
                && let Some(id) = self.keymap.lookup(*key)
            {
                return CommandEvent::Execute(id);
            }
            return CommandEvent::Editor;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter => {
                    let palette = self.palette().unwrap();
                    if let Some(id) = palette.entries.get(palette.selected).map(|c| c.id.clone()) {
                        self.close_palette();
                        return CommandEvent::Execute(id);
                    }
                }
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    let count = self.palette().unwrap().entries.len();
                    self.selected = (self.selected + 1).min(count.saturating_sub(1));
                }
                _ => {}
            }
        }
        if self.query_editor.event(self.query.as_mut().unwrap(), event) {
            self.selected = 0;
        }
        CommandEvent::Consumed
    }

    pub fn register(&mut self, command: Command) -> io::Result<()> {
        if command.id.is_empty()
            || !command
                .id
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            || self.entries.iter().any(|item| item.id == command.id)
        {
            return Err(io::Error::other(
                "Command ID is invalid or already registered",
            ));
        }
        self.entries.push(command);
        Ok(())
    }

    pub fn get(&self, id: &str) -> io::Result<fn(&mut Workspace) -> io::Result<Outcome>> {
        self.entries
            .iter()
            .find(|command| command.id == id && self.active(id))
            .map(|command| command.run)
            .ok_or_else(|| io::Error::other("Unknown command"))
    }

    pub fn load_keymap(&mut self, path: &std::path::Path) {
        let known: Vec<_> = self
            .entries
            .iter()
            .map(|command| command.id.as_str())
            .collect();
        self.keymap.load(path, &known);
    }
    pub fn reject_keymap(&mut self, message: &str) {
        self.keymap = crate::keymap::Keymap::default();
        self.keymap.warning = Some(message.into());
    }
    pub fn set_keybinding(&mut self, key: &str, id: &str) -> io::Result<()> {
        self.get(id)?;
        self.keymap.set(key, id)
    }
    pub fn warning(&self) -> Option<&str> {
        self.keymap.warning.as_deref()
    }
    pub fn shortcuts(&self, id: &str) -> String {
        self.keymap.shortcuts(id)
    }
}
