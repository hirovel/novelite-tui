use crate::{Action, Direction, View, view};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

pub(crate) struct Document {
    pub(crate) words: u64,
    pub(crate) instance: u64,
    pub(crate) text: String,
    pub(crate) cursor: usize,
    pub(crate) new_file: bool,
    newline: &'static str,
    anchor: Option<usize>,
    undo: Vec<Vec<Edit>>,
    redo: Vec<Vec<Edit>>,
    generation: u64,
    pub(crate) revision: u64,
    pub(crate) saved_revision: u64,
    pub(crate) disk: crate::fingerprint::Fingerprint,
    width: u16,
    height: u16,
    layout: Option<(u64, u16, view::Layout)>,
    alternate_layout: Option<(u64, u16, view::Layout)>,
    upstream: bool,
    preferred_column: Option<u16>,
    top: usize,
    follow_cursor: bool,
    typewriter: bool,
    center_edit: bool,
    changes: Vec<(usize, usize, usize)>,
}

#[derive(Clone)]
pub(crate) struct Position {
    cursor: usize,
    anchor: Option<usize>,
    top: usize,
    width: u16,
    height: u16,
    upstream: bool,
    preferred: Option<u16>,
    follow: bool,
    center_edit: bool,
}

struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
    cursor_after: usize,
    anchor_before: Option<usize>,
    anchor_after: Option<usize>,
    revision_before: u64,
    revision_after: u64,
}

impl Document {
    pub fn new(text: String, new_file: bool) -> Self {
        let newline = if text
            .find('\n')
            .is_some_and(|pos| text[..pos].ends_with('\r'))
        {
            "\r\n"
        } else {
            "\n"
        };
        Self {
            words: crate::word_count::count(text.chars()),
            instance: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            disk: if new_file {
                crate::fingerprint::Fingerprint::Missing
            } else {
                crate::fingerprint::Fingerprint::bytes(text.as_bytes())
            },
            text,
            cursor: 0,
            new_file,
            newline,
            anchor: None,
            undo: Vec::new(),
            redo: Vec::new(),
            generation: 0,
            revision: 0,
            saved_revision: 0,
            width: 80,
            height: 0,
            layout: None,
            alternate_layout: None,
            upstream: false,
            preferred_column: None,
            top: 0,
            follow_cursor: true,
            typewriter: false,
            center_edit: false,
            changes: Vec::new(),
        }
    }
    pub fn apply(&mut self, action: Action) -> bool {
        self.changes.clear();
        self.center_edit = false;
        self.follow_cursor = true;
        let before = self.revision;
        match action {
            Action::Newline => return self.apply(Action::Insert(self.newline.into())),
            Action::Insert(text) => {
                self.replace(self.selection(), text);
            }
            Action::Move(direction) => {
                let selection = self.selection();
                self.anchor = None;
                if !selection.is_empty() && matches!(direction, Direction::Left | Direction::Right)
                {
                    self.cursor = if matches!(direction, Direction::Left) {
                        selection.start
                    } else {
                        selection.end
                    };
                    self.upstream = false;
                    self.preferred_column = None;
                } else {
                    self.move_cursor(direction);
                }
            }
            Action::Select(direction) => {
                self.anchor.get_or_insert(self.cursor);
                self.move_cursor(direction);
            }
            Action::SelectAll => {
                self.anchor = Some(0);
                self.cursor = self.text.len();
                self.upstream = false;
                self.preferred_column = None;
            }
            Action::Undo => {
                if let Some(transaction) = self.undo.pop() {
                    for edit in transaction.iter().rev() {
                        self.changes
                            .push((edit.start, edit.inserted.len(), edit.removed.len()));
                        self.replace_text(
                            edit.start..edit.start + edit.inserted.len(),
                            &edit.removed,
                            edit.revision_before,
                        );
                        self.cursor = edit.cursor_before;
                        self.anchor = edit.anchor_before;
                        self.revision = edit.revision_before;
                        self.upstream = false;
                        self.preferred_column = None;
                    }
                    self.redo.push(transaction);
                }
            }
            Action::Redo => {
                if let Some(transaction) = self.redo.pop() {
                    for edit in &transaction {
                        self.changes
                            .push((edit.start, edit.removed.len(), edit.inserted.len()));
                        self.replace_text(
                            edit.start..edit.start + edit.removed.len(),
                            &edit.inserted,
                            edit.revision_after,
                        );
                        self.cursor = edit.cursor_after;
                        self.anchor = edit.anchor_after;
                        self.revision = edit.revision_after;
                        self.upstream = false;
                        self.preferred_column = None;
                    }
                    self.undo.push(transaction);
                }
            }
            Action::Backspace => {
                let range = self.selection();
                let range = if range.is_empty() {
                    self.previous()..self.cursor
                } else {
                    range
                };
                self.replace(range, String::new());
            }
            Action::Delete => {
                let range = self.selection();
                let range = if range.is_empty() {
                    self.cursor..self.next()
                } else {
                    range
                };
                self.replace(range, String::new());
            }
            Action::Save | Action::Quit => {}
        }
        self.center_edit = before != self.revision;
        before != self.revision
    }
    pub(crate) fn apply_assisted(&mut self, action: Action) -> bool {
        match crate::pairing::plan(&self.text, self.selection(), &action) {
            Some(crate::pairing::Assistance::Skip) => self.apply(Action::Move(Direction::Right)),
            Some(crate::pairing::Assistance::Replace {
                range,
                text,
                selection,
            }) => {
                self.changes.clear();
                self.replace(range, text);
                self.select_range(selection.clone());
                if selection.is_empty() {
                    self.anchor = None;
                }
                let edit = self.undo.last_mut().unwrap().last_mut().unwrap();
                edit.cursor_after = self.cursor;
                edit.anchor_after = self.anchor;
                true
            }
            None => self.apply(action),
        }
    }
    pub fn has_edits(&self) -> bool {
        self.generation != 0
    }
    pub fn refresh(&mut self, text: String) {
        let position = self.cursor.min(text.len());
        let cursor = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .take_while(|i| *i <= position)
            .last()
            .unwrap_or(0);
        *self = Self::new(text, false);
        self.cursor = cursor;
    }
    pub(crate) fn selection(&self) -> std::ops::Range<usize> {
        let anchor = self.anchor.unwrap_or(self.cursor);
        anchor.min(self.cursor)..anchor.max(self.cursor)
    }

