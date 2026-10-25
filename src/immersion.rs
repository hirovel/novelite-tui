use crate::{Command, Outcome, Workspace};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Focus {
    #[default]
    Off,
    Paragraph,
    Sentence,
    Nearby,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub focus: Focus,
    #[serde(default)]
    pub dialogue: bool,
    #[serde(default)]
    pub typewriter: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            focus: Focus::Off,
            dialogue: false,
            typewriter: false,
        }
    }
}
impl Workspace {
    pub fn zen_mode(&self) -> bool {
        self.zen_origin.is_some()
    }
    pub(crate) fn leave_zen(&mut self) {
        if let Some(origin) = self.zen_origin.take() {
            origin.restore(self);
        }
    }
    fn toggle_zen(&mut self) -> io::Result<Outcome> {
        if self.prompt.is_some() {
            return Err(io::Error::other("请先完成或取消当前输入，再切换禅模式"));
        }
        self.cancel_mouse_drag();
        if self.zen_mode() {
            self.leave_zen();
        } else {
            self.zen_origin = Some(crate::search::Origin::take(self));
        }
        Ok(Outcome::Continue)
    }
    pub(crate) fn immersion_settings(&self) -> Settings {
        let settings = self.modules.config.immersion;
        if settings.enabled {
            settings
        } else {
            Settings::default()
        }
    }
    fn update_immersion(&mut self, update: impl FnOnce(&mut Settings)) -> io::Result<Outcome> {
        let mut candidate = self.modules.config.clone();
        update(&mut candidate.immersion);
        candidate.persist(self.modules.path.as_deref())?;
        self.modules.config = candidate;
        Ok(Outcome::Continue)
    }
}
pub(crate) fn commands() -> Vec<Command> {
    let mut commands: Vec<_> = [
        ("focus-off", "关闭聚焦", Focus::Off),
        ("focus-paragraph", "聚焦：当前段落", Focus::Paragraph),
        ("focus-sentence", "聚焦：当前句子", Focus::Sentence),
        ("focus-nearby", "聚焦：当前及相邻视觉行", Focus::Nearby),
    ]
    .into_iter()
    .map(|(id, title, focus)| Command {
        id: id.into(),
        title: title.into(),
        run: match focus {
            Focus::Off => |w| w.update_immersion(|s| s.focus = Focus::Off),
            Focus::Paragraph => |w| w.update_immersion(|s| s.focus = Focus::Paragraph),
            Focus::Sentence => |w| w.update_immersion(|s| s.focus = Focus::Sentence),
            Focus::Nearby => |w| w.update_immersion(|s| s.focus = Focus::Nearby),
        },
    })
    .collect();
    commands.push(Command {
        id: "toggle-dialogue".into(),
        title: "开关对话高亮".into(),
        run: |w| w.update_immersion(|s| s.dialogue = !s.dialogue),
    });
    commands.push(Command {
        id: "toggle-typewriter".into(),
        title: "开关打字机滚动（编辑后居中）".into(),
        run: |w| w.update_immersion(|s| s.typewriter = !s.typewriter),
    });
    commands.push(Command {
        id: "toggle-zen".into(),
        title: "开关禅模式（暂时收起面板）".into(),
        run: Workspace::toggle_zen,
    });
    commands
}
