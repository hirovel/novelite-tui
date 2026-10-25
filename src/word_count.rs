/// The desktop counter's policy: BMP Han characters and ASCII word runs.
pub(crate) fn count(chars: impl Iterator<Item = char>) -> u64 {
    let mut total = 0;
    let mut in_word = false;
    for ch in chars {
        let word = ch.is_ascii_alphanumeric() || ch == '_';
        if matches!(ch, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{3007}' | '\u{f900}'..='\u{faff}')
            || (word && !in_word)
        {
            total += 1;
        }
        in_word = word;
    }
    total
}

/// One character on either side captures ASCII runs joined or split by an edit.
pub(crate) fn replace(
    text: &mut String,
    words: &mut u64,
    range: std::ops::Range<usize>,
    inserted: &str,
) {
    let left = text[..range.start].chars().next_back();
    let right = text[range.end..].chars().next();
    let before = count(
        left.into_iter()
            .chain(text[range.clone()].chars())
            .chain(right),
    );
    let after = count(left.into_iter().chain(inserted.chars()).chain(right));
    *words = *words - before + after;
    text.replace_range(range, inserted);
}
