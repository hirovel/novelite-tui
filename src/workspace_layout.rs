use crate::{
    ModuleId, Workspace,
    storage::{WriteMode, atomic_write},
};
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPreferences {
    pub pinned: bool,
    pub width: u16,
    pub side: PanelSide,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModuleSettings {
    pub enabled: bool,
    pub panel: PanelPreferences,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayoutFile {
    version: u32,
    #[serde(default)]
    pub appearance: crate::appearance::Appearance,
    #[serde(default)]
    pub typography: crate::typography::Settings,
    #[serde(default)]
    pub immersion: crate::immersion::Settings,
    #[serde(default = "import_enabled")]
    importing: bool,
    #[serde(default = "import_enabled")]
    exporting: bool,
    chapters: ModuleSettings,
    search: ModuleSettings,
    #[serde(default = "planning_settings")]
    planning: ModuleSettings,
}
impl Default for LayoutFile {
    fn default() -> Self {
        Self {
            version: 1,
            planning: planning_settings(),
            appearance: Default::default(),
            typography: Default::default(),
            immersion: Default::default(),
            importing: true,
            exporting: true,
            chapters: ModuleSettings {
                enabled: true,
                panel: PanelPreferences {
                    pinned: false,
                    width: 24,
                    side: PanelSide::Left,
                },
            },
            search: ModuleSettings {
                enabled: true,
                panel: PanelPreferences {
                    pinned: false,
                    width: 32,
                    side: PanelSide::Right,
                },
            },
        }
    }
}
impl LayoutFile {
    pub fn enabled(&self, id: ModuleId) -> bool {
        match id {
            ModuleId::Typography => self.typography.enabled,
            ModuleId::Immersion => self.immersion.enabled,
            ModuleId::Import => self.importing,
            ModuleId::Export => self.exporting,
            _ => self.settings(id).unwrap().enabled,
        }
    }
    pub fn set_enabled(&mut self, id: ModuleId, enabled: bool) {
        if let Some(settings) = self.settings_mut(id) {
            settings.enabled = enabled;
        } else {
            match id {
                ModuleId::Typography => self.typography.enabled = enabled,
                ModuleId::Immersion => self.immersion.enabled = enabled,
                ModuleId::Import => self.importing = enabled,
                ModuleId::Export => self.exporting = enabled,
                _ => unreachable!(),
            }
        }
    }
    pub fn settings(&self, id: ModuleId) -> Option<&ModuleSettings> {
        Some(match id {
            ModuleId::Chapters => &self.chapters,
            ModuleId::Search => &self.search,
            ModuleId::Planning => &self.planning,
            ModuleId::Typography | ModuleId::Immersion | ModuleId::Import | ModuleId::Export => {
                return None;
            }
        })
    }
    pub fn settings_mut(&mut self, id: ModuleId) -> Option<&mut ModuleSettings> {
        Some(match id {
            ModuleId::Chapters => &mut self.chapters,
            ModuleId::Search => &mut self.search,
            ModuleId::Planning => &mut self.planning,
            ModuleId::Typography | ModuleId::Immersion | ModuleId::Import | ModuleId::Export => {
                return None;
            }
        })
    }
    pub fn persist(&self, path: Option<&Path>) -> io::Result<()> {
        if let Some(path) = path {
            fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| io::Error::other("配置路径缺少父目录"))?,
            )?;
            atomic_write(path, &serde_json::to_vec_pretty(self)?, WriteMode::Replace)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct PanelPlacement {
    pub id: ModuleId,
    pub area: Rect,
    pub focused: bool,
    pub overlay: bool,
}
#[derive(Clone, Default)]
pub struct WorkspaceLayout {
    pub editor: Rect,
    pub panels: Vec<PanelPlacement>,
}

impl Workspace {
    pub fn panel_preferences(&self, id: ModuleId) -> Option<PanelPreferences> {
        self.modules.config.settings(id).map(|s| s.panel)
    }
    pub fn configure_panel(&mut self, id: ModuleId, panel: PanelPreferences) -> io::Result<()> {
        validate_panel(panel)?;
        let mut candidate = self.modules.config.clone();
        candidate
            .settings_mut(id)
            .ok_or_else(|| io::Error::other("该模块没有面板"))?
            .panel = panel;
        candidate.persist(self.modules.path.as_deref())?;
        self.modules.config = candidate;
        Ok(())
    }
    pub fn load_layout(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = crate::keymap::resolve_destination(path.as_ref())?;
        if path == self.path
            || self.library.path() == Some(path.as_path())
            || self.commands.keymap_path() == Some(path.as_path())
            || self
                .project
                .as_ref()
                .is_some_and(|p| path.starts_with(&p.root))
        {
            return Err(io::Error::other("布局配置必须与文稿、作品和其他配置分开"));
        }
        let config: LayoutFile = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => LayoutFile::default(),
            Err(e) => return Err(e),
        };
        if config.version != 1 {
            return Err(io::Error::other("不支持的布局版本"));
        }
        for id in ModuleId::ALL {
            if let Some(settings) = config.settings(id) {
                validate_panel(settings.panel)?;
            }
        }
        // Reading a configuration never rewrites it or the previously loaded file.
        self.modules.path = None;
        for id in ModuleId::ALL {
            self.set_module_enabled(id, config.enabled(id))?;
        }
        self.modules.config = config;
        if let Some(enabled) = self.modules.config.appearance.motion {
            self.set_design_motion(
                enabled && std::env::var_os("NOVELITE_REDUCED_MOTION").is_none(),
            );
        }
        self.modules.path = Some(path);
        Ok(())
    }
    pub(crate) fn protect_layout(&self, destination: &Path, project: bool) -> io::Result<()> {
        if let Some(path) = &self.modules.path {
            let destination = crate::keymap::resolve_destination(destination)?;
            if *path == destination || project && path.starts_with(destination) {
                return Err(io::Error::other("目标与布局配置冲突"));
            }
        }
        Ok(())
    }
    pub fn workspace_layout(&mut self, area: Rect) -> WorkspaceLayout {
        let mut layout = WorkspaceLayout {
            editor: area,
            panels: vec![],
        };
        for id in ModuleId::ALL {
            if !self.module_enabled(id) {
                continue;
            }
            let Some(panel) = self.panel_preferences(id) else {
                continue;
            };
            let focused = self.module_focused(id);
            if self.zen_mode() && !focused {
                continue;
            }
            if !self.zen_mode()
                && panel.pinned
                && area.height >= 10
                && layout.editor.width >= panel.width + 41
            {
                let x = match panel.side {
                    PanelSide::Left => layout.editor.x,
                    PanelSide::Right => layout.editor.right() - panel.width,
                };
                layout.panels.push(PanelPlacement {
                    id,
                    area: Rect::new(x, area.y, panel.width, area.height),
                    focused,
                    overlay: false,
                });
                layout.editor.width -= panel.width + 1;
                if panel.side == PanelSide::Left {
                    layout.editor.x += panel.width + 1;
                }
            } else if focused {
                layout.panels.push(PanelPlacement {
                    id,
                    area,
                    focused,
                    overlay: true,
                });
            }
        }
        // Back-to-front order is shared by rendering and reverse hit testing.
        layout.panels.sort_by_key(|p| p.overlay);
        self.modules.layout = layout.clone();
        layout
    }
}
fn validate_panel(panel: PanelPreferences) -> io::Result<()> {
    if !(18..=60).contains(&panel.width) {
        return Err(io::Error::other("面板宽度应为 18–60 列"));
    }
    Ok(())
}

fn planning_settings() -> ModuleSettings {
    ModuleSettings {
        enabled: true,
        panel: PanelPreferences {
            pinned: false,
            width: 32,
            side: PanelSide::Right,
        },
    }
}

fn import_enabled() -> bool {
    true
}

// One geometry contract for panel drawing and mouse input.
pub(crate) fn panel_content_rect(area: Rect) -> Rect {
    let x = if area.width >= 22 { 2 } else { 1 };
    let y = if area.height >= 12 { 2 } else { 1 };
    Rect::new(
        area.x + x,
        area.y + y,
        area.width.saturating_sub(x * 2),
        area.height.saturating_sub(y * 2),
    )
}
