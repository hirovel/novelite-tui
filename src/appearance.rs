use crate::{Command, Outcome, Workspace};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisualStyle {
    #[default]
    Reading,
    Terminal,
    Compact,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMode {
    #[default]
    Auto,
    Basic,
    Monochrome,
    TrueColor,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Background {
    #[default]
    Terminal,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorTheme {
    #[default]
    Night,
    Forest,
    Paper,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextWidth {
    #[default]
    Adaptive,
    Wide,
    Full,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Brightness {
    Soft,
    #[default]
    Normal,
    Bright,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Material {
    #[default]
    Solid,
    Terminal,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Appearance {
    pub style: VisualStyle,
    #[serde(default)]
    pub theme: ColorTheme,
    #[serde(default)]
    pub text_width: TextWidth,
    #[serde(default)]
    pub density: Density,
    #[serde(default)]
    pub motion: Option<bool>,
    #[serde(default)]
    pub brightness: Brightness,
    #[serde(default)]
    pub material: Material,
    #[serde(default)]
    pub colors: ColorMode,
    #[serde(default)]
    pub background: Background,
}

pub(crate) struct Theme {
    pub base: Style,
    pub accent: Style,
    pub dialogue: Style,
    pub warning: Style,
    pub border: Style,
    pub muted: Style,
    pub surface: Style,
    pub selection: Style,
}

impl Workspace {
    pub fn set_brightness(&mut self, value: Brightness) -> io::Result<()> {
        self.update_appearance(|a| a.brightness = value)
    }
    pub fn set_material(&mut self, value: Material) -> io::Result<()> {
        self.update_appearance(|a| a.material = value)
    }
    pub fn configure_color_support(&mut self, support: ColorMode) {
        self.color_support = support;
    }
    pub fn set_color_mode(&mut self, colors: ColorMode) -> io::Result<()> {
        self.update_appearance(|a| a.colors = colors)
    }
    pub fn set_background(&mut self, background: Background) -> io::Result<()> {
        self.update_appearance(|a| {
            a.background = background;
            match background {
                Background::Light => a.theme = ColorTheme::Paper,
                Background::Dark if a.theme == ColorTheme::Paper => a.theme = ColorTheme::Night,
                _ => {}
            }
        })
    }
    pub(crate) fn update_appearance(
        &mut self,
        update: impl FnOnce(&mut Appearance),
    ) -> io::Result<()> {
        let mut candidate = self.modules.config.clone();
        update(&mut candidate.appearance);
        candidate.persist(self.modules.path.as_deref())?;
        self.modules.config = candidate;
        Ok(())
    }
    pub(crate) fn theme(&self) -> Theme {
        let appearance = &self.modules.config.appearance;
        let colors = self.effective_color_mode();
        let base = Style::default().fg(Color::Reset).bg(Color::Reset);
        if colors == ColorMode::Monochrome {
            return Theme {
                base,
                accent: base.add_modifier(Modifier::BOLD),
                dialogue: base.add_modifier(Modifier::UNDERLINED),
                warning: base.add_modifier(Modifier::BOLD),
                border: base,
                muted: base,
                surface: base,
                selection: base.add_modifier(Modifier::REVERSED),
            };
        }
        let true_color =
            colors == ColorMode::TrueColor && appearance.background != Background::Terminal;
        let dark = appearance.background != Background::Light;
        let palette = match appearance.theme {
            ColorTheme::Night => [
                (204, 211, 225),
                (18, 20, 29),
                (134, 148, 169),
                (25, 29, 42),
                (125, 174, 236),
                (48, 58, 77),
                (40, 49, 70),
                (141, 191, 173),
            ],
            ColorTheme::Forest => [
                (221, 218, 207),
                (27, 33, 30),
                (155, 172, 157),
                (35, 43, 38),
                (215, 190, 140),
                (64, 80, 68),
                (57, 73, 58),
                (131, 192, 174),
            ],
            ColorTheme::Paper => [
                (49, 57, 70),
                (246, 243, 235),
                (104, 111, 125),
                (236, 231, 218),
                (91, 74, 151),
                (200, 192, 177),
                (216, 208, 232),
                (36, 117, 103),
            ],
        };
        let palette = palette.map(|(r, g, b)| {
            let factor = match appearance.brightness {
                Brightness::Soft => 0.88,
                Brightness::Normal => 1.0,
                Brightness::Bright => 1.12,
            };
            let scale = |c: u8| {
                if !dark && appearance.brightness == Brightness::Bright {
                    (f32::from(c) + (255.0 - f32::from(c)) * 0.18).round() as u8
                } else {
                    (f32::from(c) * factor).round().min(255.0) as u8
                }
            };
            (scale(r), scale(g), scale(b))
        });
        let rgb = |dark_rgb: (u8, u8, u8), light_rgb: (u8, u8, u8)| {
            let (r, g, b) = if dark { dark_rgb } else { light_rgb };
            Color::Rgb(r, g, b)
        };
        let base = if true_color {
            base.fg(rgb(palette[0], palette[0]))
                .bg(rgb(palette[1], palette[1]))
        } else {
            match appearance.background {
                Background::Terminal => base,
                Background::Dark => base.fg(Color::White).bg(Color::Black),
                Background::Light => base.fg(Color::Black).bg(Color::White),
            }
        };
        let base = if appearance.material == Material::Terminal {
            base.fg(Color::Reset).bg(Color::Reset)
        } else {
            base
        };
        let muted = if appearance.material == Material::Terminal {
            base.add_modifier(Modifier::DIM)
        } else if true_color {
            base.fg(rgb(palette[2], palette[2]))
        } else {
            base
        };
        let surface = if true_color && appearance.material == Material::Solid {
            base.bg(rgb(palette[3], palette[3]))
        } else {
            base
        };
        let accent = if true_color {
            base.fg(rgb(palette[4], palette[4]))
        } else {
            base.fg(Color::Cyan)
        };
        let muted = Style { bg: None, ..muted };
        let accent = Style { bg: None, ..accent };
        Theme {
            base,
            muted,
            surface,
            accent: accent.add_modifier(Modifier::BOLD),
            dialogue: Style::default().fg(if true_color {
                rgb(palette[7], palette[7])
            } else {
                Color::Green
            }),
            warning: if true_color {
                base.fg(rgb((235, 175, 143), (145, 53, 35)))
            } else {
                base.add_modifier(Modifier::BOLD)
            },
            border: if true_color {
                surface.fg(rgb(palette[5], palette[5]))
            } else {
                base
            },
            selection: if true_color && appearance.material == Material::Solid {
                base.bg(rgb(palette[6], palette[6]))
            } else {
                base.add_modifier(Modifier::REVERSED)
            },
        }
    }

    /// Restore host-owned colors without changing writing layout or content.
    pub fn use_terminal_appearance(&mut self) -> io::Result<()> {
        self.update_appearance(|a| {
            a.material = Material::Terminal;
            a.background = Background::Terminal;
            a.colors = ColorMode::Auto;
        })
    }

    fn use_custom_theme(&mut self, theme: ColorTheme) -> io::Result<()> {
        self.update_appearance(|a| {
            a.material = Material::Solid;
            a.theme = theme;
            a.colors = ColorMode::TrueColor;
            a.background = if theme == ColorTheme::Paper {
                Background::Light
            } else {
                Background::Dark
            };
        })
    }

    pub fn set_color_theme(&mut self, theme: ColorTheme) -> io::Result<()> {
        self.update_appearance(|a| {
            a.theme = theme;
            a.colors = ColorMode::TrueColor;
            a.background = if theme == ColorTheme::Paper {
                Background::Light
            } else {
                Background::Dark
            };
        })
    }
    pub fn set_text_width(&mut self, width: TextWidth) -> io::Result<()> {
        self.update_appearance(|a| a.text_width = width)
    }
    pub fn set_density(&mut self, density: Density) -> io::Result<()> {
        self.update_appearance(|a| a.density = density)
    }
    pub fn visual_style(&self) -> VisualStyle {
        self.modules.config.appearance.style
    }
    pub fn set_visual_style(&mut self, style: VisualStyle) -> io::Result<()> {
        self.update_appearance(|a| a.style = style)
    }
    pub fn editor_area(&self, area: Rect) -> Rect {
        let (horizontal, vertical) = match self.visual_style() {
            VisualStyle::Reading => match self.modules.config.appearance.density {
                Density::Compact => (0, 0),
                Density::Comfortable => (2, 1),
                Density::Spacious => (4, 2),
            },
            VisualStyle::Terminal => (1, 0),
            VisualStyle::Compact => (0, 0),
        };
        // Keep split thresholds and small terminals usable; decoration yields first.
        let minimum = if self.is_split() { 81 } else { 40 };
        let horizontal = if area.width >= minimum + horizontal * 2 {
            horizontal
        } else {
            0
        };
        let vertical = (area.height.saturating_sub(10) / 8).min(vertical * 2);
        let width = area.width.saturating_sub(horizontal * 2);
        let width = if self.visual_style() == VisualStyle::Reading && !self.is_split() {
            match self.modules.config.appearance.text_width {
                TextWidth::Adaptive => (width * 4 / 5).clamp(40, 92).min(width),
                TextWidth::Wide => width.min(110),
                TextWidth::Full => width,
            }
        } else {
            width
        };
        let padded = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + vertical,
            width,
            area.height.saturating_sub(vertical * 2),
        );
        if self.is_split() && self.split_fits(area) && !self.split_fits(padded) {
            area
        } else {
            padded
        }
    }
}

pub(crate) fn commands() -> Vec<Command> {
    type Run = fn(&mut Workspace) -> io::Result<Outcome>;
    fn set(w: &mut Workspace, style: VisualStyle) -> io::Result<Outcome> {
        w.set_visual_style(style)?;
        Ok(Outcome::Continue)
    }
    let entries: &[(&str, &str, Run)] = &[
        (
            "terminal-defaults",
            "外观：恢复终端配色与背景",
            |w| {
                w.use_terminal_appearance()?;
                Ok(Outcome::Continue)
            },
        ),
        ("theme-night", "自定义主题：星夜", |w| {
            w.use_custom_theme(ColorTheme::Night)?;
            Ok(Outcome::Continue)
        }),
        ("theme-forest", "自定义主题：松林", |w| {
            w.use_custom_theme(ColorTheme::Forest)?;
            Ok(Outcome::Continue)
        }),
        ("theme-paper", "自定义主题：纸页", |w| {
            w.use_custom_theme(ColorTheme::Paper)?;
            Ok(Outcome::Continue)
        }),
        ("style-reading", "视觉风格：阅读感", |w| {
            set(w, VisualStyle::Reading)
        }),
        ("style-terminal", "视觉风格：跟随终端", |w| {
            set(w, VisualStyle::Terminal)
        }),
        ("style-compact", "视觉风格：紧凑工作台", |w| {
            set(w, VisualStyle::Compact)
        }),
        ("color-auto", "颜色：自动能力降级", |w| {
            w.set_color_mode(ColorMode::Auto)?;
            Ok(Outcome::Continue)
        }),
        ("color-basic", "颜色：基础色", |w| {
            w.set_color_mode(ColorMode::Basic)?;
            Ok(Outcome::Continue)
        }),
        ("color-mono", "颜色：单色", |w| {
            w.set_color_mode(ColorMode::Monochrome)?;
            Ok(Outcome::Continue)
        }),
        ("color-truecolor", "颜色：真彩色", |w| {
            w.set_color_mode(ColorMode::TrueColor)?;
            Ok(Outcome::Continue)
        }),
        ("background-terminal", "背景：跟随终端", |w| {
            w.set_background(Background::Terminal)?;
            Ok(Outcome::Continue)
        }),
        ("background-light", "背景：浅色", |w| {
            w.set_background(Background::Light)?;
            Ok(Outcome::Continue)
        }),
        ("background-dark", "背景：深色", |w| {
            w.set_background(Background::Dark)?;
            Ok(Outcome::Continue)
        }),
    ];
    entries
        .iter()
        .map(|(id, title, run)| Command {
            id: (*id).into(),
            title: (*title).into(),
            run: *run,
        })
        .collect()
}
