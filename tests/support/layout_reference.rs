// Independent full-layout oracle for the incremental layout regression tests.
use std::{borrow::Cow, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct View {
    pub lines: Vec<String>,
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
    paragraph: usize,
    sentence: usize,
    glyphs: Vec<Glyph>,
    columns: u16,
}
pub(crate) struct Layout {
    pub rows: Vec<Row>,
    width: u16,
}

impl Row {
    fn new(start: usize, paragraph: usize, sentence: usize) -> Self {
        Self {
            start,
            paragraph,
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

impl Layout {
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
        Self { rows, width }
    }

    pub fn position(&self, cursor: usize, upstream: bool) -> (usize, u16) {
        let mut index = self
            .rows
            .partition_point(|row| row.start <= cursor)
            .saturating_sub(1);
        if upstream && index > 0 && self.rows[index - 1].end == cursor {
            index -= 1;
        }
        (index, self.rows[index].column(cursor))
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
            return View {
                lines: Vec::new(),
                cursor: None,
                selection: Vec::new(),
                ..View::default()
            };
        }
        let (row, column) = self.position(caret.0, caret.1);
        let active_row = row;
        let active_paragraph = self.rows[row].paragraph;
        let active_sentence = self.rows[row]
            .glyphs
            .iter()
            .find(|g| g.range.end > caret.0)
            .map_or(self.rows[row].sentence, |g| g.sentence);
        let mut focus = Vec::new();
        let mut dialogue = Vec::new();
        let mut selected = Vec::new();
        let lines = self
            .rows
            .iter()
            .skip(top)
            .take(height as usize)
            .enumerate()
            .map(|(y, row)| {
                let mut line = String::new();
                let mut x = 0;
                for glyph in &row.glyphs {
                    if settings.dialogue && glyph.dialogue {
                        dialogue.push((x, y as u16, glyph.width));
                    }
                    if settings.focus != crate::immersion::Focus::Off {
                        let active = match settings.focus {
                            crate::immersion::Focus::Off => true,
                            crate::immersion::Focus::Paragraph => row.paragraph == active_paragraph,
                            crate::immersion::Focus::Sentence => glyph.sentence == active_sentence,
                            crate::immersion::Focus::Nearby => (top + y).abs_diff(active_row) <= 1,
                        };
                        focus.push((x, y as u16, glyph.width, active));
                    }
                    if !selection.is_empty()
                        && glyph.range.start < selection.end
                        && glyph.range.end > selection.start
                    {
                        selected.push((x, y as u16, glyph.width));
                    }
                    if glyph.replacement {
                        line.push('�');
                    } else {
                        line.push_str(&visible(&text[glyph.range.clone()]).0);
                    }
                    x += glyph.width;
                }
                line
            })
            .collect();
        View {
            lines,
            cursor: (row >= top && row < top + height as usize)
                .then(|| (column.min(self.width - 1), (row - top) as u16)),
            selection: selected,
            focus,
            dialogue,
        }
    }
}
