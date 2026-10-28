mod view;
// Settings surface with shared draw and hit geometry.
use crate::{Density, ModuleId, Outcome, TextWidth, Workspace};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::io;

#[derive(Default)]
pub(crate) struct DesignUi {
    pub open: bool,
    category: usize,
    selected: usize,
    scroll: usize,
    pub palette_scroll: usize,
    pub revision: u64,
    can_apply: bool,
    hits: Vec<(Rect, Hit)>,
    command_hit: Option<Rect>,
    pub(crate) zen_exit_hit: Option<Rect>,
    pub(crate) heading_hit: Option<Rect>,
    pub palette_close: Option<Rect>,
    pub palette_hits: Vec<(Rect, String)>,
}
#[derive(Clone, Copy)]
enum Hit {
    Category(usize),
    Option(usize),
    Close,
    Commands,
}
#[derive(Clone, Copy)]
enum Choice {
    Width(TextWidth),
    Density(Density),
    Command(&'static str),
}
struct Row {
    title: &'static str,
    value: String,
    detail: &'static str,
    previous: Choice,
    next: Choice,
}
const CATEGORIES: [&str; 3] = ["排版", "布局", "写作"];
fn cycle_row(
    title: &'static str,
    detail: &'static str,
    choices: &[(&str, Choice)],
    current: usize,
) -> Row {
    Row {
        title,
        value: choices[current].0.into(),
        detail,
        previous: choices[(current + choices.len() - 1) % choices.len()].1,
        next: choices[(current + 1) % choices.len()].1,
    }
}
fn toggle_row(
    title: &'static str,
    detail: &'static str,
    enabled: bool,
    command: &'static str,
) -> Row {
    Row {
        title,
        detail,
        value: if enabled { "开" } else { "关" }.into(),
        previous: Choice::Command(command),
        next: Choice::Command(command),
    }
}
impl Workspace {
    pub(crate) fn begin_design_frame(&mut self) {
        self.navigation_ui.clear_hits();
        self.design_ui.command_hit = None;
        self.design_ui.zen_exit_hit = None;
        self.design_ui.heading_hit = None;
        self.design_ui.palette_hits.clear();
        self.design_ui.palette_close = None;
        self.design_ui.hits.clear();
        self.design_ui.can_apply = false;
    }
    pub fn design_settings_open(&self) -> bool {
        self.design_ui.open
    }
    pub fn open_design_settings(&mut self) -> io::Result<Outcome> {
        if self.prompt.is_some() || self.import_active() || self.history_preview.is_some() {
            return Err(io::Error::other("请先完成或关闭当前操作"));
        }
        self.commands.close_palette();
        self.cancel_mouse_drag();
        self.design_ui.open = true;
        self.design_ui.revision += 1;
        Ok(Outcome::Continue)
    }
    pub fn toggle_saved_motion(&mut self) -> io::Result<Outcome> {
        let enabled = !self.design_motion_enabled();
        self.update_appearance(|a| a.motion = Some(enabled))?;
        self.set_design_motion(enabled);
        Ok(Outcome::Continue)
    }
    fn settings_rows(&self) -> Vec<Row> {
        let a = &self.modules.config.appearance;
        let immersion = &self.modules.config.immersion;
        match self.design_ui.category {
            0 => vec![
                cycle_row(
                    "正文宽度",
                    "阅读随窗口调整；宽幅最多 110 列；填满可用正文区域",
                    &[
                        ("阅读", Choice::Width(TextWidth::Adaptive)),
                        ("宽幅", Choice::Width(TextWidth::Wide)),
                        ("填满", Choice::Width(TextWidth::Full)),
                    ],
                    match a.text_width {
                        TextWidth::Adaptive => 0,
                        TextWidth::Wide => 1,
                        TextWidth::Full => 2,
                    },
                ),
                cycle_row(
                    "页面留白",
                    "仅调整正文周围的留白，不改动段落内容",
                    &[
                        ("紧凑", Choice::Density(Density::Compact)),
                        ("舒适", Choice::Density(Density::Comfortable)),
                        ("宽松", Choice::Density(Density::Spacious)),
                    ],
                    match a.density {
                        Density::Compact => 0,
                        Density::Comfortable => 1,
                        Density::Spacious => 2,
                    },
                ),
            ],
            1 => {
                let chapters = self.panel_preferences(ModuleId::Chapters);
                let planning = self.panel_preferences(ModuleId::Planning);
                vec![
                    cycle_row(
                        "分栏",
                        "两个窗格都可编辑；收起保留文稿与历史",
                        &[
                            ("单栏", Choice::Command("close-pane")),
                            ("左右", Choice::Command("split-right")),
                            ("上下", Choice::Command("split-down")),
                        ],
                        if !self.is_split() {
                            0
                        } else if self.panes.stacked {
                            2
                        } else {
                            1
                        },
                    ),
                    toggle_row(
                        "固定章节",
                        "窗口较窄时自动让出正文空间",
                        chapters.is_some_and(|p| p.pinned),
                        "pin-chapters",
                    ),
                    toggle_row(
                        "固定大纲",
                        "在正文旁显示章节大纲与写作目标",
                        planning.is_some_and(|p| p.pinned),
                        "pin-planning",
                    ),
                    Row {
                        title: "章节宽度",
                        value: chapters.map_or("—".into(), |p| format!("{} 列", p.width)),
                        detail: "左右键调整 4 列；窄屏会临时收起边栏",
                        previous: Choice::Command("chapters-narrower"),
                        next: Choice::Command("chapters-wider"),
                    },
                    Row {
                        title: "大纲宽度",
                        value: planning.map_or("—".into(), |p| format!("{} 列", p.width)),
                        detail: "左右键调整 4 列；窄屏会临时收起边栏",
                        previous: Choice::Command("planning-narrower"),
                        next: Choice::Command("planning-wider"),
                    },
                ]
            }
            _ => vec![
                toggle_row(
                    "对话高亮",
                    "突出引号内的对话，不改动原文",
                    immersion.dialogue,
                    "toggle-dialogue",
                ),
                toggle_row(
                    "打字机",
                    "输入后让当前行保持居中",
                    immersion.typewriter,
                    "toggle-typewriter",
                ),
                Row {
                    title: "段落聚焦",
                    value: match immersion.focus {
                        crate::immersion::Focus::Off => "关",
                        crate::immersion::Focus::Paragraph => "开",
                        crate::immersion::Focus::Sentence => "句子",
                        crate::immersion::Focus::Nearby => "邻近行",
                    }
                    .into(),
                    detail: "降低其他段落的亮度；原文与光标位置不变",
                    previous: Choice::Command(if immersion.focus == crate::immersion::Focus::Off {
                        "focus-paragraph"
                    } else {
                        "focus-off"
                    }),
                    next: Choice::Command(if immersion.focus == crate::immersion::Focus::Off {
                        "focus-paragraph"
                    } else {
                        "focus-off"
                    }),
                },
                toggle_row(
                    "界面过渡",
                    "只过渡界面强调色；输入可随时打断",
                    self.design_motion_enabled(),
                    "design-motion",
                ),
                toggle_row(
                    "专注写作",
                    "临时只显示当前文稿；退出恢复窗格与面板，不改个人偏好",
                    self.zen_mode(),
                    "toggle-zen",
                ),
            ],
        }
    }

