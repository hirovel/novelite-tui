// Color-only UI transitions. Text and hit rectangles are final immediately.
use crate::{Background, ColorMode, Workspace};
use ratatui::{buffer::Buffer, layout::Position, style::Color};
use std::time::{Duration, Instant};
#[derive(PartialEq, Eq)]
pub struct Scene {
    revision: u64,
    settings: bool,
    zen: bool,
    palette: Option<usize>,
    navigator: bool,
    prompt: bool,
    pane: usize,
    chapters: bool,
    planning: bool,
    search: bool,
}
const DURATION: Duration = Duration::from_millis(220);
pub(crate) struct Motion {
    enabled: bool,
    last: Option<Buffer>,
    source: Option<(Buffer, Instant, Vec<ratatui::layout::Rect>)>,
    last_regions: Vec<ratatui::layout::Rect>,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            enabled: std::env::var_os("NOVELITE_REDUCED_MOTION").is_none(),
            last: None,
            source: None,
            last_regions: Vec::new(),
        }
    }
}
impl Motion {
    fn clear_history(&mut self) {
        self.source = None;
        self.last = None;
        self.last_regions.clear();
    }
}
impl Workspace {
    pub fn design_motion_enabled(&self) -> bool {
        self.design_motion.enabled
    }
    pub fn set_design_motion(&mut self, enabled: bool) {
        self.design_motion.enabled = enabled;
        if !enabled {
            self.design_motion.clear_history();
        }
    }
    fn color_motion_available(&self) -> bool {
        self.design_motion.enabled
            && self.effective_color_mode() == ColorMode::TrueColor
            && self.modules.config.appearance.background != Background::Terminal
    }
    pub(crate) fn reset_design_transition(&mut self) {
        self.design_motion.clear_history();
    }
    pub fn cancel_design_transition(&mut self) {
        self.design_motion.source = None;
    }
    pub fn begin_design_transition(&mut self, now: Instant) {
        if !self.color_motion_available() {
            self.design_motion.clear_history();
            return;
        }
        self.design_motion.source = self
            .design_motion
            .last
            .as_ref()
            .map(|last| (last.clone(), now, self.design_motion.last_regions.clone()));
    }
    pub fn design_needs_frame(&self) -> bool {
        self.design_motion.source.is_some()
    }
    pub fn design_scene(&self) -> Scene {
        Scene {
            revision: self.design_ui.revision,
            settings: self.design_ui.open,
            zen: self.zen_mode(),
            palette: self.commands.palette_selection(),
            navigator: self.navigator.is_some(),
            prompt: self.prompt.is_some(),
            pane: self.active_pane(),
            chapters: self.module_focused(crate::ModuleId::Chapters),
            planning: self.module_focused(crate::ModuleId::Planning),
            search: self.book_search_focused(),
        }
    }
    pub fn render_design(&mut self, frame: &mut ratatui::Frame<'_>, message: &str, now: Instant) {
        self.render(frame, message);
        if !self.color_motion_available() {
            self.design_motion.clear_history();
            return;
        }
        let buffer = frame.buffer_mut();
        if let Some((previous, start, previous_regions)) = &self.design_motion.source {
            let elapsed = now.saturating_duration_since(*start);
            if elapsed >= DURATION || previous.area != buffer.area {
                self.design_motion.source = None;
            } else {
                let t = elapsed.as_secs_f32() / DURATION.as_secs_f32();
                let ease = 1.0 - (1.0 - t).powi(3);
                let width = buffer.area.width as usize;
                let theme_changed =
                    previous.content.first().map(|c| c.bg) != buffer.content.first().map(|c| c.bg);
                let mut changing_colors = false;
                for (index, cell) in buffer.content.iter_mut().enumerate() {
                    if width == 0 {
                        break;
                    }
                    let point = Position::new(
                        buffer.area.x + (index % width) as u16,
                        buffer.area.y + (index / width) as u16,
                    );
                    if !theme_changed
                        && !self
                            .design_effect_regions
                            .iter()
                            .chain(previous_regions.iter())
                            .any(|r| r.contains(point))
                    {
                        continue;
                    }
                    let old = &previous.content[index];
                    let fg_from = if cell.bg == Color::Reset && old.fg != cell.fg {
                        match cell.fg {
                            Color::Rgb(r, g, b) => {
                                let gray =
                                    (u16::from(r) * 3 + u16::from(g) * 6 + u16::from(b)) / 10;
                                Color::Rgb(gray as u8, gray as u8, gray as u8)
                            }
                            _ => old.fg,
                        }
                    } else if old.symbol() == cell.symbol() {
                        old.fg
                    } else {
                        blend(cell.bg, cell.fg, 0.25)
                    };
                    changing_colors |= can_blend(fg_from, cell.fg) || can_blend(old.bg, cell.bg);
                    cell.fg = blend(fg_from, cell.fg, ease);
                    cell.bg = blend(old.bg, cell.bg, ease);
                }
                if !changing_colors {
                    self.design_motion.source = None;
                }
            }
        }
        self.design_motion.last = Some(buffer.clone());
        self.design_motion.last_regions = self.design_effect_regions.clone();
    }
}
fn can_blend(from: Color, to: Color) -> bool {
    from != to && matches!((from, to), (Color::Rgb(..), Color::Rgb(..)))
}
fn blend(from: Color, to: Color, t: f32) -> Color {
    match (from, to) {
        (Color::Rgb(a, b, c), Color::Rgb(x, y, z)) => {
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            Color::Rgb(mix(a, x), mix(b, y), mix(c, z))
        }
        _ => to,
    }
}
