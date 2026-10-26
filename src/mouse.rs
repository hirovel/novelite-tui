use crate::{View, Workspace};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

#[derive(Default)]
pub(crate) struct Mouse {
    pub enabled: bool,
    area: Rect,
    dragging: bool,
    outside: Option<MouseEvent>,
    last_scroll: Option<std::time::Instant>,
    generation: u64,
}
impl Workspace {
    pub fn configure_mouse(&mut self, enabled: bool) {
        self.mouse.enabled = enabled;
        self.mouse.dragging = false;
        self.mouse.outside = None;
    }
    pub fn mouse_enabled(&self) -> bool {
        self.mouse.enabled
    }
    pub fn editor_view(&mut self, area: Rect) -> View {
        self.mouse.area = area;
        self.view(area.width, area.height)
    }
    pub(crate) fn handle_mouse(&mut self, event: MouseEvent) -> std::io::Result<()> {
        if self.mouse.enabled
            && let Some(prompt) = &mut self.prompt
            && let Some(editor) = &mut prompt.editor
        {
            let area = prompt.editor_area;
            let inside = area.contains(Position::new(event.column, event.row));
            if area.width > 0 && area.height > 0 {
                match event.kind {
                    MouseEventKind::Down(MouseButton::Left) if inside => {
                        prompt.dragging = true;
                        editor.point(event.column - area.x, event.row - area.y, false);
                    }
                    MouseEventKind::Drag(MouseButton::Left) if prompt.dragging => editor.point(
                        event.column.saturating_sub(area.x).min(area.width - 1),
                        event.row.saturating_sub(area.y).min(area.height - 1),
                        true,
                    ),
                    MouseEventKind::Up(MouseButton::Left) => prompt.dragging = false,
                    MouseEventKind::ScrollUp if inside => editor.scroll(-3, area.height),
                    MouseEventKind::ScrollDown if inside => editor.scroll(3, area.height),
                    _ => {}
                }
            }
            return Ok(());
        }
        if self.mouse.enabled && self.recent_mouse(event)? {
            self.cancel_mouse_drag();
            return Ok(());
        }
        if !self.mouse.enabled
            || self.prompt.is_some()
            || self.palette().is_some()
            || self.history_preview.is_some()
            || self
                .navigator
                .as_ref()
                .is_some_and(|n| n.panel_module().is_none())
        {
            self.cancel_mouse_drag();
            return Ok(());
        }
        if self.module_mouse(event)? || self.navigator.is_some() || self.book_search_focused() {
            self.cancel_mouse_drag();
            return Ok(());
        }
        if matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
        ) && let Some(area) = self.panes.areas[1 - self.active_pane()]
            && area.contains(Position::new(event.column, event.row))
        {
            self.focus_other_pane()?;
            self.mouse.area = area;
        }
        let area = self.mouse.area;
        if area.width == 0 || area.height == 0 {
            return Ok(());
        }
        let inside = area.contains(Position::new(event.column, event.row));
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if inside => {
                self.mouse.dragging = true;
                self.document
                    .point(event.column - area.x, event.row - area.y, false);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.mouse.dragging => {
                self.drag_mouse(event);
            }
            MouseEventKind::ScrollUp if inside => self.document.scroll(-3, area.height),
            MouseEventKind::ScrollDown if inside => self.document.scroll(3, area.height),
            MouseEventKind::Up(MouseButton::Left) => self.cancel_mouse_drag(),
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn invalidate_editor_geometry(&mut self) {
        self.cancel_mouse_drag();
        self.mouse.area = Rect::default();
        self.panes.areas = [None, None];
    }
    pub(crate) fn cancel_mouse_drag(&mut self) {
        self.mouse.dragging = false;
        self.mouse.outside = None;
    }
    fn drag_mouse(&mut self, event: MouseEvent) {
        let area = self.mouse.area;
        let direction = if event.row < area.y {
            -1
        } else if event.row >= area.bottom() {
            1
        } else {
            0
        };
        self.document.scroll(direction, area.height);
        self.document.point(
            event.column.saturating_sub(area.x).min(area.width),
            event
                .row
                .saturating_sub(area.y)
                .min(area.height.saturating_sub(1)),
            true,
        );
        self.mouse.outside = (direction != 0).then_some(event);
        self.mouse.last_scroll = Some(std::time::Instant::now());
        self.mouse.generation += 1;
    }
    pub(crate) fn tick_mouse(&mut self, now: std::time::Instant) {
        if let Some(event) = self.mouse.outside
            && self.mouse.last_scroll.is_none_or(|last| {
                now.saturating_duration_since(last) >= std::time::Duration::from_millis(80)
            })
        {
            self.drag_mouse(event);
            self.mouse.last_scroll = Some(now);
        }
    }
    pub fn mouse_generation(&self) -> u64 {
        self.mouse.generation
    }
}
pub(crate) fn commands() -> Vec<crate::Command> {
    vec![crate::Command {
        id: "toggle-mouse".into(),
        title: "切换应用鼠标 / 终端原生选区".into(),
        run: |work| {
            work.configure_mouse(!work.mouse_enabled());
            Ok(crate::Outcome::Continue)
        },
    }]
}