    fn apply_settings_row(&mut self, index: usize, forward: bool) -> io::Result<()> {
        let rows = self.settings_rows();
        if let Some(r) = rows.get(index) {
            match if forward { r.next } else { r.previous } {
                Choice::Width(w) => self.set_text_width(w)?,
                Choice::Density(d) => self.set_density(d)?,
                Choice::Command(c) => {
                    self.execute_command(c)?;
                }
            }
            self.design_ui.selected = index;
            self.design_ui.revision += 1;
        }
        Ok(())
    }
    pub(crate) fn design_event(&mut self, event: &Event) -> io::Result<Option<Outcome>> {
        if self.design_ui.open {
            if crate::commands::Commands::toggles_palette(event) {
                return self.execute_command("design-commands").map(Some);
            }
            if self.commands.global_command(event).is_some() {
                return Ok(None);
            }
            if let Some(id) = self.commands.bound_command(event) {
                if id == "design-motion" || id == "design-commands" {
                    return self.execute_command(&id).map(Some);
                }
                if id == "design-settings" {
                    self.design_ui.open = false;
                    self.design_ui.revision += 1;
                    return Ok(Some(Outcome::Continue));
                }
            }
            match event {
                Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                    KeyCode::Esc => {
                        self.design_ui.open = false;
                        self.design_ui.revision += 1;
                    }
                    KeyCode::BackTab => {
                        self.design_ui.category =
                            (self.design_ui.category + CATEGORIES.len() - 1) % CATEGORIES.len();
                        self.design_ui.selected = 0;
                        self.design_ui.scroll = 0;
                        self.design_ui.revision += 1;
                    }
                    KeyCode::Tab => {
                        self.design_ui.category = (self.design_ui.category + 1) % CATEGORIES.len();
                        self.design_ui.selected = 0;
                        self.design_ui.scroll = 0;
                        self.design_ui.revision += 1;
                    }
                    KeyCode::Left if self.design_ui.can_apply => {
                        self.apply_settings_row(self.design_ui.selected, false)?;
                    }
                    KeyCode::Right if self.design_ui.can_apply => {
                        self.apply_settings_row(self.design_ui.selected, true)?;
                    }
                    KeyCode::Up => {
                        self.design_ui.selected = self.design_ui.selected.saturating_sub(1);
                        self.design_ui.revision += 1;
                    }
                    KeyCode::Down => {
                        self.design_ui.selected =
                            (self.design_ui.selected + 1).min(self.settings_rows().len() - 1);
                        self.design_ui.revision += 1;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') if self.design_ui.can_apply => {
                        self.apply_settings_row(self.design_ui.selected, true)?;
                    }
                    _ => {}
                },
                Event::Mouse(m) if self.mouse_enabled() => {
                    if m.kind == MouseEventKind::Down(MouseButton::Left) {
                        let hit = self
                            .design_ui
                            .hits
                            .iter()
                            .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                            .map(|(_, h)| *h);
                        match hit {
                            Some(Hit::Category(i)) => {
                                self.design_ui.category = i;
                                self.design_ui.selected = 0;
                                self.design_ui.scroll = 0;
                                self.design_ui.revision += 1;
                            }
                            Some(Hit::Commands) => {
                                return self.execute_command("design-commands").map(Some);
                            }
                            Some(Hit::Option(i)) => self.apply_settings_row(i, true)?,
                            Some(Hit::Close) => {
                                self.design_ui.open = false;
                                self.design_ui.revision += 1;
                            }
                            _ => {}
                        }
                    } else if matches!(
                        m.kind,
                        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                    ) {
                        self.design_ui.selected = if m.kind == MouseEventKind::ScrollUp {
                            self.design_ui.selected.saturating_sub(1)
                        } else {
                            (self.design_ui.selected + 1).min(self.settings_rows().len() - 1)
                        };
                        self.design_ui.revision += 1;
                    }
                }
                _ => {}
            }
            return Ok(Some(Outcome::Continue));
        }
        if self.commands.palette_selection().is_some()
            && self.mouse_enabled()
            && let Event::Mouse(m) = event
            && matches!(
                m.kind,
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
            )
        {
            let code = if m.kind == MouseEventKind::ScrollDown {
                KeyCode::Down
            } else {
                KeyCode::Up
            };
            self.commands
                .event(&Event::Key(crossterm::event::KeyEvent::new(
                    code,
                    crossterm::event::KeyModifiers::NONE,
                )));
            return Ok(Some(Outcome::Continue));
        }
        if let Event::Mouse(m) = event
            && self.mouse_enabled()
            && m.kind == MouseEventKind::Down(MouseButton::Left)
        {
            let pos = Position::new(m.column, m.row);
            if self.commands.palette_selection().is_some() {
                if self
                    .design_ui
                    .palette_close
                    .is_some_and(|r| r.contains(pos))
                {
                    self.commands.close_palette();
                    return Ok(Some(Outcome::Continue));
                }
                if let Some((_, id)) = self
                    .design_ui
                    .palette_hits
                    .iter()
                    .find(|(r, _)| r.contains(pos))
                {
                    let id = id.clone();
                    self.commands.close_palette();
                    return self.execute_command(&id).map(Some);
                }
            } else if self.zen_mode()
                && self.prompt.is_none()
                && !self.import_active()
                && self.history_preview.is_none()
                && self.design_ui.zen_exit_hit.is_some_and(|r| r.contains(pos))
            {
                return self.execute_command("toggle-zen").map(Some);
            } else if self.prompt.is_none()
                && !self.import_active()
                && self.history_preview.is_none()
                && self.design_ui.heading_hit.is_some_and(|r| r.contains(pos))
            {
                return self.open_paragraph_style().map(Some);
            } else if self.prompt.is_none()
                && !self.import_active()
                && self.design_ui.command_hit.is_some_and(|r| r.contains(pos))
            {
                return self.execute_command("design-commands").map(Some);
            }
        }
        Ok(None)
    }
    pub(crate) fn render_command_entry(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if area.width < 4 || area.height == 0 {
            return;
        }
        let theme = self.theme();
        let label = if area.width >= 19 {
            Line::from(vec![
                Span::styled("› 命令  ", theme.accent),
                Span::styled("Ctrl+P", theme.accent),
                Span::styled(" · F1", theme.muted),
            ])
        } else if area.width >= 11 {
            Line::from(vec![
                Span::styled("命令 ", theme.accent),
                Span::styled("Ctrl+P", theme.accent),
            ])
        } else {
            Line::styled("命令", theme.accent)
        };
        f.render_widget(Paragraph::new(label), area);
        self.design_ui.command_hit = Some(area);
    }
}
