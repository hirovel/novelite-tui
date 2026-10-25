use std::{borrow::Cow, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct View {
    pub lines: Vec<String>,
    pub headings: Vec<bool>,
    pub(crate) logical_lines: Vec<Option<Range<usize>>>,
    /// (column, row, width, emphasized) for focus styling.
    pub dialogue: Vec<(u16, u16, u16)>,
    pub focus: Vec<(u16, u16, u16, bool)>,
    pub cursor: Option<(u16, u16)>,
    /// (column, row, width) of selected terminal cells.
    pub selection: Vec<(u16, u16, u16)>,
}

fn visible(cluster: &str) -> (Cow<'_, str>, usize) {
    if cluster == "\t" {
        return (Cow::Borrowed("    "), 4);
    }
    if !cluster.chars().any(char::is_control) {
        let width = cluster.width();
        if width > 0 {
            return (Cow::Borrowed(cluster), width);
        }
        let text = format!("◌{cluster}");
        let width = text.width();
        return (Cow::Owned(text), width);
    }
    let text: String = cluster
        .chars()
        .map(|ch| match ch {
            '\x00'..='\x1f' => char::from_u32(ch as u32 + 0x2400).unwrap(),
            '\x7f' => '␡',
            ch if ch.is_control() => '�',
            ch => ch,
        })
        .collect();
    let width = text.width();
    (Cow::Owned(text), width)
}

struct Glyph {
    range: Range<usize>,
    width: u16,
    replacement: bool,
    dialogue: bool,
    sentence: usize,
}
pub(crate) struct Row {
    pub start: usize,
    pub end: usize,
    sentence: usize,
    glyphs: Vec<Glyph>,
    columns: u16,
}
struct Paragraph {
    pub rows: Vec<Row>,
}

impl Row {
    fn new(start: usize, _paragraph: usize, sentence: usize) -> Self {
        Self {
            start,
            sentence,
            end: start,
            glyphs: Vec::new(),
            columns: 0,
        }
    }
    fn column(&self, offset: usize) -> u16 {
        self.glyphs
            .iter()
            .take_while(|glyph| glyph.range.end <= offset)
            .map(|glyph| glyph.width)
            .sum()
    }
    pub fn offset(&self, column: u16) -> usize {
        let mut x = 0;
        for glyph in &self.glyphs {
            if x + glyph.width > column {
                return glyph.range.start;
            }
            x += glyph.width;
        }
        self.end
    }
}

impl Paragraph {
    pub fn new(text: &str, width: u16) -> Self {
        let width = width.max(1);
        let mut rows = Vec::new();
        let mut paragraph = 0;
        let mut sentence = 0;
        let mut ended = false;
        let mut closing_quote = None;
        let mut row = Row::new(0, paragraph, sentence);
        for (offset, cluster) in text.grapheme_indices(true) {
            if cluster == "\n" || cluster == "\r\n" {
                row.end = offset;
                rows.push(row);
                paragraph += 1;
                sentence += 1;
                ended = false;
                closing_quote = None;
                row = Row::new(offset + cluster.len(), paragraph, sentence);
                continue;
            }
            if ended && !cluster.chars().all(|c| "。！？!?….”’\"」』".contains(c)) {
                sentence += 1;
                ended = false;
            }
            let size = visible(cluster).1.max(1);
            let replacement = size > width as usize;
            let size = if replacement { 1 } else { size as u16 };
            if row.columns + size > width {
                row.end = offset;
                rows.push(row);
                row = Row::new(offset, paragraph, sentence);
            }
            let quote = cluster.chars().next().unwrap();
            let was_quoted = closing_quote.is_some();
            if closing_quote == Some(quote) {
                closing_quote = None;
            } else if closing_quote.is_none() {
                closing_quote = match quote {
                    '“' => Some('”'),
                    '「' => Some('」'),
                    '『' => Some('』'),
                    '"' => Some('"'),
                    _ => None,
                };
            }
            row.glyphs.push(Glyph {
                range: offset..offset + cluster.len(),
                width: size,
                replacement,
                dialogue: was_quoted || closing_quote.is_some(),
                sentence,
            });
            ended |= cluster.chars().any(|c| "。！？!?….".contains(c));
            row.sentence = sentence;
            row.columns += size;
            row.end = offset + cluster.len();
        }
        let full_at_eof = row.columns == width;
        rows.push(row);
        if full_at_eof {
            rows.push(Row::new(text.len(), paragraph, sentence));
        }
        Self { rows }
    }
}

/// Each block owns relative glyph offsets, so edits do not rewrite every glyph
/// after the insertion. Only the lightweight block index needs rebasing.
#[derive(Clone)]
struct Block {
    start: usize,
    first_row: usize,
    bytes: usize,
    paragraph: std::sync::Arc<Paragraph>,
    syntax: crate::heading::Syntax,
    protected: bool,
    fence_after: crate::markdown::Fence,
}

/// Incremental display index. The document supplies every replacement in order,
/// including undo/redo; callers never need to invalidate individual widgets.
/// Storage is bounded to one layout per cached width, not per edit revision.
#[derive(Clone)]
pub(crate) struct Layout {
    blocks: Vec<Block>,
    width: u16,
    rows: usize,
    #[cfg(test)]
    measured_bytes: usize,
}

impl Layout {
    pub fn new(text: &str, width: u16) -> Self {
        let mut layout = Self {
            blocks: Self::measure(text, width.max(1), true),
            width: width.max(1),
            rows: 0,
            #[cfg(test)]
            measured_bytes: text.len(),
        };
        layout.reindex(0);
        layout
    }

    fn measure(text: &str, width: u16, at_eof: bool) -> Vec<Block> {
        let mut blocks = Vec::new();
        for part in text.split_inclusive('\n') {
            let terminated = part.ends_with('\n');
            let body = if terminated {
                part.strip_suffix("\r\n").unwrap_or(&part[..part.len() - 1])
            } else {
                part
            };
            let mut paragraph = Paragraph::new(body, width);
            // Only EOF needs an extra cursor row after an exactly full line.
            if terminated
                && paragraph.rows.len() > 1
                && paragraph
                    .rows
                    .last()
                    .is_some_and(|row| row.start == body.len())
            {
                paragraph.rows.pop();
            }
            blocks.push(Block {
                start: 0,
                first_row: 0,
                bytes: part.len(),
                paragraph: std::sync::Arc::new(paragraph),
                syntax: crate::heading::Syntax::new(body),
                protected: false,
                fence_after: Default::default(),
            });
        }
        if at_eof && (text.is_empty() || text.ends_with('\n')) {
            blocks.push(Block {
                start: 0,
                first_row: 0,
                bytes: 0,
                paragraph: std::sync::Arc::new(Paragraph::new("", width)),
                syntax: crate::heading::Syntax::new(""),
                protected: false,
                fence_after: Default::default(),
            });
        }
        blocks
    }

    fn reindex(&mut self, from: usize) {
        let (mut byte, mut row) = if from == 0 {
            (0, 0)
        } else {
            let previous = &self.blocks[from - 1];
            (
                previous.start + previous.bytes,
                previous.first_row + previous.paragraph.rows.len(),
            )
        };
        let mut fence = if from == 0 {
            crate::markdown::Fence::default()
        } else {
            self.blocks[from - 1].fence_after.clone()
        };
        for block in &mut self.blocks[from..] {
            block.protected = fence.protects(block.syntax.fence_line.as_deref().unwrap_or(""));
            block.fence_after = fence.clone();
            block.start = byte;
            block.first_row = row;
            byte += block.bytes;
            row += block.paragraph.rows.len();
        }
        self.rows = row;
    }

    fn block_at(&self, offset: usize) -> usize {
        self.blocks
            .partition_point(|block| block.start <= offset)
            .saturating_sub(1)
    }

    /// `text` is the pre-edit document. Reparse the complete affected physical
    /// lines: newline, CRLF and grapheme context cannot leak across these seams.
    pub fn replace(&mut self, text: &str, range: Range<usize>, inserted: &str) {
        let first = self.block_at(range.start);
        let last = self.block_at(range.end);
        let start = self.blocks[first].start;
        let end = self.blocks[last].start + self.blocks[last].bytes;
        let mut changed = String::with_capacity(end - start - range.len() + inserted.len());
        changed.push_str(&text[start..range.start]);
        changed.push_str(inserted);
        changed.push_str(&text[range.end..end]);
        #[cfg(test)]
        {
            self.measured_bytes += changed.len();
        }
        let replacement = Self::measure(&changed, self.width, last + 1 == self.blocks.len());
        self.blocks.splice(first..=last, replacement);
        self.reindex(first);
    }

    pub(crate) fn logical_line(&self, offset: usize) -> crate::heading::LineInfo {
        let block = &self.blocks[self.block_at(offset)];
        crate::heading::LineInfo {
            range: block.start..block.start + block.syntax.bytes,
            prefix: block
                .syntax
                .prefix
                .as_ref()
                .map(|p| block.start + p.start..block.start + p.end),
            protected: block.protected,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows
    }

    fn row(&self, index: usize) -> (&Block, &Row, usize) {
        let block_index = self
            .blocks
            .partition_point(|block| block.first_row <= index)
            .saturating_sub(1);
        let block = &self.blocks[block_index];
        (
            block,
            &block.paragraph.rows[index - block.first_row],
            block_index,
        )
    }
    pub fn row_start(&self, index: usize) -> usize {
        let (block, row, _) = self.row(index);
        block.start + row.start
    }
    pub fn row_end(&self, index: usize) -> usize {
        let (block, row, _) = self.row(index);
        block.start + row.end
    }
    pub fn row_offset(&self, index: usize, column: u16) -> usize {
        let (block, row, _) = self.row(index);
        block.start + row.offset(column)
    }
    pub fn position(&self, cursor: usize, upstream: bool) -> (usize, u16) {
        let block = &self.blocks[self.block_at(cursor)];
        let local = cursor - block.start;
        let index = block
            .paragraph
            .rows
            .partition_point(|row| row.start <= local)
            .saturating_sub(1);
        let mut index = block.first_row + index;
        if upstream && index > 0 && self.row_end(index - 1) == cursor {
            index -= 1;
        }
        let (block, row, _) = self.row(index);
        (index, row.column(cursor - block.start))
    }

    pub fn viewport(
        &self,
        text: &str,
        caret: (usize, bool),
        selection: Range<usize>,
        height: u16,
        top: usize,
        settings: crate::immersion::Settings,
    ) -> View {
        if height == 0 {
            return View::default();
        }
        let (active_row, column) = self.position(caret.0, caret.1);
        let (active_block, row, active_paragraph) = self.row(active_row);
        let active_sentence = row
            .glyphs
            .iter()
            .find(|g| g.range.end + active_block.start > caret.0)
            .map_or(row.sentence, |g| g.sentence);
        let mut result = View::default();
        for (y, index) in (top..self.rows.min(top + height as usize)).enumerate() {
            let (block, row, paragraph) = self.row(index);
            let mut line = String::new();
            let mut x = 0;
            for glyph in &row.glyphs {
                if settings.dialogue && glyph.dialogue {
                    result.dialogue.push((x, y as u16, glyph.width));
                }
                if settings.focus != crate::immersion::Focus::Off {
                    let active = match settings.focus {
                        crate::immersion::Focus::Off => true,
                        crate::immersion::Focus::Paragraph => paragraph == active_paragraph,
                        crate::immersion::Focus::Sentence => {
                            paragraph == active_paragraph && glyph.sentence == active_sentence
                        }
                        crate::immersion::Focus::Nearby => index.abs_diff(active_row) <= 1,
                    };
                    result.focus.push((x, y as u16, glyph.width, active));
                }
                let range = block.start + glyph.range.start..block.start + glyph.range.end;
                if !selection.is_empty()
                    && range.start < selection.end
                    && range.end > selection.start
                {
                    result.selection.push((x, y as u16, glyph.width));
                }
                if glyph.replacement {
                    line.push('�');
                } else {
                    line.push_str(&visible(&text[range]).0);
                }
                x += glyph.width;
            }
            result
                .headings
                .push(!block.protected && block.syntax.prefix.is_some());
            result
                .logical_lines
                .push((!block.protected).then_some(block.start..block.start + block.syntax.bytes));
            result.lines.push(line);
        }
        result.cursor = (active_row >= top && active_row < top + height as usize)
            .then(|| (column.min(self.width - 1), (active_row - top) as u16));
        result
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/layout_reference.rs"]
mod reference;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immersion::{Focus, Settings};

    fn compare(layout: &Layout, text: &str, width: u16, seed: usize) {
        let expected = reference::Layout::new(text, width);
        assert_eq!(
            layout.row_count(),
            expected.rows.len(),
            "{text:?} width {width}"
        );
        for i in 0..layout.row_count() {
            assert_eq!(
                (layout.row_start(i), layout.row_end(i)),
                (expected.rows[i].start, expected.rows[i].end)
            );
            for x in [0, width / 2, width] {
                assert_eq!(layout.row_offset(i, x), expected.rows[i].offset(x));
            }
        }
        let offsets: Vec<_> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        let cursor = offsets[seed % offsets.len()];
        for upstream in [false, true] {
            assert_eq!(
                layout.position(cursor, upstream),
                expected.position(cursor, upstream)
            );
            for focus in [Focus::Off, Focus::Paragraph, Focus::Sentence, Focus::Nearby] {
                let settings = Settings {
                    focus,
                    dialogue: true,
                    ..Settings::default()
                };
                let top = seed % layout.row_count();
                let a = layout.viewport(text, (cursor, upstream), 0..cursor, 9, top, settings);
                let b = expected.viewport(text, (cursor, upstream), 0..cursor, 9, top, settings);
                assert_eq!(
                    (a.lines, a.cursor, a.selection, a.focus, a.dialogue),
                    (b.lines, b.cursor, b.selection, b.focus, b.dialogue),
                    "seed {seed}, text {text:?}"
                );
            }
        }
    }

    #[test]
    fn incremental_changes_match_original_full_layout_for_unicode_and_newlines() {
        let inserts = [
            "中",
            "\n",
            "\r",
            "\r\n",
            "e\u{301}",
            "👩‍💻",
            "\u{301}",
            "\u{200d}",
            "🇨🇳",
            "“a。”,",
            "\t\x1b",
            "",
            "长段落\n\n末尾",
        ];
        for width in [1, 2, 7, 20, 80] {
            let mut text = "甲“乙。丙”\r\n\n👩‍💻e\u{301}\t尾\r孤\n".to_owned();
            let mut layout = Layout::new(&text, width);
            let mut seed = 19u64;
            compare(&layout, &text, width, 0);
            for step in 0..240 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let offsets: Vec<_> = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain([text.len()])
                    .collect();
                let a = seed as usize % offsets.len();
                let b = (a + (seed >> 32) as usize % 4).min(offsets.len() - 1);
                let range = offsets[a]..offsets[b];
                let inserted = inserts[step % inserts.len()];
                layout.replace(&text, range.clone(), inserted);
                text.replace_range(range, inserted);
                compare(&layout, &text, width, step);
            }
        }
    }

    #[test]
    fn local_edit_measures_only_affected_lines_and_keeps_other_glyph_storage() {
        let mut text = "一段“中文”。e\u{301}👩‍💻\n".repeat(10_000);
        let mut layout = Layout::new(&text, 40);
        let untouched = layout.blocks[9999].paragraph.rows[0].glyphs.as_ptr();
        let before = layout.measured_bytes;
        layout.replace(&text, 3..3, "新\n段");
        text.insert_str(3, "新\n段");
        assert!(layout.measured_bytes - before < 100);
        assert_eq!(layout.blocks.last().unwrap().bytes, 0);
        assert_eq!(
            layout.blocks[10_000].paragraph.rows[0].glyphs.as_ptr(),
            untouched
        );
        let rebuilt = Layout::new(&text, 40);
        assert_eq!(layout.row_count(), rebuilt.row_count());
        assert_eq!(layout.row_start(layout.row_count() - 1), text.len());
    }
}
