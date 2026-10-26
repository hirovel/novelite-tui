use crate::{Command, Outcome, View, Workspace, document::Position};
use ratatui::layout::Rect;
use std::io;

pub struct PaneView {
    pub index: usize,
    pub active: bool,
    pub title: String,
    pub area: Rect,
    pub heading: Option<Rect>,
    pub view: View,
}
struct Pane {
    chapter: Option<String>,
    position: Position,
}
#[derive(Default)]
pub(crate) struct Panes {
    other: Option<Pane>,
    active: usize,
    message: Option<String>,
    pub(crate) stacked: bool,
    pub areas: [Option<Rect>; 2],
}
impl Workspace {
    fn chapter_id(&self) -> Option<String> {
        self.active_chapter().map(|c| c.id.clone())
    }
    pub fn pane_message(&self) -> Option<&str> {
        self.panes.message.as_deref()
    }
    pub fn is_split(&self) -> bool {
        self.panes.other.is_some()
    }
    /// A retained second pane can be temporarily hidden without closing its document.
    pub(crate) fn visible_split(&self) -> bool {
        self.is_split() && !self.zen_mode()
    }
    pub fn active_pane(&self) -> usize {
        self.panes.active
    }
    fn split(&mut self, stacked: bool) {
        self.panes.stacked = stacked;
        if self.panes.other.is_none() {
            self.panes.other = Some(Pane {
                chapter: self.chapter_id(),
                position: self.document.position(),
            });
        }
    }
    pub fn focus_other_pane(&mut self) -> io::Result<()> {
        self.cancel_mouse_drag();
        self.reconcile_panes();
        let Some(other) = self.panes.other.take() else {
            return Ok(());
        };
        let previous = Pane {
            chapter: self.chapter_id(),
            position: self.document.position(),
        };
        let mut result = Ok(());
        if let Some(id) = &other.chapter
            && other.chapter != previous.chapter
            && let Err(error) = self.switch_chapter(id)
        {
            if self.chapter_id() == previous.chapter {
                self.panes.other = Some(other);
                return Err(error);
            }
            result = Err(error);
        }
        self.document.restore_position(&other.position);
        self.panes.other = Some(previous);
        self.panes.active = 1 - self.panes.active;
        self.clear_history_view();
        self.navigator = None;
        result
    }
    pub(crate) fn reconcile_panes(&mut self) {
        if let Some(other) = &self.panes.other
            && let Some(id) = &other.chapter
            && self
                .project
                .as_ref()
                .is_none_or(|store| store.chapter(id).is_none())
        {
            self.panes.other = None;
            self.panes.active = 0;
            self.panes.areas = [None, None];
            self.panes.message =
                Some("对照章节已移出目录，已收起对应窗格；文稿仍由作品保留".into());
        }
    }
    pub(crate) fn map_other_pane(&mut self) {
        let chapter = self.chapter_id();
        if let Some(other) = &mut self.panes.other
            && other.chapter == chapter
        {
            self.document.map_position(&mut other.position);
        }
    }
    pub(crate) fn announce_history_target(&mut self, redo: bool) {
        let title = self.active_chapter().map_or("当前文稿", |c| &c.title);
        self.panes.message = Some(format!(
            "{}「{title}」的最近事务（同章窗格共用历史）",
            if redo { "重做" } else { "撤销" }
        ));
    }
    pub fn pane_views(&mut self, area: Rect) -> Vec<PaneView> {
        self.reconcile_panes();
        let split = self.visible_split();
        let fits = split && self.split_fits(area);
        let (first, second) = if fits && self.panes.stacked {
            let height = (area.height - 1) / 2;
            (
                Rect::new(area.x, area.y, area.width, height),
                Rect::new(
                    area.x,
                    area.y + height + 1,
                    area.width,
                    area.height - height - 1,
                ),
            )
        } else if fits {
            let width = (area.width - 1) / 2;
            // Put the spare cell in the gutter. Equal text widths align wrapping
            // and let two views of one chapter share its expensive layout.
            (
                Rect::new(area.x, area.y, width, area.height),
                Rect::new(area.x + area.width - width, area.y, width, area.height),
            )
        } else {
            (area, area)
        };
        let frame = if self.panes.active == 0 {
            first
        } else {
            second
        };
        let (heading, active_area) = content_area(frame, split);
        let title = self
            .active_chapter()
            .map_or("文稿", |c| &c.title)
            .to_owned();
        let chapter = self.chapter_id();
        let mut view = self.editor_view(active_area);
        self.document
            .mark_chapter_title(&mut view, self.active_chapter().map(|c| c.title.as_str()));
        self.panes.areas = [None, None];
        self.panes.areas[self.panes.active] = Some(active_area);
        let active_title = if split && !fits {
            format!(
                "{title} · 另一窗格隐藏 / {} 切换",
                self.command_shortcuts("focus-other-pane")
            )
        } else {
            title.clone()
        };
        let mut views = vec![PaneView {
            index: self.panes.active,
            active: true,
            title: active_title,
            heading,
            area: active_area,
            view,
        }];
        let settings = self.immersion_settings();
        if fits && let Some(other) = &mut self.panes.other {
            let (heading, other_area) = content_area(
                if self.panes.active == 0 {
                    second
                } else {
                    first
                },
                true,
            );
            let document = if other.chapter == chapter {
                Some(&mut self.document)
            } else {
                other
                    .chapter
                    .as_ref()
                    .and_then(|id| self.chapters.get_mut(id))
            };
            if let Some(document) = document {
                let stored = document.position();
                document.restore_position(&other.position);
                let mut view = document.writing_view(other_area.width, other_area.height, settings);
                document.mark_chapter_title(
                    &mut view,
                    other
                        .chapter
                        .as_ref()
                        .and_then(|id| self.project.as_ref()?.chapter(id))
                        .map(|c| c.title.as_str()),
                );
                other.position = document.position();
                document.restore_position(&stored);
                let title = other
                    .chapter
                    .as_ref()
                    .and_then(|id| self.project.as_ref()?.chapter(id))
                    .map_or(title, |c| c.title.clone());
                self.panes.areas[1 - self.panes.active] = Some(other_area);
                views.push(PaneView {
                    index: 1 - self.panes.active,
                    active: false,
                    title,
                    heading,
                    area: other_area,
                    view,
                });
            }
        }
        views.sort_by_key(|p| p.index);
        views
    }
    pub(crate) fn split_fits(&self, area: Rect) -> bool {
        if self.panes.stacked {
            area.width >= 40 && area.height >= 23
        } else {
            area.width >= 81 && area.height >= 11
        }
    }
}
fn content_area(frame: Rect, split: bool) -> (Option<Rect>, Rect) {
    if !split || frame.height == 0 {
        return (None, frame);
    }
    (
        Some(Rect::new(frame.x, frame.y, frame.width, 1)),
        Rect::new(frame.x, frame.y + 1, frame.width, frame.height - 1),
    )
}
pub(crate) fn commands() -> Vec<Command> {
    vec![
        Command {
            id: "split-right".into(),
            title: "左右双窗格".into(),
            run: |w| {
                w.split(false);
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "split-down".into(),
            title: "上下双窗格".into(),
            run: |w| {
                w.split(true);
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "focus-other-pane".into(),
            title: "切换编辑窗格".into(),
            run: |w| {
                w.focus_other_pane()?;
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "swap-panes".into(),
            title: "交换窗格位置（焦点跟随文稿）".into(),
            run: |w| {
                if w.is_split() {
                    w.panes.active = 1 - w.panes.active;
                }
                Ok(Outcome::Continue)
            },
        },
        Command {
            id: "close-pane".into(),
            title: "关闭当前窗格（文稿与历史保留）".into(),
            run: |w| {
                w.focus_other_pane()?;
                w.panes.other = None;
                w.panes.active = 0;
                Ok(Outcome::Continue)
            },
        },
    ]
}
