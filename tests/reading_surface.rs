use novelite_tui::{Action, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn draw(w: &mut Workspace, width: u16, height: u16, message: &str) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, message)).unwrap();
    t.backend().buffer().clone()
}
fn text(b: &Buffer) -> String {
    b.content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .replace(' ', "")
}

#[test]
fn readonly_warning_survives_the_quiet_chrome() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book");
    let mut owner = Workspace::create_project(&book, "共享书稿").unwrap();
    owner.dispatch(Action::Insert("保留正文".into())).unwrap();
    owner.dispatch(Action::Save).unwrap();
    let mut reader = Workspace::open_project(&book).unwrap();
    let visible = text(&draw(&mut reader, 80, 24, ""));
    assert!(visible.contains("只读"));
    assert!(visible.contains("作品正在其他实例中打开"));
    assert!(visible.contains("保留正文"));
}
