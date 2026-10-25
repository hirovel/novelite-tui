/// Tracks fenced blocks while leaving each caller's treatment of other Markdown local.
#[derive(Clone, Default)]
pub(crate) struct Fence(Option<(char, usize)>);
impl Fence {
    pub fn protects(&mut self, line: &str) -> bool {
        let stripped = line.trim();
        let marker = stripped
            .chars()
            .next()
            .filter(|c| matches!(c, '`' | '~'))
            .map(|c| (c, stripped.chars().take_while(|v| *v == c).count()));
        if let Some((ch, size)) = self.0 {
            if marker.is_some_and(|(c, n)| c == ch && n >= size && stripped[n..].trim().is_empty())
            {
                self.0 = None;
            }
            true
        } else if let Some(marker) = marker.filter(|(_, n)| *n >= 3) {
            self.0 = Some(marker);
            true
        } else {
            false
        }
    }
}
