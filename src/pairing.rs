use crate::Action;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) enum Assistance {
    Skip,
    Replace {
        range: Range<usize>,
        text: String,
        selection: Range<usize>,
    },
}
fn closing(open: char) -> Option<char> {
    Some(match open {
        '“' => '”',
        '‘' => '’',
        '"' => '"',
        '\'' => '\'',
        '《' => '》',
        '（' => '）',
        '(' => ')',
        '【' => '】',
        '[' => ']',
        '「' => '」',
        '『' => '』',
        '{' => '}',
        _ => return None,
    })
}
pub(crate) fn plan(text: &str, range: Range<usize>, action: &Action) -> Option<Assistance> {
    let previous = text[..range.start].chars().next_back();
    let next = text[range.end..].chars().next();
    match action {
        Action::Insert(value) => {
            let mut chars = value.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if range.is_empty() && next == Some(ch) && "”’\"'》）)】]」』}".contains(ch) {
                return Some(Assistance::Skip);
            }
            if ch == '\'' && previous.is_some_and(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            let close = closing(ch)?;
            let selection = range.start + ch.len_utf8()..range.end + ch.len_utf8();
            Some(Assistance::Replace {
                text: format!("{ch}{}{close}", &text[range.clone()]),
                range,
                selection,
            })
        }
        Action::Backspace if range.is_empty() => {
            let open = previous?;
            if closing(open) != next {
                return None;
            }
            let close = next?;
            if text[..range.start].graphemes(true).next_back()?.len() != open.len_utf8()
                || text[range.end..].graphemes(true).next()?.len() != close.len_utf8()
            {
                return None;
            }
            let start = range.start - open.len_utf8();
            Some(Assistance::Replace {
                range: start..range.end + close.len_utf8(),
                text: String::new(),
                selection: start..start,
            })
        }
        _ => None,
    }
}