    fn replace(&mut self, range: std::ops::Range<usize>, inserted: String) {
        if self.text[range.clone()] == inserted {
            self.cursor = range.end;
            self.anchor = None;
            self.upstream = false;
            self.preferred_column = None;
            return;
        }
        self.center_edit = true;
        self.generation += 1;
        let mut edit = Edit {
            start: range.start,
            removed: self.text[range.clone()].into(),
            cursor_before: self.cursor,
            cursor_after: range.start + inserted.len(),
            anchor_before: self.anchor,
            anchor_after: None,
            revision_before: self.revision,
            revision_after: self.generation,
            inserted,
        };
        self.replace_text(range, &edit.inserted, edit.revision_after);
        self.changes
            .push((edit.start, edit.removed.len(), edit.inserted.len()));
        let mut boundary = GraphemeCursor::new(edit.cursor_after, self.text.len(), true);
        if !boundary
            .is_boundary(&self.text, 0)
            .expect("complete text supplies grapheme context")
        {
            edit.cursor_after = boundary
                .next_boundary(&self.text, 0)
                .expect("complete text supplies grapheme context")
                .unwrap_or(self.text.len());
        }
        self.cursor = edit.cursor_after;
        self.anchor = None;
        self.revision = edit.revision_after;
        self.undo.push(vec![edit]);
        self.redo.clear();
        self.upstream = false;
        self.preferred_column = None;
    }

    pub(crate) fn current_line(&mut self) -> crate::heading::LineInfo {
        self.ensure_layout();
        self.layout.as_ref().unwrap().2.logical_line(self.cursor)
    }

    pub(crate) fn replace_preserving_selection(
        &mut self,
        range: std::ops::Range<usize>,
        inserted: &str,
    ) -> bool {
        let mut position = self.position();
        if !self.replace_ranges(&[range], inserted) {
            return false;
        }
        self.map_position(&mut position);
        self.restore_position(&position);
        self.follow_cursor = true;
        self.preferred_column = None;
        if let Some(edit) = self.undo.last_mut().and_then(|t| t.last_mut()) {
            edit.cursor_after = self.cursor;
            edit.anchor_after = self.anchor;
        }
        true
    }

