//! Startup has no document until the writer chooses a real destination.
use crate::{
    RecentProject, Workspace,
    library::Library,
    prompt::{Prompt, Purpose, Response},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph},
};
use std::{
    io,
    path::{Path, PathBuf},
};

pub enum StartOutcome {
    Stay,
    Open(Box<Workspace>),
    Exit,
}
pub struct StartScreen {
    library: Library,
    prompt: Option<Prompt>,
    selected: usize,
    message: String,
    reserved: Vec<PathBuf>,
    hits: Vec<(Rect, usize)>,
    mouse_enabled: bool,
}
impl StartScreen {
    pub fn new(state: Option<PathBuf>) -> Self {
        let library = state.map_or_else(Library::default, |path| match Library::read(&path) {
            Ok(library) => library,
            Err(error) => Library::unavailable(path, &error),
        });
        let message = library.warning().unwrap_or_default().to_owned();
        Self {
            library,
            prompt: None,
            selected: 0,
            message,
            reserved: vec![],
            hits: vec![],
            mouse_enabled: false,
        }
    }
    /// Configuration destinations are checked before creating a project or entering an editor.
    pub fn reserve(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.reserved
            .push(crate::keymap::resolve_destination(path.as_ref())?);
        Ok(())
    }
    pub fn configure_mouse(&mut self, enabled: bool) {
        self.mouse_enabled = enabled;
    }
    pub fn entries(&self) -> &[RecentProject] {
        &self.library.entries
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }
    pub fn set_message(&mut self, message: String) {
        self.message = message;
    }
    pub fn resume(&mut self) -> Option<Workspace> {
        let entry = self.library.entries.first()?.clone();
        match self
            .protect(&entry.root, entry.kind == crate::RecentKind::Project)
            .and_then(|_| Workspace::open_recent(&entry))
            .and_then(|work| self.attach(work))
        {
            Ok(work) => Some(work),
            Err(error) => {
                self.message =
                    format!("无法继续「{}」：{error}；请选择其他文件或作品", entry.title);
                None
            }
        }
    }
    fn protect(&self, path: &Path, project: bool) -> io::Result<()> {
        self.library.protect(path, project)?;
        let path = crate::keymap::resolve_destination(path)?;
        if self
            .reserved
            .iter()
            .any(|p| *p == path || project && p.starts_with(&path))
        {
            return Err(io::Error::other(
                "文稿和作品不能覆盖配置文件，请选择其他路径",
            ));
        }
        Ok(())
    }
    fn attach(&self, mut work: Workspace) -> io::Result<Workspace> {
        let root = work
            .project
            .as_ref()
            .map_or(work.path.as_path(), |s| s.root.as_path());
        self.protect(root, work.project.is_some())?;
        work.library = self.library.clone();
        work.restore_location()?;
        work.remember_location()?;
        Ok(work)
    }
    /// CLI keeps its existing ability to create a new standalone file in an existing folder.
    pub fn open_requested(
        &self,
        path: impl AsRef<Path>,
        new_project: bool,
    ) -> io::Result<Workspace> {
        let path = if new_project {
            crate::user_path::expand(path.as_ref())?
        } else {
            crate::user_path::resolve(path.as_ref(), true)?
        };
        self.protect(&path, new_project || path.is_dir())?;
        let work = if new_project {
            let title = path.file_name().unwrap_or_default().to_string_lossy();
            Workspace::create_project(&path, &title)?
        } else {
            Workspace::open_user_path(path, true)?
        };
        self.attach(work)
    }
    fn activate(&mut self) -> io::Result<StartOutcome> {
        let recent = self.library.entries.len();
        if self.selected < recent {
            let entry = self.library.entries[self.selected].clone();
            self.protect(&entry.root, entry.kind == crate::RecentKind::Project)?;
            let work = self.attach(Workspace::open_recent(&entry)?)?;
            Ok(StartOutcome::Open(Box::new(work)))
        } else {
            self.prompt = Some(Prompt::new(if self.selected == recent {
                Purpose::OpenProject
            } else {
                Purpose::NewProject
            }));
            Ok(StartOutcome::Stay)
        }
    }
    pub fn handle_event(&mut self, event: Event) -> io::Result<StartOutcome> {
        if matches!(&event, Event::Key(k) if k.kind != KeyEventKind::Release && ((k.code == KeyCode::Char('q') && k.modifiers == KeyModifiers::CONTROL) || k.code == KeyCode::F(10)))
        {
            return Ok(StartOutcome::Exit);
        }
        if let Some(prompt) = &mut self.prompt {
            match prompt.event(event) {
                Response::Cancel => self.prompt = None,
                Response::Submit => {
                    let new = matches!(prompt.purpose, Purpose::NewProject);
                    let text = prompt.text.clone();
                    if text.trim().is_empty() {
                        return Err(io::Error::other("输入不能为空"));
                    }
                    if !new {
                        crate::user_path::resolve(Path::new(&text), false)?;
                    }
                    let work = self.open_requested(text, new)?;
                    return Ok(StartOutcome::Open(Box::new(work)));
                }
                Response::Editing => {}
            }
            return Ok(StartOutcome::Stay);
        }
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => return Ok(StartOutcome::Exit),
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(self.library.entries.len() + 1)
                }
                KeyCode::Enter => return self.activate(),
                KeyCode::Char('o') | KeyCode::Char('p')
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::CONTROL =>
                {
                    self.prompt = Some(Prompt::new(Purpose::OpenProject))
                }
                KeyCode::Char('n') if key.modifiers.is_empty() => {
                    self.prompt = Some(Prompt::new(Purpose::NewProject))
                }
                KeyCode::Char('d')
                    if key.modifiers == KeyModifiers::CONTROL
                        && self.selected < self.library.entries.len() =>
                {
                    let mut library = self.library.clone();
                    library.entries.remove(self.selected);
                    library.persist()?;
                    self.library = library;
                    self.selected = self.selected.min(self.library.entries.len() + 1);
                }
                _ => {}
            },
            Event::Mouse(mouse) if self.mouse_enabled => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((_, index)) = self
                        .hits
                        .iter()
                        .find(|(r, _)| r.contains(Position::new(mouse.column, mouse.row)))
                    {
                        self.selected = *index;
                        return self.activate();
                    }
                }
                MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
                MouseEventKind::ScrollDown => {
                    self.selected = (self.selected + 1).min(self.library.entries.len() + 1)
                }
                _ => {}
            },
            _ => {}
        }
        Ok(StartOutcome::Stay)
    }
    pub fn render(&mut self, frame: &mut ratatui::Frame<'_>) {
        use crate::render::{chrome_line, fit_label, safe_label};
        self.hits.clear();
        let area = frame.area();
        let base = Style::default().fg(Color::Reset).bg(Color::Reset);
        let monochrome = crate::detect_color_support() == crate::ColorMode::Monochrome;
        let accent = if monochrome {
            base
        } else {
            base.fg(Color::Blue)
        }
        .add_modifier(Modifier::BOLD);
        let muted = base.add_modifier(Modifier::DIM);
        frame.render_widget(Block::default().style(base), area);
        if area.width < 8 || area.height < 5 {
            return;
        }
        let width = area.width.saturating_sub(4).min(84);
        let x = area.x + (area.width - width) / 2;
        let top = area.y + (area.height / 7).min(4);
        let row = |y| Rect::new(x, y, width, 1);
        frame.render_widget(Paragraph::new("Novelite").style(accent), row(top));
        frame.render_widget(Paragraph::new("继续你的故事").style(muted), row(top + 1));
        let bottom = area
            .bottom()
            .saturating_sub(if self.prompt.is_some() { 7 } else { 3 });
        let capacity = bottom.saturating_sub(top + 3) as usize;
        let first = self.selected.saturating_sub(capacity.saturating_sub(1));
        for (offset, index) in (first..self.library.entries.len() + 2)
            .take(capacity)
            .enumerate()
        {
            let r = row(top + 3 + offset as u16);
            let (title, detail) = if let Some(entry) = self.library.entries.get(index) {
                (
                    entry.title.clone(),
                    format!(
                        "{} · {}",
                        if entry.kind == crate::RecentKind::File {
                            "单篇"
                        } else {
                            "作品"
                        },
                        entry.root.display()
                    ),
                )
            } else if index == self.library.entries.len() {
                ("打开文件或作品…".into(), "".into())
            } else {
                ("新建作品…".into(), "".into())
            };
            let selected = index == self.selected;
            chrome_line(
                frame,
                r,
                &format!(
                    "{} {}",
                    if selected { "▎" } else { " " },
                    safe_label(&title)
                ),
                &safe_label(&detail),
                if selected { accent } else { base },
                muted,
            );
            self.hits.push((r, index));
        }
        let message_y = area.bottom().saturating_sub(3);
        if !self.message.is_empty() {
            frame.render_widget(
                Paragraph::new(safe_label(&self.message)).style(if monochrome {
                    accent
                } else {
                    base.fg(Color::Yellow)
                }),
                Rect::new(x, message_y, width, 2),
            );
        }
        let hint = if self.library.entries.is_empty() {
            "Enter 打开 · N 新建 · Esc 退出"
        } else if width < 52 {
            "Enter 继续 · Ctrl+D 移除"
        } else {
            "Enter 继续 · O 打开 · N 新建 · Ctrl+D 移除记录"
        };
        frame.render_widget(
            Paragraph::new(fit_label(hint, width)).style(muted),
            row(area.bottom() - 1),
        );
        if let Some(prompt) = &mut self.prompt {
            let y = area.bottom().saturating_sub(6);
            frame.render_widget(Block::default().style(base), Rect::new(x, y, width, 5));
            frame.render_widget(
                Paragraph::new(fit_label(&prompt.title, width)).style(accent),
                row(y),
            );
            let (text, cursor) = prompt.line_view(width.saturating_sub(2));
            frame.render_widget(Paragraph::new(format!("› {text}")).style(base), row(y + 1));
            frame.render_widget(
                Paragraph::new(fit_label(prompt.hint(), width)).style(muted),
                row(y + 2),
            );
            if !self.message.is_empty() {
                frame.render_widget(
                    Paragraph::new(safe_label(&self.message)).style(muted),
                    row(y + 3),
                );
            }
            frame.set_cursor_position((x + 2 + cursor, y + 1));
        }
    }
}
