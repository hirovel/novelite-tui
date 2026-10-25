// Terminal output policy: explicit user choice overrides inherited defaults.
// This adapter is called by the native terminal runner, not by buffer renderers.
use crate::{ColorMode, Workspace};
pub fn detect_color_support() -> ColorMode {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return ColorMode::Monochrome;
    }
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if term == "dumb" {
        return ColorMode::Monochrome;
    }
    let colorterm = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Windows Terminal does not have to set COLORTERM. Its session marker is enough.
    if std::env::var_os("WT_SESSION").is_some_and(|v| !v.is_empty())
        || matches!(colorterm.as_str(), "truecolor" | "24bit")
        || matches!(term.as_str(), "xterm-kitty" | "wezterm")
        || matches!(program.as_str(), "wezterm" | "kitty")
    {
        ColorMode::TrueColor
    } else {
        ColorMode::Basic
    }
}
impl Workspace {
    pub fn effective_color_mode(&self) -> ColorMode {
        match self.modules.config.appearance.colors {
            ColorMode::Auto => self.color_support,
            explicit => explicit,
        }
    }
    pub fn prepare_terminal_output(&self) -> ColorMode {
        let mode = self.effective_color_mode();
        crossterm::style::force_color_output(mode != ColorMode::Monochrome);
        mode
    }
    pub fn color_mode_label(&self) -> &'static str {
        match self.effective_color_mode() {
            ColorMode::TrueColor => "真彩色",
            ColorMode::Monochrome => "单色",
            _ => "基础色",
        }
    }
}