    pub(crate) fn mark_chapter_title(&self, view: &mut View, title: Option<&str>) {
        let Some(title) = title else {
            return;
        };
        for (heading, range) in view.headings.iter_mut().zip(&view.logical_lines) {
            if let Some(range) = range {
                *heading |= crate::heading::matches_title(&self.text[range.clone()], title);
            }
        }
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    pub(crate) fn replace_ranges(&mut self, ranges: &[std::ops::Range<usize>], text: &str) -> bool {
        self.changes.clear();
        self.follow_cursor = true;
        let start = self.undo.len();
        for range in ranges.iter().rev() {
            self.replace(range.clone(), text.to_owned());
        }
        if self.undo.len() == start {
            return false;
        }
        let transaction = self.undo.drain(start..).flatten().collect();
        self.undo.push(transaction);
        true
    }

    pub(crate) fn select_range(&mut self, range: std::ops::Range<usize>) {
        self.anchor = Some(self.boundary(range.start));
        self.cursor = self.boundary(range.end);
        self.upstream = false;
        self.preferred_column = None;
        self.follow_cursor = true;
    }

    fn move_cursor(&mut self, direction: Direction) {
        if matches!(direction, Direction::Left | Direction::Right) {
            self.cursor = if matches!(direction, Direction::Left) {
                self.previous()
            } else {
                self.next()
            };
            self.preferred_column = None;
            self.upstream = false;
            return;
        }
        self.ensure_layout();
        let layout = &self.layout.as_ref().unwrap().2;
        let (row, column) = layout.position(self.cursor, self.upstream);
        let target = match direction {
            Direction::Up => row.saturating_sub(1),
            Direction::Down => (row + 1).min(layout.row_count() - 1),
            _ => row,
        };
        match direction {
            Direction::Home => {
                self.cursor = layout.row_start(row);
                self.preferred_column = None;
                self.upstream = false;
            }
            Direction::End => {
                self.cursor = layout.row_end(row);
                self.preferred_column = None;
                self.upstream = layout.row_start(row) != layout.row_end(row);
            }
            _ => {
                let desired = *self.preferred_column.get_or_insert(column);
                self.cursor = layout.row_offset(target, desired);
                self.upstream = layout.row_start(target) != layout.row_end(target)
                    && self.cursor == layout.row_end(target);
            }
        }
    }

    // One mutation seam for normal edits, assisted input, batch replace and history.
    // Update the active display index before publishing text; retire stale widths.
    fn replace_text(&mut self, range: std::ops::Range<usize>, inserted: &str, revision: u64) {
        let cached_target = self.alternate_layout.as_ref().is_some_and(|(r, w, _)| {
            *r == revision
                && self
                    .layout
                    .as_ref()
                    .is_some_and(|(_, current_width, _)| w == current_width)
        });
        if cached_target {
            // Undo/redo can reuse the last revision, without reflowing even a long line.
            std::mem::swap(&mut self.layout, &mut self.alternate_layout);
        } else if let Some((cached_revision, width, mut layout)) = self.layout.take() {
            if cached_revision == self.revision {
                // Only the block index is cloned; unchanged paragraph glyphs are shared.
                let previous = layout.clone();
                layout.replace(&self.text, range.clone(), inserted);
                self.layout = Some((revision, width, layout));
                self.alternate_layout = Some((cached_revision, width, previous));
            } else {
                self.alternate_layout = None;
            }
        } else {
            self.alternate_layout = None;
        }
        crate::word_count::replace(&mut self.text, &mut self.words, range, inserted);
    }

    fn ensure_layout(&mut self) {
        let matches = |cached: &Option<(u64, u16, view::Layout)>| {
            cached.as_ref().is_some_and(|(revision, width, _)| {
                *revision == self.revision && *width == self.width
            })
        };
        if matches(&self.layout) {
            return;
        }
        if matches(&self.alternate_layout) {
            std::mem::swap(&mut self.layout, &mut self.alternate_layout);
        } else {
            // Two panes can have different widths. Keep both layouts without growing
            // a cache for every width visited during a resize or every undo revision.
            self.alternate_layout = self.layout.replace((
                self.revision,
                self.width,
                view::Layout::new(&self.text, self.width),
            ));
        }
    }
    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(pos, _)| pos)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |cluster| self.cursor + cluster.len())
    }

    pub fn view(&mut self, width: u16, height: u16) -> View {
        self.writing_view(width, height, crate::immersion::Settings::default())
    }
    pub fn writing_view(
        &mut self,
        width: u16,
        height: u16,
        settings: crate::immersion::Settings,
    ) -> View {
        self.typewriter = settings.typewriter;
        if width == 0 {
            return View {
                lines: Vec::new(),
                cursor: None,
                selection: Vec::new(),
                ..View::default()
            };
        }
        self.ensure_layout();
        let old_layout = &self.layout.as_ref().unwrap().2;
        let anchor = old_layout.row_start(self.top.min(old_layout.row_count() - 1));
        let row = old_layout.position(self.cursor, self.upstream).0;
        let was_visible = row >= self.top && row < self.top + self.height as usize;
        let resized = self.width != width || self.height != height;
        // Reflow changes geometry, not the writer's vertical-navigation goal.
        // In particular, a short line must not erase that goal when a panel
        // opens or the terminal resizes. Explicit horizontal motion/editing
        // already resets it; each target row clamps the remembered column.
        self.width = width;
        self.height = height;
        self.ensure_layout();
        let layout = &self.layout.as_ref().unwrap().2;
        if resized {
            self.top = layout.position(anchor, false).0;
            self.follow_cursor |= was_visible;
        }
        self.top = self.top.min(self.scroll_limit(height));
        if settings.typewriter && self.center_edit && self.follow_cursor && height > 0 {
            self.top = layout
                .position(self.cursor, self.upstream)
                .0
                .saturating_sub(height as usize / 2);
        }
        self.center_edit = false;
        if self.follow_cursor && height > 0 {
            let (row, _) = layout.position(self.cursor, self.upstream);
            if row < self.top {
                self.top = row;
            }
            if row >= self.top + height as usize {
                self.top = row + 1 - height as usize;
            }
        }
        self.follow_cursor = false;
        self.layout.as_ref().unwrap().2.viewport(
            &self.text,
            (self.cursor, self.upstream),
            self.selection(),
            height,
            self.top,
            settings,
        )
    }
    pub fn point(&mut self, column: u16, row: u16, selecting: bool) {
        self.ensure_layout();
        if selecting {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        let layout = &self.layout.as_ref().unwrap().2;
        let row = (self.top + row as usize).min(layout.row_count() - 1);
        self.cursor = layout.row_offset(row, column);
        self.upstream =
            layout.row_start(row) != layout.row_end(row) && self.cursor == layout.row_end(row);
        self.preferred_column = None;
        self.follow_cursor = false;
        self.center_edit = false;
    }
    fn scroll_limit(&self, height: u16) -> usize {
        let bottom = if self.typewriter { 1 } else { height as usize };
        self.layout
            .as_ref()
            .unwrap()
            .2
            .row_count()
            .saturating_sub(bottom)
    }
    pub fn scroll(&mut self, rows: isize, height: u16) {
        self.ensure_layout();
        self.top = self
            .top
            .saturating_add_signed(rows)
            .min(self.scroll_limit(height));
        self.follow_cursor = false;
        self.center_edit = false;
    }
    pub fn position(&mut self) -> Position {
        self.ensure_layout();
        let layout = &self.layout.as_ref().unwrap().2;
        Position {
            cursor: self.cursor,
            anchor: self.anchor,
            top: layout.row_start(self.top.min(layout.row_count() - 1)),
            width: self.width,
            height: self.height,
            upstream: self.upstream,
            preferred: self.preferred_column,
            follow: self.follow_cursor,
            center_edit: self.center_edit,
        }
    }
    fn boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let mut cursor = GraphemeCursor::new(offset, self.text.len(), true);
        if cursor.is_boundary(&self.text, 0).unwrap() {
            offset
        } else {
            cursor.prev_boundary(&self.text, 0).unwrap().unwrap_or(0)
        }
    }
    pub fn restore_position(&mut self, position: &Position) {
        self.cursor = self.boundary(position.cursor);
        self.anchor = position.anchor.map(|p| self.boundary(p));
        self.width = position.width;
        self.height = position.height;
        self.upstream = position.upstream;
        self.preferred_column = position.preferred;
        self.follow_cursor = position.follow;
        self.center_edit = position.center_edit;
        self.ensure_layout();
        self.top = self
            .layout
            .as_ref()
            .unwrap()
            .2
            .position(self.boundary(position.top), false)
            .0;
    }
    pub fn map_position(&self, position: &mut Position) {
        for &(start, removed, inserted) in &self.changes {
            let map = |offset: usize, after: bool| {
                if offset < start || (offset == start && !after) {
                    offset
                } else if offset <= start + removed {
                    start + inserted
                } else {
                    offset - removed + inserted
                }
            };
            position.cursor = map(position.cursor, true);
            position.anchor = position.anchor.map(|p| map(p, true));
            position.top = map(position.top, false);
        }
        position.cursor = self.boundary(position.cursor);
        position.anchor = position.anchor.map(|p| self.boundary(p));
        position.top = self.boundary(position.top);
        position.preferred = None;
    }
}
