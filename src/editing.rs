use crate::{Action, Outcome, Workspace};
use std::io;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditTarget {
    Manuscript(usize),
    Outline,
    Tool,
}

pub(crate) struct EditFocus {
    target: EditTarget,
    generation: u64,
}

impl Default for EditFocus {
    fn default() -> Self {
        Self {
            target: EditTarget::Manuscript(0),
            generation: 0,
        }
    }
}

impl EditFocus {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

impl Workspace {
    pub(crate) fn edit_target(&self) -> EditTarget {
        if self.design_ui.open
            || self.commands.palette_selection().is_some()
            || self.import_active()
        {
            return EditTarget::Tool;
        }
        if let Some(prompt) = &self.prompt {
            return if prompt.editor.is_some() {
                EditTarget::Outline
            } else {
                EditTarget::Tool
            };
        }
        if self.navigator.is_some() || self.history_preview.is_some() || self.book_search_focused()
        {
            EditTarget::Tool
        } else {
            EditTarget::Manuscript(self.active_pane())
        }
    }

    pub(crate) fn run_edit_command(&mut self, action: Action) -> io::Result<Outcome> {
        if self.edit_target() == EditTarget::Tool {
            return Ok(Outcome::Continue);
        }
        self.dispatch(action)
    }

    pub(crate) fn sync_edit_focus(&mut self) {
        let target = self.edit_target();
        if self.edit_focus.target != target {
            self.edit_focus.target = target;
            self.edit_focus.generation += 1;
        }
    }
}
