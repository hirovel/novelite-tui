//! Shared single-line editing. Text stays with its owner; byte offsets and the
//! grapheme-aligned horizontal viewport remain private to this Module.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Default)]
pub(crate) struct LineEditor {
    // None starts at the end, including values prefilled before first use.
    cursor: Option<usize>,
    horizontal_start: usize,
    viewport_width: u16,
}

impl LineEditor {
    fn cursor(&self, text: &str) -> usize {
        let desired = self.cursor.unwrap_or(text.len()).min(text.len());
        text.grapheme_indices(true)
            .map(|(at, _)| at)
            .chain(std::iter::once(text.len()))
            .take_while(|&at| at <= desired)
            .last()
            .unwrap_or(0)
    }

    fn insert(&mut self, text: &mut String, value: &str, cursor: usize) -> bool {
        let value: String = value.chars().filter(|ch| !ch.is_control()).collect();
        if value.is_empty() {
            return false;
        }
        text.insert_str(cursor, &value);
        // Insertion can join a combining sequence on either side.
        self.cursor = Some(
            text.grapheme_indices(true)
                .map(|(at, _)| at)
                .chain(std::iter::once(text.len()))
                .find(|&at| at >= cursor + value.len())
                .unwrap_or(text.len()),
        );
        true
    }

    /// Apply a text edit or caret movement, returning whether the text changed.
    /// Submit/cancel, list navigation and shortcuts belong to the caller.
    pub(crate) fn event(&mut self, text: &mut String, event: &Event) -> bool {
        let cursor = self.cursor(text);
        self.cursor = Some(cursor);
        match event {
            Event::Paste(value) => self.insert(text, value, cursor),
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let previous = || {
                    text[..cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(at, _)| at)
                };
                let next = || {
                    text[cursor..]
                        .graphemes(true)
                        .next()
                        .map_or(cursor, |g| cursor + g.len())
                };
                match key.code {
                    KeyCode::Left => self.cursor = Some(previous()),
                    KeyCode::Right => self.cursor = Some(next()),
                    KeyCode::Home => self.cursor = Some(0),
                    KeyCode::End => self.cursor = Some(text.len()),
                    KeyCode::Backspace => {
                        let previous = previous();
                        text.replace_range(previous..cursor, "");
                        self.cursor = Some(previous);
                        return previous != cursor;
                    }
                    KeyCode::Delete => {
                        let next = next();
                        text.replace_range(cursor..next, "");
                        return next != cursor;
                    }
                    KeyCode::Char(ch)
                        if !key.modifiers.intersects(
                            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                        ) =>
                    {
                        return self.insert(text, &ch.to_string(), cursor);
                    }
                    _ => {}
                }
                false
            }
            _ => false,
        }
    }

    /// One grapheme/column map controls clipping and the visible caret. The caller
    /// reserves this width for the input itself, without its prefix or borders.
    pub(crate) fn view(&mut self, text: &str, width: u16) -> (String, u16) {
        if width == 0 {
            return (String::new(), 0);
        }
        let cursor = self.cursor(text);
        let glyphs: Vec<_> = text
            .grapheme_indices(true)
            .map(|(at, glyph)| {
                let label = crate::render::safe_label(glyph);
                let label = if label.width() == 0 {
                    format!("◌{label}")
                } else {
                    label
                };
                let columns = label.width();
                (at, label, columns)
            })
            .collect();
        let cursor_index = glyphs.partition_point(|(at, _, _)| *at < cursor);
        // Keep the complete glyph under the caret visible, not just its first
        // column. At end-of-line reserve a blank cursor cell instead.
        let caret_columns = glyphs
            .get(cursor_index)
            .map_or(1, |(_, _, columns)| *columns)
            .max(1)
            .min(usize::from(width));
        let mut first = glyphs
            .partition_point(|(at, _, _)| *at < self.horizontal_start)
            .min(cursor_index);
        let mut column: usize = glyphs[first..cursor_index].iter().map(|(_, _, w)| w).sum();
        while column + caret_columns > usize::from(width) && first < cursor_index {
            column -= glyphs[first].2;
            first += 1;
        }
        // Widening reveals more context without shifting a visible caret on edits.
        while width > self.viewport_width
            && first > 0
            && column + glyphs[first - 1].2 + caret_columns <= usize::from(width)
        {
            first -= 1;
            column += glyphs[first].2;
        }
        self.viewport_width = width;
        self.horizontal_start = glyphs.get(first).map_or(text.len(), |(at, _, _)| *at);
        let mut line = String::new();
        let mut used = 0;
        for (_, label, columns) in &glyphs[first..] {
            if used + columns > usize::from(width) {
                break;
            }
            line.push_str(label);
            used += columns;
        }
        (line, column as u16)
    }
}
