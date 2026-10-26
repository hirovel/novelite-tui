use crate::{
    Action, Command, Direction, Document, HistoryPolicy, Outcome, View, Workspace, navigator::Kind,
    prompt::Purpose,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use std::io;

pub(crate) struct Preview {
    id: String,
    document: Document,
}
pub(crate) fn timestamp(seconds: u64) -> String {
    i64::try_from(seconds)
        .ok()
        .and_then(|s| time::OffsetDateTime::from_unix_timestamp(s).ok())
        .map(|date| {
            format!(
                "{} {:02}:{:02}:{:02} UTC",
                date.date(),
                date.hour(),
                date.minute(),
                date.second()
            )
        })
        .unwrap_or_else(|| "时间超出显示范围".into())
}
impl Workspace {
    pub(crate) fn clear_history_view(&mut self) {
        self.history_preview = None;
        if self
            .navigator
            .as_ref()
            .is_some_and(|navigator| navigator.is_history())
        {
            self.navigator = None;
        }
    }
    pub(crate) fn open_history_preview(&mut self, id: &str) -> io::Result<()> {
        self.navigation_ui.clear_hits();
        self.invalidate_editor_geometry();
        let text = self.preview_snapshot(id)?;
        self.history_preview = Some(Preview {
            id: id.into(),
            document: Document::new(text, false),
        });
        Ok(())
    }
    pub(crate) fn history_preview_id(&self) -> Option<&str> {
        self.history_preview.as_ref().map(|p| p.id.as_str())
    }
    pub(crate) fn scroll_history_preview(&mut self, delta: isize, height: u16) {
        if let Some(preview) = &mut self.history_preview {
            preview.document.scroll(delta, height);
        }
    }
    pub fn history_preview(&mut self, width: u16, height: u16) -> Option<View> {
        self.history_preview
            .as_mut()
            .map(|preview| preview.document.view(width, height))
    }
    pub(crate) fn history_preview_event(&mut self, event: Event) -> io::Result<Outcome> {
        if let Event::Key(key) = event
            && key.kind != KeyEventKind::Release
        {
            match key.code {
                KeyCode::Esc => {
                    self.history_preview = None;
                    self.navigation_ui.clear_hits();
                    self.invalidate_editor_geometry();
                }
                KeyCode::Enter => {
                    let id = self.history_preview.as_ref().unwrap().id.clone();
                    self.restore_snapshot(&id)?;
                    self.history_preview = None;
                    self.navigation_ui.clear_hits();
                    self.invalidate_editor_geometry();
                }
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End => {
                    let direction = match key.code {
                        KeyCode::Up => Direction::Up,
                        KeyCode::Down => Direction::Down,
                        KeyCode::Left => Direction::Left,
                        KeyCode::Right => Direction::Right,
                        KeyCode::Home => Direction::Home,
                        _ => Direction::End,
                    };
                    self.history_preview
                        .as_mut()
                        .unwrap()
                        .document
                        .apply(Action::Move(direction));
                }
                _ => {}
            }
        }
        self.restore_popup_if_closed();
        Ok(Outcome::Continue)
    }
    pub(crate) fn apply_history_settings(&mut self, text: &str) -> io::Result<()> {
        let parts: Vec<u64> = text
            .split(',')
            .map(|part| part.trim().parse().map_err(io::Error::other))
            .collect::<io::Result<_>>()?;
        if parts.len() != 3 {
            return Err(io::Error::other("输入：近期点数,保留天数,每章容量MiB"));
        }
        self.set_history_policy(HistoryPolicy {
            recent: usize::try_from(parts[0]).map_err(io::Error::other)?,
            days: parts[1],
            max_bytes: parts[2]
                .checked_mul(1024 * 1024)
                .ok_or_else(|| io::Error::other("容量超出范围"))?,
        })
    }
}
pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "history".into(),
            title: "历史快照 / 预览与恢复".into(),
            run: |w| {
                let entries = w.history_entries()?;
                let usage = w.history_usage()?;
                let capacity = w.history_policy()?.max_bytes;
                w.history_preview = None;
                w.show_navigation(Kind::History {
                    entries,
                    usage,
                    capacity,
                })
            },
        },
        Command {
            id: "mark-snapshot".into(),
            title: "手动标记当前恢复点（永久保留）".into(),
            run: |w| {
                w.mark_snapshot()?;
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "history-settings".into(),
            title: "配置历史保留策略与容量".into(),
            run: |w| {
                w.history_policy()?;
                w.begin_prompt(Purpose::HistoryPolicy)
            },
        },
    ]
}
