use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, TextWidth, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn point(b: &Buffer, symbol: &str) -> (u16, u16) {
    let i = b.content.iter().position(|c| c.symbol() == symbol).unwrap();
    (
        (i % b.area.width as usize) as u16,
        (i / b.area.width as usize) as u16,
    )
}
#[test]
fn reading_width_and_full_width_keep_header_manuscript_and_tools_on_one_axis() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("稿.txt")).unwrap();
    w.dispatch(Action::Insert("正文第一行\n第二行".into()))
        .unwrap();
    for mode in [TextWidth::Adaptive, TextWidth::Full] {
        w.set_text_width(mode).unwrap();
        for width in [80, 120, 180] {
            let b = draw(&mut w, width, 32);
            let left = point(&b, "正").0;
            assert_eq!(point(&b, "稿").0, left);
            key(&mut w, KeyCode::F(1));
            let command = draw(&mut w, width, 32);
            assert_eq!(point(&command, "▎").0, left);
            assert_eq!(point(&command, "正").0, left);
            let command_y = point(&command, "令").1;
            w.open_design_settings().unwrap();
            let settings = draw(&mut w, width, 32);
            assert_eq!(point(&settings, "设").0, left + 1);
            assert_eq!(point(&settings, "设").1, command_y);
            key(&mut w, KeyCode::Tab);
            let layout = draw(&mut w, width, 32);
            assert_eq!(
                point(&layout, "设").1,
                point(&settings, "设").1,
                "classification must not resize the dock"
            );
            for y in 30..32 {
                for x in 0..left {
                    assert_eq!(
                        layout[(x, y)].symbol(),
                        " ",
                        "hidden footer must not leak into the margin"
                    );
                }
            }
            key(&mut w, KeyCode::Esc);
            assert_eq!(point(&draw(&mut w, width, 32), "正").0, left);
        }
    }
    assert_eq!(w.text(), "正文第一行\n第二行");
}
