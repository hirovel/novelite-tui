use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Command, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn row(b: &Buffer, y: u16) -> String {
    (0..b.area.width)
        .map(|x| b[(x, y)].symbol())
        .collect::<String>()
        .replace(' ', "")
}
#[test]
fn a_long_unicode_command_title_has_a_visible_truncation_marker() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.register_command(Command {
        id: "long-label".into(),
        title: "很长的中文标题👩‍💻e\u{301}".repeat(8),
        run: |_| Ok(novelite_tui::Outcome::Continue),
    })
    .unwrap();
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("long-label".into())).unwrap();
    let b = draw(&mut w, 32, 10);
    let y = (0..10).find(|&y| row(&b, y).contains('▎')).unwrap();
    assert!(row(&b, y).ends_with('…'));
    assert!(!row(&b, y).contains('�'));
    assert_eq!(w.text(), "");
}
